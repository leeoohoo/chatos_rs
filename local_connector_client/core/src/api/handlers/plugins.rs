// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::Json;
use chatos_plugin_management_sdk::{
    PluginInstallSource, PluginInstallSourceList, UpdateUserPluginPreferenceResponse,
    UserPluginPreferenceRecord,
};
use chrono::Utc;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::config::api_url;
use crate::mcp::repository::runtime_identity;
use crate::plugins::{
    evaluate_auto_update, local_plugin_store_snapshot, merge_auto_update_state,
    merge_network_plugin_sources, verify_plugin_install_source, ActivePluginInstallation,
    LocalPluginOAuthConnection, LocalPluginStatusSnapshot, LocalPluginStoreSnapshot,
    PendingPluginInstall, PluginAutoUpdateDecision, PluginAutoUpdateState,
    PluginCredentialMetadata, PluginCredentialScope, PluginInstallRequest,
    PluginOAuthAuthorizationStart, PluginRecoveryReport,
};
use crate::{tracing_stdout, LocalRuntime};

use super::super::types::{
    BeginPluginOAuthRequest, CompletePluginOAuthRequest, LocalApiError, PluginEventsQuery,
    UninstallPluginRequest, UpdatePluginPreferenceRequest, UpsertPluginCredentialRequest,
};

const PLUGIN_EVENT_POLL_INTERVAL_MS: u64 = 200;
const DEFAULT_PLUGIN_EVENT_TIMEOUT_MS: u64 = 25_000;
const MAX_PLUGIN_EVENT_TIMEOUT_MS: u64 = 30_000;
const PLUGIN_DOWNLOAD_PROGRESS_MIN_BYTES: u64 = 64 * 1024;
const PLUGIN_DOWNLOAD_PROGRESS_INTERVAL_MS: u64 = 250;
const PLUGIN_DOWNLOAD_PROGRESS_SLOW_INTERVAL_MS: u64 = 1_000;
const PLUGIN_AUTO_UPDATE_INTERVAL_SECS: u64 = 15 * 60;
const PLUGIN_AUTO_UPDATE_STARTUP_DELAY_SECS: u64 = 30;

#[derive(Debug, Serialize)]
pub(crate) struct LocalPluginStatusEvent {
    schema_version: u32,
    cursor: String,
    changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<LocalPluginStatusSnapshot>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct LocalPluginAutoUpdateReport {
    schema_version: u32,
    catalog_items: usize,
    eligible: usize,
    attempted: usize,
    updated: usize,
    deferred: usize,
    busy: usize,
    failures: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped_reason: Option<String>,
    #[serde(default)]
    errors: Vec<String>,
}

pub(crate) async fn local_plugin_status(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalPluginStatusSnapshot>, LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let mut snapshot = tokio::task::spawn_blocking(move || installer.status_snapshot())
        .await
        .map_err(|error| anyhow::anyhow!("join Plugin status task failed: {error}"))??;
    snapshot.runtime = runtime.plugin_runtime.telemetry_snapshot();
    Ok(Json(snapshot))
}

pub(crate) async fn local_plugin_events(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<PluginEventsQuery>,
) -> Result<Json<LocalPluginStatusEvent>, LocalApiError> {
    let requested_cursor = query
        .cursor
        .as_deref()
        .map(validate_plugin_event_cursor)
        .transpose()?;
    let timeout_ms = query
        .timeout_ms
        .unwrap_or(DEFAULT_PLUGIN_EVENT_TIMEOUT_MS)
        .clamp(1_000, MAX_PLUGIN_EVENT_TIMEOUT_MS);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        let installer = runtime.plugin_installer.clone();
        let mut snapshot = tokio::task::spawn_blocking(move || installer.status_snapshot())
            .await
            .map_err(|error| {
                anyhow::anyhow!("join Plugin event snapshot task failed: {error}")
            })??;
        snapshot.runtime = runtime.plugin_runtime.telemetry_snapshot();
        let cursor = plugin_status_cursor(&snapshot)?;
        if requested_cursor != Some(cursor.as_str()) {
            return Ok(Json(LocalPluginStatusEvent {
                schema_version: 1,
                cursor,
                changed: true,
                snapshot: Some(snapshot),
            }));
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(Json(LocalPluginStatusEvent {
                schema_version: 1,
                cursor,
                changed: false,
                snapshot: None,
            }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            PLUGIN_EVENT_POLL_INTERVAL_MS,
        ))
        .await;
    }
}

fn validate_plugin_event_cursor(value: &str) -> Result<&str, LocalApiError> {
    if value.len() != 64
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(LocalApiError::bad_request(
            "Plugin event cursor must be a lower-case SHA-256 digest",
        ));
    }
    Ok(value)
}

fn plugin_status_cursor(snapshot: &LocalPluginStatusSnapshot) -> anyhow::Result<String> {
    let payload = serde_json::to_vec(snapshot)?;
    Ok(hex::encode(Sha256::digest(payload)))
}

pub(crate) async fn local_plugin_catalog(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalPluginStoreSnapshot>, LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let mut status = tokio::task::spawn_blocking(move || installer.status_snapshot())
        .await
        .map_err(|error| anyhow::anyhow!("join Plugin catalog task failed: {error}"))??;
    status.runtime = runtime.plugin_runtime.telemetry_snapshot();
    let mut snapshot = local_plugin_store_snapshot(status)?;
    snapshot.bundled_install_available = bundled_plugin_root()
        .is_some_and(|root| root.is_dir() && root.join("plugin-bundle-index.json").is_file());
    for item in &mut snapshot.items {
        if item.install_source == "bundled" {
            item.install_available = snapshot.bundled_install_available;
        }
    }
    match fetch_remote_plugin_sources(&runtime).await {
        Ok(Some(sources)) => {
            if let Err(error) = merge_network_plugin_sources(&mut snapshot, sources) {
                snapshot.network_catalog_error = Some(error.to_string());
                snapshot.network_install_available = false;
            }
        }
        Ok(None) => {}
        Err(error) => {
            snapshot.network_catalog_error = Some(error.to_string());
            snapshot.network_install_available = false;
        }
    }
    match PluginAutoUpdateState::load(runtime.plugin_installer.plugin_root()) {
        Ok(state) => merge_auto_update_state(&mut snapshot, &state),
        Err(error) => {
            snapshot.auto_update_error = Some(error.to_string());
        }
    }
    Ok(Json(snapshot))
}

pub(crate) async fn local_install_plugin(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
) -> Result<Json<LocalPluginStatusSnapshot>, LocalApiError> {
    validate_plugin_id(plugin_id.as_str())?;
    if !plugin_id.starts_with("bundled-plugin-") {
        return local_install_network_plugin(runtime, plugin_id).await;
    }
    let bundled_root = bundled_plugin_root().ok_or_else(|| {
        LocalApiError::conflict(
            "Bundled Plugin resources are unavailable in this Local Connector installation",
        )
    })?;
    let installer = runtime.plugin_installer.clone();
    let snapshot = tokio::task::spawn_blocking(move || {
        installer.install_bundled_directory(bundled_root.as_path(), plugin_id.as_str())?;
        installer.status_snapshot()
    })
    .await
    .map_err(|error| anyhow::anyhow!("join bundled Plugin install task failed: {error}"))?
    .map_err(|error: anyhow::Error| LocalApiError::conflict(error.to_string()))?;
    Ok(Json(snapshot))
}

async fn local_install_network_plugin(
    runtime: LocalRuntime,
    plugin_id: String,
) -> Result<Json<LocalPluginStatusSnapshot>, LocalApiError> {
    let sources = fetch_remote_plugin_sources(&runtime)
        .await
        .map_err(|error| LocalApiError::bad_gateway(error.to_string()))?
        .ok_or_else(|| {
            LocalApiError::bad_request("please login before installing Marketplace Plugins")
        })?;
    let source = sources
        .items
        .into_iter()
        .find(|source| source.catalog.id == plugin_id)
        .ok_or_else(|| {
            LocalApiError::conflict("Plugin is not available from the trusted Marketplace Catalog")
        })?;
    verify_plugin_install_source(&source)
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    install_network_plugin_source(&runtime, source)
        .await
        .map(Json)
}

async fn install_network_plugin_source(
    runtime: &LocalRuntime,
    source: PluginInstallSource,
) -> Result<LocalPluginStatusSnapshot, LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let begin_source = source.clone();
    let pending = tokio::task::spawn_blocking(move || {
        installer.begin_network_install(
            &begin_source.marketplace,
            &begin_source.catalog,
            &begin_source.release,
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join network Plugin download transaction failed: {error}"))?
    .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let download_path = runtime
        .plugin_installer
        .plugin_root()
        .join(pending.relative_download_path.as_str());
    let artifact =
        match download_remote_plugin_artifact(runtime, &source, &pending, download_path).await {
            Ok(artifact) => artifact,
            Err(error) => {
                return Err(reject_failed_plugin_download(
                    runtime.plugin_installer.clone(),
                    pending,
                    error,
                )
                .await);
            }
        };
    let installer = runtime.plugin_installer.clone();
    let archive_path = artifact.path.clone();
    let snapshot = tokio::task::spawn_blocking(move || {
        installer.install_downloaded_archive(
            pending,
            PluginInstallRequest {
                marketplace: &source.marketplace,
                catalog: &source.catalog,
                release: &source.release,
                archive_path: archive_path.as_path(),
            },
        )?;
        installer.status_snapshot()
    })
    .await
    .map_err(|error| anyhow::anyhow!("join network Plugin install task failed: {error}"))?
    .map_err(|error: anyhow::Error| LocalApiError::conflict(error.to_string()))?;
    drop(artifact);
    Ok(snapshot)
}

pub(crate) async fn local_update_plugin_preference(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
    Json(request): Json<UpdatePluginPreferenceRequest>,
) -> Result<Json<UserPluginPreferenceRecord>, LocalApiError> {
    validate_plugin_id(plugin_id.as_str())?;
    if plugin_id.starts_with("bundled-plugin-") {
        return Err(LocalApiError::bad_request(
            "bundled Plugins are updated with the Local Connector application",
        ));
    }
    if request.auto_update == Some(true)
        && request.release_channel.as_deref().unwrap_or("stable") != "stable"
    {
        return Err(LocalApiError::bad_request(
            "automatic Plugin updates currently support only the stable release channel",
        ));
    }
    if request.auto_update == Some(true) {
        let installer = runtime.plugin_installer.clone();
        let plugin_id_for_check = plugin_id.clone();
        let installed = tokio::task::spawn_blocking(move || {
            Ok::<_, anyhow::Error>(
                installer
                    .registry()?
                    .plugins
                    .get(plugin_id_for_check.as_str())
                    .is_some_and(|plugin| plugin.active_version.is_some()),
            )
        })
        .await
        .map_err(|error| anyhow::anyhow!("join Plugin preference validation failed: {error}"))??;
        if !installed {
            return Err(LocalApiError::conflict(
                "automatic updates can be enabled only for an installed Plugin",
            ));
        }
    }
    let (cloud_base_url, access_token) = plugin_cloud_auth(&runtime).await.ok_or_else(|| {
        LocalApiError::bad_request("please login before changing Marketplace Plugin preferences")
    })?;
    let device_id = runtime
        .state
        .read()
        .await
        .device_id
        .clone()
        .ok_or_else(|| LocalApiError::conflict("Local Connector device is not registered"))?;
    let response = runtime
        .http_client
        .put(api_url(
            cloud_base_url.as_str(),
            format!(
                "/api/plugin-management/plugins/{}/preference",
                urlencoding::encode(plugin_id.as_str()),
            )
            .as_str(),
        ))
        .bearer_auth(access_token)
        .json(&json!({
            "device_id": device_id,
            "enabled": request.enabled,
            "auto_update": request.auto_update,
            "release_channel": request.release_channel,
            "enabled_components": request.enabled_components,
        }))
        .send()
        .await
        .map_err(|error| {
            LocalApiError::bad_gateway(format!(
                "update Marketplace Plugin preference failed: {error}"
            ))
        })?;
    let status = response.status();
    let body = response_bytes_limited(response, 256 * 1024)
        .await
        .map_err(|error| LocalApiError::bad_gateway(error.to_string()))?;
    if !status.is_success() {
        let message = serde_json::from_slice::<Value>(body.as_slice())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                format!(
                    "Marketplace Plugin preference returned status {}",
                    status.as_u16()
                )
            });
        return Err(LocalApiError::bad_gateway(message));
    }
    let update = serde_json::from_slice::<UpdateUserPluginPreferenceResponse>(body.as_slice())
        .map_err(|error| {
            LocalApiError::bad_gateway(format!(
                "decode Marketplace Plugin preference failed: {error}"
            ))
        })?;
    let preference = &update.preference;
    if preference.plugin_id != plugin_id {
        return Err(LocalApiError::bad_gateway(
            "Marketplace Plugin preference returned a mismatched Plugin identity",
        ));
    }
    if preference.enabled != request.enabled
        || update.disabled_transition
            != (update.previous_enabled == Some(true) && !preference.enabled)
    {
        return Err(LocalApiError::bad_gateway(
            "Marketplace Plugin preference returned an inconsistent enabled transition",
        ));
    }
    if !preference.enabled || !preference.auto_update {
        let mut state = PluginAutoUpdateState::load(runtime.plugin_installer.plugin_root())
            .map_err(|error| LocalApiError::conflict(error.to_string()))?;
        state.clear_retry(plugin_id.as_str());
        state
            .save(runtime.plugin_installer.plugin_root())
            .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    }
    if preference.enabled {
        runtime
            .plugin_runtime
            .mark_plugin_enabled(plugin_id.as_str());
    }
    if update.disabled_transition {
        let report = runtime
            .plugin_runtime
            .dispatch_plugin_disabled(plugin_id.as_str())
            .await;
        tracing_stdout(
            format!(
                "PluginDisabled lifecycle completed for {}: {} sessions cancelled, {} Hook dispatch errors",
                plugin_id,
                report.cancelled_sessions,
                report.errors.len()
            )
            .as_str(),
        );
    }
    Ok(Json(update.preference))
}

pub(crate) async fn local_check_plugin_updates(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalPluginAutoUpdateReport>, LocalApiError> {
    run_plugin_auto_update_cycle(&runtime).await.map(Json)
}

pub(crate) fn spawn_plugin_auto_update_checker(
    runtime: LocalRuntime,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let start = tokio::time::Instant::now()
            + std::time::Duration::from_secs(PLUGIN_AUTO_UPDATE_STARTUP_DELAY_SECS);
        let mut interval = tokio::time::interval_at(
            start,
            std::time::Duration::from_secs(PLUGIN_AUTO_UPDATE_INTERVAL_SECS),
        );
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match run_plugin_auto_update_cycle(&runtime).await {
                Ok(report) if report.updated > 0 => tracing_stdout(
                    format!(
                        "automatic Plugin update completed: {} updated, {} failed",
                        report.updated, report.failures
                    )
                    .as_str(),
                ),
                Ok(_) => {}
                Err(error) => tracing_stdout(
                    format!("automatic Plugin update check failed: {}", error.message()).as_str(),
                ),
            }
        }
    })
}

async fn run_plugin_auto_update_cycle(
    runtime: &LocalRuntime,
) -> Result<LocalPluginAutoUpdateReport, LocalApiError> {
    let _guard = runtime.plugin_auto_update_lock.lock().await;
    let mut report = LocalPluginAutoUpdateReport {
        schema_version: 1,
        ..LocalPluginAutoUpdateReport::default()
    };
    let Some(sources) = fetch_remote_plugin_sources(runtime)
        .await
        .map_err(|error| LocalApiError::bad_gateway(error.to_string()))?
    else {
        report.skipped_reason = Some("not_authenticated".to_string());
        return Ok(report);
    };
    report.catalog_items = sources.items.len();
    let installer = runtime.plugin_installer.clone();
    let status = tokio::task::spawn_blocking(move || installer.status_snapshot())
        .await
        .map_err(|error| {
            anyhow::anyhow!("join Plugin auto-update status task failed: {error}")
        })??;
    let mut state = PluginAutoUpdateState::load(runtime.plugin_installer.plugin_root())
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;

    for source in sources.items {
        let plugin_id = source.catalog.id.clone();
        let release_id = source.release.id.clone();
        let now = Utc::now();
        let decision =
            evaluate_auto_update(&source, &status, state.plugins.get(plugin_id.as_str()), now);
        match decision {
            PluginAutoUpdateDecision::Ineligible(_reason) => continue,
            PluginAutoUpdateDecision::UpToDate => {
                report.eligible += 1;
                state.mark_up_to_date(plugin_id.as_str(), release_id.as_str(), now);
            }
            PluginAutoUpdateDecision::Busy => {
                report.eligible += 1;
                report.busy += 1;
                state.mark_checked(plugin_id.as_str(), release_id.as_str(), now);
            }
            PluginAutoUpdateDecision::Deferred => {
                report.eligible += 1;
                report.deferred += 1;
                state.mark_checked(plugin_id.as_str(), release_id.as_str(), now);
            }
            PluginAutoUpdateDecision::Ready => {
                report.eligible += 1;
                report.attempted += 1;
                state.mark_checked(plugin_id.as_str(), release_id.as_str(), now);
                state
                    .save(runtime.plugin_installer.plugin_root())
                    .map_err(|error| LocalApiError::conflict(error.to_string()))?;
                match install_network_plugin_source(runtime, source).await {
                    Ok(_) => {
                        report.updated += 1;
                        state.mark_success(plugin_id.as_str(), release_id.as_str(), Utc::now());
                    }
                    Err(error) => {
                        report.failures += 1;
                        report
                            .errors
                            .push(format!("{plugin_id}: {}", error.message()));
                        state.mark_failure(
                            plugin_id.as_str(),
                            release_id.as_str(),
                            Utc::now(),
                            error.message(),
                        );
                    }
                }
            }
        }
        state
            .save(runtime.plugin_installer.plugin_root())
            .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    }
    Ok(report)
}

async fn reject_failed_plugin_download(
    installer: crate::plugins::PluginInstaller,
    pending: PendingPluginInstall,
    error: LocalApiError,
) -> LocalApiError {
    let message = error.message().to_string();
    match tokio::task::spawn_blocking(move || {
        installer.reject_pending_install(&pending, message.as_str())
    })
    .await
    {
        Ok(Ok(())) => error,
        Ok(Err(journal_error)) => LocalApiError::conflict(format!(
            "{}; persist rejected Plugin download failed: {journal_error}",
            error.message()
        )),
        Err(join_error) => LocalApiError::conflict(format!(
            "{}; join rejected Plugin download task failed: {join_error}",
            error.message()
        )),
    }
}

async fn fetch_remote_plugin_sources(
    runtime: &LocalRuntime,
) -> anyhow::Result<Option<PluginInstallSourceList>> {
    let Some((cloud_base_url, access_token)) = plugin_cloud_auth(runtime).await else {
        return Ok(None);
    };
    let response = runtime
        .http_client
        .get(api_url(
            cloud_base_url.as_str(),
            "/api/plugin-management/plugins/install-sources",
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("fetch trusted Plugin Catalog failed: {error}"))?;
    let status = response.status();
    let body = response_bytes_limited(response, 4 * 1024 * 1024).await?;
    if !status.is_success() {
        let message = serde_json::from_slice::<Value>(body.as_slice())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                format!("trusted Plugin Catalog returned status {}", status.as_u16())
            });
        anyhow::bail!(message);
    }
    let sources = serde_json::from_slice::<PluginInstallSourceList>(body.as_slice())
        .map_err(|error| anyhow::anyhow!("decode trusted Plugin Catalog failed: {error}"))?;
    let mut plugin_ids = BTreeSet::new();
    for source in &sources.items {
        if !plugin_ids.insert(source.catalog.id.as_str()) {
            anyhow::bail!("trusted Plugin Catalog contains a duplicate Plugin ID");
        }
        verify_plugin_install_source(source).map_err(|error| {
            anyhow::anyhow!("trusted Plugin Catalog verification failed: {error}")
        })?;
        if source
            .preference
            .as_ref()
            .is_some_and(|preference| preference.plugin_id != source.catalog.id)
        {
            anyhow::bail!("trusted Plugin Catalog contains a mismatched user preference");
        }
    }
    Ok(Some(sources))
}

async fn plugin_cloud_auth(runtime: &LocalRuntime) -> Option<(String, String)> {
    let state = runtime.state.read().await;
    state
        .auth
        .as_ref()
        .map(|auth| (auth.cloud_base_url.clone(), auth.access_token.clone()))
}

async fn download_remote_plugin_artifact(
    runtime: &LocalRuntime,
    source: &PluginInstallSource,
    pending: &PendingPluginInstall,
    path: PathBuf,
) -> Result<DownloadedPluginArtifact, LocalApiError> {
    let (cloud_base_url, access_token) = plugin_cloud_auth(runtime).await.ok_or_else(|| {
        LocalApiError::bad_request("please login before downloading Marketplace Plugins")
    })?;
    let response = runtime
        .http_client
        .get(api_url(
            cloud_base_url.as_str(),
            format!(
                "/api/plugin-management/plugins/{}/releases/{}/artifact",
                urlencoding::encode(source.catalog.id.as_str()),
                urlencoding::encode(source.release.id.as_str()),
            )
            .as_str(),
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| {
            LocalApiError::bad_gateway(format!("download Plugin artifact failed: {error}"))
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response_bytes_limited(response, 16 * 1024)
            .await
            .unwrap_or_default();
        let message = serde_json::from_slice::<Value>(body.as_slice())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                format!("Plugin artifact proxy returned status {}", status.as_u16())
            });
        return Err(LocalApiError::bad_gateway(message));
    }
    validate_artifact_proxy_header(
        response.headers(),
        "x-chatos-plugin-id",
        source.catalog.id.as_str(),
    )?;
    validate_artifact_proxy_header(
        response.headers(),
        "x-chatos-plugin-release-id",
        source.release.id.as_str(),
    )?;
    validate_artifact_proxy_header(
        response.headers(),
        "x-chatos-plugin-artifact-sha256",
        source.release.artifact_sha256.as_str(),
    )?;
    let limit = runtime.plugin_installer.archive_limits().max_archive_bytes;
    let expected_total = response.content_length();
    if expected_total.is_some_and(|length| length > limit) {
        return Err(LocalApiError::bad_gateway(
            "Plugin artifact exceeds the local download size limit",
        ));
    }
    let download_root = path
        .parent()
        .ok_or_else(|| LocalApiError::conflict("Plugin download path has no parent"))?;
    tokio::fs::create_dir_all(download_root)
        .await
        .map_err(|error| {
            LocalApiError::conflict(format!("create Plugin download directory failed: {error}"))
        })?;
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path.as_path())
        .await
        .map_err(|error| {
            LocalApiError::conflict(format!("create Plugin artifact file failed: {error}"))
        })?;
    let artifact = DownloadedPluginArtifact { path };
    persist_plugin_download_progress(runtime, pending, 0, expected_total).await?;
    let mut downloaded = 0_u64;
    let mut persisted_bytes = 0_u64;
    let mut persisted_at = tokio::time::Instant::now();
    let mut digest = Sha256::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            LocalApiError::bad_gateway(format!("read Plugin artifact download failed: {error}"))
        })?;
        downloaded = downloaded
            .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| LocalApiError::bad_gateway("Plugin artifact size overflow"))?;
        if downloaded > limit {
            return Err(LocalApiError::bad_gateway(
                "Plugin artifact exceeds the local download size limit",
            ));
        }
        digest.update(chunk.as_ref());
        file.write_all(chunk.as_ref()).await.map_err(|error| {
            LocalApiError::conflict(format!("write Plugin artifact failed: {error}"))
        })?;
        let elapsed = persisted_at.elapsed();
        let byte_delta = downloaded.saturating_sub(persisted_bytes);
        if (elapsed >= std::time::Duration::from_millis(PLUGIN_DOWNLOAD_PROGRESS_INTERVAL_MS)
            && byte_delta >= PLUGIN_DOWNLOAD_PROGRESS_MIN_BYTES)
            || elapsed
                >= std::time::Duration::from_millis(PLUGIN_DOWNLOAD_PROGRESS_SLOW_INTERVAL_MS)
        {
            persist_plugin_download_progress(runtime, pending, downloaded, expected_total).await?;
            persisted_bytes = downloaded;
            persisted_at = tokio::time::Instant::now();
        }
    }
    file.sync_all().await.map_err(|error| {
        LocalApiError::conflict(format!("sync Plugin artifact failed: {error}"))
    })?;
    drop(file);
    if expected_total.is_some_and(|total| total != downloaded) {
        return Err(LocalApiError::bad_gateway(
            "Plugin artifact byte count differs from the proxy Content-Length",
        ));
    }
    persist_plugin_download_progress(runtime, pending, downloaded, Some(downloaded)).await?;
    if hex::encode(digest.finalize()) != source.release.artifact_sha256 {
        return Err(LocalApiError::conflict(
            "downloaded Plugin artifact SHA-256 does not match the signed Release",
        ));
    }
    Ok(artifact)
}

async fn persist_plugin_download_progress(
    runtime: &LocalRuntime,
    pending: &PendingPluginInstall,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) -> Result<(), LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let pending = pending.clone();
    tokio::task::spawn_blocking(move || {
        installer.update_network_download_progress(&pending, downloaded_bytes, total_bytes)
    })
    .await
    .map_err(|error| {
        LocalApiError::conflict(format!(
            "join Plugin download progress update failed: {error}"
        ))
    })?
    .map_err(|error| {
        LocalApiError::conflict(format!("persist Plugin download progress failed: {error}"))
    })
}

fn validate_artifact_proxy_header(
    headers: &reqwest::header::HeaderMap,
    name: &'static str,
    expected: &str,
) -> Result<(), LocalApiError> {
    let actual = headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            LocalApiError::bad_gateway(format!("Plugin artifact proxy omitted {name}"))
        })?;
    if actual != expected {
        return Err(LocalApiError::bad_gateway(format!(
            "Plugin artifact proxy returned mismatched {name}"
        )));
    }
    Ok(())
}

async fn response_bytes_limited(
    response: reqwest::Response,
    limit: usize,
) -> anyhow::Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        anyhow::bail!("Plugin Marketplace response exceeds the size limit");
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if body.len().saturating_add(chunk.len()) > limit {
            anyhow::bail!("Plugin Marketplace response exceeds the size limit");
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

struct DownloadedPluginArtifact {
    path: PathBuf,
}

impl Drop for DownloadedPluginArtifact {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.path.as_path());
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{
        LocalPluginRegistry, PluginTransactionJournal, PluginTransactionOperation,
        PluginTransactionRecord,
    };
    use chatos_plugin_management_sdk::PluginInstallStatus;

    #[test]
    fn plugin_event_cursor_is_stable_and_tracks_journal_changes() {
        let mut snapshot = LocalPluginStatusSnapshot {
            registry: LocalPluginRegistry::default(),
            transactions: PluginTransactionJournal::default(),
            runtime: Default::default(),
        };
        let first = plugin_status_cursor(&snapshot).expect("first cursor");
        assert_eq!(
            first,
            plugin_status_cursor(&snapshot).expect("stable cursor")
        );
        snapshot.transactions.history.push(PluginTransactionRecord {
            transaction_id: "transaction".to_string(),
            operation: PluginTransactionOperation::Install,
            status: PluginInstallStatus::Rejected,
            plugin_id: "plugin".to_string(),
            release_id: Some("release".to_string()),
            from_version: None,
            target_version: Some("1.0.0".to_string()),
            relative_staging_path: None,
            relative_final_path: None,
            relative_storage_path: None,
            relative_trash_path: None,
            downloaded_bytes: 128,
            total_bytes: Some(256),
            started_at: "2026-07-25T00:00:00Z".to_string(),
            updated_at: "2026-07-25T00:00:01Z".to_string(),
            completed_at: Some("2026-07-25T00:00:01Z".to_string()),
            recovered_after_restart: false,
            last_error: Some("rejected".to_string()),
        });
        let with_transaction = plugin_status_cursor(&snapshot).expect("changed cursor");
        assert_ne!(first, with_transaction);
        snapshot
            .transactions
            .history
            .last_mut()
            .expect("transaction")
            .downloaded_bytes = 256;
        assert_ne!(
            with_transaction,
            plugin_status_cursor(&snapshot).expect("progress cursor")
        );
        let progress_cursor = plugin_status_cursor(&snapshot).expect("progress cursor");
        snapshot.runtime.revision = 1;
        assert_ne!(
            progress_cursor,
            plugin_status_cursor(&snapshot).expect("runtime cursor")
        );
    }

    #[test]
    fn plugin_event_cursor_rejects_unbounded_input() {
        assert!(validate_plugin_event_cursor("not-a-cursor").is_err());
        assert!(validate_plugin_event_cursor("A".repeat(64).as_str()).is_err());
        assert!(validate_plugin_event_cursor("a".repeat(64).as_str()).is_ok());
    }
}

pub(crate) async fn local_rollback_plugin(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
) -> Result<Json<LocalPluginStatusSnapshot>, LocalApiError> {
    validate_plugin_id(plugin_id.as_str())?;
    let installer = runtime.plugin_installer.clone();
    let snapshot = tokio::task::spawn_blocking(move || {
        installer.rollback(plugin_id.as_str())?;
        installer.status_snapshot()
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin rollback task failed: {error}"))?
    .map_err(|error: anyhow::Error| LocalApiError::conflict(error.to_string()))?;
    Ok(Json(snapshot))
}

pub(crate) async fn local_uninstall_plugin(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
    Json(request): Json<UninstallPluginRequest>,
) -> Result<Json<LocalPluginStatusSnapshot>, LocalApiError> {
    validate_plugin_id(plugin_id.as_str())?;
    if !request.acknowledge_plugin_data_removal {
        return Err(LocalApiError::bad_request(
            "Plugin uninstall requires explicit acknowledgement of local data removal",
        ));
    }
    let installer = runtime.plugin_installer.clone();
    let (uninstalled, snapshot) = tokio::task::spawn_blocking(move || {
        let uninstalled = installer.uninstall(plugin_id.as_str())?;
        Ok::<_, anyhow::Error>((uninstalled, installer.status_snapshot()?))
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin uninstall task failed: {error}"))?
    .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    if !uninstalled {
        return Err(LocalApiError::conflict("Plugin is not installed"));
    }
    Ok(Json(snapshot))
}

fn validate_plugin_id(plugin_id: &str) -> Result<(), LocalApiError> {
    if plugin_id.trim() != plugin_id || plugin_id.is_empty() || plugin_id.len() > 200 {
        return Err(LocalApiError::bad_request("Plugin ID is invalid"));
    }
    Ok(())
}

fn bundled_plugin_root() -> Option<std::path::PathBuf> {
    std::env::var_os("CHATOS_BUNDLED_PLUGINS_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

pub(crate) async fn local_recover_plugin_transactions(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<PluginRecoveryReport>, LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let report = tokio::task::spawn_blocking(move || installer.recover_incomplete_transactions())
        .await
        .map_err(|error| anyhow::anyhow!("join Plugin recovery task failed: {error}"))??;
    Ok(Json(report))
}

pub(crate) async fn local_plugin_credentials(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
) -> Result<Json<Vec<PluginCredentialMetadata>>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let installation = active_plugin_installation(&runtime, plugin_id.as_str()).await?;
    let release_id = installation.version.release_id;
    let vault = runtime.plugin_credentials.clone();
    let credentials = tokio::task::spawn_blocking(move || {
        vault.list(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
            release_id.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin credential list task failed: {error}"))??;
    Ok(Json(credentials))
}

pub(crate) async fn local_upsert_plugin_credential(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key, secret_name)): Path<(String, String, String)>,
    Json(request): Json<UpsertPluginCredentialRequest>,
) -> Result<Json<PluginCredentialMetadata>, LocalApiError> {
    if request.value.is_empty() || request.value.len() > 64 * 1024 {
        return Err(LocalApiError::bad_request(
            "Plugin credential must contain between 1 byte and 64 KiB",
        ));
    }
    let scope = active_credential_scope(&runtime, plugin_id, component_key, secret_name).await?;
    let vault = runtime.plugin_credentials.clone();
    let value = request.value.into_bytes();
    let metadata = tokio::task::spawn_blocking(move || {
        let mut value = value;
        let result = vault.upsert(&scope, value.as_slice());
        value.fill(0);
        result
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin credential write task failed: {error}"))??;
    Ok(Json(metadata))
}

pub(crate) async fn local_delete_plugin_credential(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key, secret_name)): Path<(String, String, String)>,
) -> Result<Json<Value>, LocalApiError> {
    let scope = active_credential_scope(&runtime, plugin_id, component_key, secret_name).await?;
    let vault = runtime.plugin_credentials.clone();
    let deleted = tokio::task::spawn_blocking(move || vault.delete(&scope))
        .await
        .map_err(|error| anyhow::anyhow!("join Plugin credential delete task failed: {error}"))??;
    Ok(Json(json!({ "deleted": deleted })))
}

pub(crate) async fn local_plugin_oauth_connections(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
) -> Result<Json<Vec<LocalPluginOAuthConnection>>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let broker = runtime.plugin_oauth.clone();
    let connections = tokio::task::spawn_blocking(move || {
        broker.list_connections(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin OAuth list task failed: {error}"))??;
    Ok(Json(connections))
}

pub(crate) async fn local_begin_plugin_oauth(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key)): Path<(String, String)>,
    Json(request): Json<BeginPluginOAuthRequest>,
) -> Result<Json<PluginOAuthAuthorizationStart>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let installation = active_plugin_installation(&runtime, plugin_id.as_str()).await?;
    let release_id = installation.version.release_id;
    let broker = runtime.plugin_oauth.clone();
    let open_browser = request.open_browser.unwrap_or(true);
    let redirect_uri = super::super::local_plugin_oauth_redirect_uri();
    if request
        .redirect_uri
        .as_deref()
        .is_some_and(|requested| requested != redirect_uri)
    {
        return Err(LocalApiError::bad_request(
            "Plugin OAuth redirect_uri must match the Local Connector callback",
        ));
    }
    let mut authorization = tokio::task::spawn_blocking(move || {
        broker.begin_authorization(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
            redirect_uri.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin OAuth start task failed: {error}"))??;
    if open_browser {
        match crate::external_url::open_external_url(authorization.authorization_url.as_str()).await
        {
            Ok(()) => authorization.browser_opened = true,
            Err(error) => {
                tracing_stdout(
                    format!("open Plugin OAuth authorization URL failed: {error}").as_str(),
                );
                authorization.browser_error =
                    Some("The system browser could not be opened automatically".to_string());
            }
        }
    }
    Ok(Json(authorization))
}

pub(crate) async fn local_complete_plugin_oauth(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CompletePluginOAuthRequest>,
) -> Result<Json<LocalPluginOAuthConnection>, LocalApiError> {
    complete_plugin_oauth(runtime, request).await
}

pub(crate) async fn local_complete_plugin_oauth_query(
    State(runtime): State<LocalRuntime>,
    Query(request): Query<CompletePluginOAuthRequest>,
) -> Result<Json<LocalPluginOAuthConnection>, LocalApiError> {
    complete_plugin_oauth(runtime, request).await
}

pub(crate) async fn local_disconnect_plugin_oauth(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key, provider)): Path<(String, String, String)>,
) -> Result<Json<Value>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let broker = runtime.plugin_oauth.clone();
    let disconnected = tokio::task::spawn_blocking(move || {
        broker.disconnect(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
            component_key.as_str(),
            provider.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin OAuth disconnect task failed: {error}"))??;
    Ok(Json(json!({ "disconnected": disconnected })))
}

async fn active_credential_scope(
    runtime: &LocalRuntime,
    plugin_id: String,
    component_key: String,
    secret_name: String,
) -> Result<PluginCredentialScope, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let installation = active_plugin_installation(runtime, plugin_id.as_str()).await?;
    if !installation
        .version
        .inventory
        .components
        .iter()
        .any(|component| component.component_key == component_key)
    {
        return Err(LocalApiError::bad_request(
            "Plugin credential component is not present in the active signed inventory",
        ));
    }
    PluginCredentialScope::new(
        owner_user_id,
        device_id,
        plugin_id,
        installation.version.release_id,
        component_key,
        secret_name,
    )
    .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

async fn complete_plugin_oauth(
    runtime: LocalRuntime,
    request: CompletePluginOAuthRequest,
) -> Result<Json<LocalPluginOAuthConnection>, LocalApiError> {
    if let Some(error) = request.error.as_deref() {
        if request.code.is_some() {
            return Err(LocalApiError::bad_request(
                "Plugin OAuth callback cannot contain both code and error",
            ));
        }
        let failure = runtime
            .plugin_oauth
            .consume_authorization_error(
                request.state.as_str(),
                error,
                request.error_description.as_deref(),
            )
            .map_err(plugin_oauth_callback_error)?;
        return Err(LocalApiError::conflict_code(failure.code, failure.message));
    }
    if request.error_description.is_some() {
        return Err(LocalApiError::bad_request(
            "Plugin OAuth callback error_description requires error",
        ));
    }
    let code = request
        .code
        .as_deref()
        .filter(|code| !code.trim().is_empty())
        .ok_or_else(|| LocalApiError::bad_request("Plugin OAuth callback requires code"))?;
    let connection = runtime
        .plugin_oauth
        .complete_authorization(request.state.as_str(), code)
        .await
        .map_err(plugin_oauth_callback_error)?;
    Ok(Json(connection))
}

fn plugin_oauth_callback_error(error: anyhow::Error) -> LocalApiError {
    let message = error.to_string();
    let code = if message.contains("Plugin OAuth state is invalid or expired") {
        "plugin_oauth_state_invalid"
    } else {
        "plugin_oauth_callback_failed"
    };
    LocalApiError::conflict_code(code, message)
}

async fn active_plugin_installation(
    runtime: &LocalRuntime,
    plugin_id: &str,
) -> Result<ActivePluginInstallation, LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let plugin_id = plugin_id.to_string();
    tokio::task::spawn_blocking(move || installer.active_installation(plugin_id.as_str()))
        .await
        .map_err(|error| anyhow::anyhow!("join active Plugin validation task failed: {error}"))??
        .ok_or_else(|| LocalApiError::conflict("Plugin is not installed and active"))
}
