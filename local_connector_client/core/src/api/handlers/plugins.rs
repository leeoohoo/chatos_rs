// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::Json;
use chatos_plugin_management_sdk::{
    PluginComponentKind, PluginInstallSource, UpdateUserPluginPreferenceResponse,
    UserPluginPreferenceRecord,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::api_url;
use crate::local_runtime::sync_local_capability_snapshots;
use crate::plugins::{
    local_plugin_store_snapshot, merge_auto_update_state, merge_network_plugin_sources,
    verify_plugin_install_source, LocalPluginStatusSnapshot, LocalPluginStoreSnapshot,
    PluginAutoUpdateState, PluginInstallRequest, PluginRecoveryReport,
};
use crate::skills::{sync_skill_inventory, update_user_skill_preference};
use crate::{tracing_stdout, LocalRuntime};

use super::super::types::{
    LocalApiError, PluginEventsQuery, UninstallPluginRequest, UpdatePluginPreferenceRequest,
};

mod credential_oauth;
mod network_lifecycle;

pub(crate) use credential_oauth::{
    local_begin_plugin_oauth, local_complete_plugin_oauth, local_complete_plugin_oauth_query,
    local_delete_plugin_credential, local_disconnect_plugin_oauth, local_plugin_credentials,
    local_plugin_oauth_connections, local_upsert_plugin_credential,
};
use network_lifecycle::{
    download_remote_plugin_artifact, fetch_remote_plugin_sources, plugin_service_auth,
    reject_failed_plugin_download, response_bytes_limited,
};
pub(crate) use network_lifecycle::{local_check_plugin_updates, spawn_plugin_auto_update_checker};

const PLUGIN_EVENT_POLL_INTERVAL_MS: u64 = 200;
const DEFAULT_PLUGIN_EVENT_TIMEOUT_MS: u64 = 25_000;
const MAX_PLUGIN_EVENT_TIMEOUT_MS: u64 = 30_000;

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
    let plugin_id_for_install = plugin_id.clone();
    let snapshot = tokio::task::spawn_blocking(move || {
        installer
            .install_bundled_directory(bundled_root.as_path(), plugin_id_for_install.as_str())?;
        installer.status_snapshot()
    })
    .await
    .map_err(|error| anyhow::anyhow!("join bundled Plugin install task failed: {error}"))?
    .map_err(|error: anyhow::Error| LocalApiError::conflict(error.to_string()))?;
    publish_installed_plugin_skills(&runtime, &snapshot, plugin_id.as_str()).await;
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
        .map(|snapshot| {
            let runtime = runtime.clone();
            let plugin_id = plugin_id.clone();
            let snapshot_for_publish = snapshot.clone();
            tokio::spawn(async move {
                publish_installed_plugin_skills(
                    &runtime,
                    &snapshot_for_publish,
                    plugin_id.as_str(),
                )
                .await;
            });
            Json(snapshot)
        })
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

async fn publish_installed_plugin_skills(
    runtime: &LocalRuntime,
    snapshot: &LocalPluginStatusSnapshot,
    plugin_id: &str,
) {
    let skill_ids = installed_plugin_skill_ids(snapshot, plugin_id);
    if skill_ids.is_empty() {
        return;
    }
    if let Err(error) = sync_skill_inventory(runtime).await {
        tracing_stdout(
            format!("post-install Skill inventory sync failed for {plugin_id}: {error}").as_str(),
        );
    }
    let mut enabled = 0usize;
    for skill_id in &skill_ids {
        match update_user_skill_preference(runtime, skill_id.as_str(), true).await {
            Ok(_) => enabled += 1,
            Err(error) => tracing_stdout(
                format!(
                    "post-install Skill preference enable skipped for {plugin_id}/{skill_id}: {error}"
                )
                .as_str(),
            ),
        }
    }
    if enabled > 0 {
        match sync_local_capability_snapshots(runtime).await {
            Ok(_) => tracing_stdout(
                format!(
                    "post-install enabled {enabled}/{} Skills for {plugin_id} and refreshed capability snapshots",
                    skill_ids.len()
                )
                .as_str(),
            ),
            Err(error) => tracing_stdout(
                format!("post-install capability refresh failed for {plugin_id}: {error}")
                    .as_str(),
            ),
        }
    }
}

fn installed_plugin_skill_ids(
    snapshot: &LocalPluginStatusSnapshot,
    plugin_id: &str,
) -> Vec<String> {
    let Some(plugin) = snapshot.registry.plugins.get(plugin_id) else {
        return Vec::new();
    };
    let Some(active_version) = plugin.active_version.as_deref() else {
        return Vec::new();
    };
    let Some(version) = plugin.versions.get(active_version) else {
        return Vec::new();
    };
    let mut ids = BTreeSet::new();
    for component in &version.inventory.components {
        if component.kind != PluginComponentKind::SkillCollection {
            continue;
        }
        if let Some(skill_id) = component
            .metadata
            .get("skill_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            ids.insert(skill_id.to_string());
        }
    }
    ids.into_iter().collect()
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
    let (service_base_url, access_token) =
        plugin_service_auth(&runtime).await.ok_or_else(|| {
            LocalApiError::bad_request(
                "please login before changing Marketplace Plugin preferences",
            )
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
            service_base_url.as_str(),
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
    body: Bytes,
) -> Result<Json<LocalPluginStatusSnapshot>, LocalApiError> {
    validate_plugin_id(plugin_id.as_str())?;
    let request = parse_uninstall_plugin_request(&body)?;
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

fn parse_uninstall_plugin_request(body: &[u8]) -> Result<UninstallPluginRequest, LocalApiError> {
    if body.is_empty() {
        return Ok(UninstallPluginRequest {
            acknowledge_plugin_data_removal: false,
        });
    }
    serde_json::from_slice::<UninstallPluginRequest>(body).map_err(|error| {
        LocalApiError::bad_request(format!(
            "Plugin uninstall request body must be JSON with acknowledge_plugin_data_removal=true: {error}"
        ))
    })
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

    #[test]
    fn parse_uninstall_plugin_request_accepts_explicit_acknowledgement() {
        let request =
            parse_uninstall_plugin_request(br#"{"acknowledge_plugin_data_removal":true}"#)
                .expect("request");
        assert!(request.acknowledge_plugin_data_removal);
    }

    #[test]
    fn parse_uninstall_plugin_request_returns_json_api_error_for_invalid_body() {
        let error = parse_uninstall_plugin_request(b"Failed to parse").expect_err("error");
        assert!(error
            .message()
            .contains("Plugin uninstall request body must be JSON"));
    }
}
