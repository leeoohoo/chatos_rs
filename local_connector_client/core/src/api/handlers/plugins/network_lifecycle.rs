// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::path::PathBuf;

use axum::extract::State;
use axum::Json;
use chatos_plugin_management_sdk::{PluginInstallSource, PluginInstallSourceList};
use chrono::Utc;
use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::config::api_url;
use crate::plugins::{
    evaluate_auto_update, verify_plugin_install_source, PendingPluginInstall,
    PluginAutoUpdateDecision, PluginAutoUpdateState,
};
use crate::{tracing_stdout, LocalRuntime};

use super::super::super::types::LocalApiError;
use super::{install_network_plugin_source, LocalPluginAutoUpdateReport};

const PLUGIN_DOWNLOAD_PROGRESS_MIN_BYTES: u64 = 64 * 1024;
const PLUGIN_DOWNLOAD_PROGRESS_INTERVAL_MS: u64 = 250;
const PLUGIN_DOWNLOAD_PROGRESS_SLOW_INTERVAL_MS: u64 = 1_000;
const PLUGIN_AUTO_UPDATE_INTERVAL_SECS: u64 = 15 * 60;
const PLUGIN_AUTO_UPDATE_STARTUP_DELAY_SECS: u64 = 30;

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

pub(super) async fn reject_failed_plugin_download(
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

pub(super) async fn fetch_remote_plugin_sources(
    runtime: &LocalRuntime,
) -> anyhow::Result<Option<PluginInstallSourceList>> {
    let Some((service_base_url, access_token)) = plugin_service_auth(runtime).await else {
        return Ok(None);
    };
    let response = runtime
        .http_client
        .get(api_url(
            service_base_url.as_str(),
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

pub(super) async fn plugin_service_auth(runtime: &LocalRuntime) -> Option<(String, String)> {
    let state = runtime.state.read().await;
    state
        .auth
        .as_ref()
        .map(|auth| (auth.cloud_base_url.clone(), auth.access_token.clone()))
}

pub(super) async fn download_remote_plugin_artifact(
    runtime: &LocalRuntime,
    source: &PluginInstallSource,
    pending: &PendingPluginInstall,
    path: PathBuf,
) -> Result<DownloadedPluginArtifact, LocalApiError> {
    let (service_base_url, access_token) = plugin_service_auth(runtime).await.ok_or_else(|| {
        LocalApiError::bad_request("please login before downloading Marketplace Plugins")
    })?;
    let response = runtime
        .http_client
        .get(api_url(
            service_base_url.as_str(),
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

pub(super) async fn response_bytes_limited(
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

pub(super) struct DownloadedPluginArtifact {
    pub(super) path: PathBuf,
}

impl Drop for DownloadedPluginArtifact {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.path.as_path());
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}
