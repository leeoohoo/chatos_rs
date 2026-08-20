// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use super::*;

pub(super) async fn list_installed_plugins(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<PluginInstalledQuery>,
) -> Result<Json<ListResponse<PluginInstallationRecord>>, ApiError> {
    let device_id = required_text(query.device_id.as_deref(), "device_id")?;
    let items = state
        .store
        .list_plugin_installations(user.effective_owner_user_id(), device_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn sync_plugin_installation_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<PluginInstallationSyncPayload>,
) -> Result<Json<PluginInstallationRecord>, ApiError> {
    let identity =
        require_local_connector_internal_request(&state, &headers, PLUGIN_INSTALL_MANAGE_SCOPE)?;
    normalize_installation_payload(&mut payload)?;
    let mut internal_audit = PluginManagementInternalAuditGuard::new(
        &identity,
        Some(payload.owner_user_id.as_str()),
        "plugin_installation",
        payload.plugin_id.as_str(),
        "sync",
    );
    internal_audit.resource_name(Some(payload.platform.as_str()));
    let plugin = state
        .store
        .get_plugin_catalog_entry(payload.plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin not found"))?;
    let release = state
        .store
        .get_plugin_release(payload.release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin release not found"))?;
    validate_installation_release(&payload, &plugin, &release)?;
    if let Some(previous_release_id) = payload.previous_release_id.as_deref() {
        let previous = state
            .store
            .get_plugin_release(previous_release_id)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::bad_request("previous Plugin release not found"))?;
        if previous.plugin_id != plugin.id {
            return Err(ApiError::bad_request(
                "previous Plugin release belongs to a different Plugin",
            ));
        }
    }
    validate_component_statuses(&payload.component_statuses, &release)?;
    let existing = state
        .store
        .get_plugin_installation(
            payload.owner_user_id.as_str(),
            payload.device_id.as_str(),
            payload.plugin_id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    let now = now_rfc3339();
    let record = PluginInstallationRecord {
        id: existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}:{}:{}",
                    payload.owner_user_id, payload.device_id, payload.plugin_id
                )
            }),
        owner_user_id: payload.owner_user_id.clone(),
        device_id: payload.device_id.clone(),
        plugin_id: payload.plugin_id.clone(),
        release_id: payload.release_id,
        version: payload.version,
        artifact_sha256: payload.artifact_sha256,
        platform: payload.platform,
        install_status: payload.install_status,
        availability_status: payload.availability_status,
        dependency_status: payload.dependency_status,
        permission_status: payload.permission_status,
        auth_status: payload.auth_status,
        component_statuses: payload.component_statuses,
        active: payload.active,
        previous_release_id: payload.previous_release_id,
        installed_at: existing
            .as_ref()
            .map(|record| record.installed_at.clone())
            .or(payload.installed_at)
            .unwrap_or_else(|| now.clone()),
        last_checked_at: now.clone(),
        last_error: payload
            .last_error
            .as_deref()
            .and_then(|value| normalized(Some(value)))
            .map(|value| truncate_text(value.as_str(), 1000)),
    };
    state
        .store
        .replace_plugin_installation(&record)
        .await
        .map_err(ApiError::internal)?;
    if state
        .store
        .get_user_plugin_preference(record.owner_user_id.as_str(), record.plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .is_none()
    {
        state
            .store
            .replace_user_plugin_preference(&UserPluginPreferenceRecord {
                owner_user_id: record.owner_user_id.clone(),
                plugin_id: record.plugin_id.clone(),
                enabled: record.active,
                auto_update: record.plugin_id.starts_with("bundled-plugin-"),
                release_channel: release.release_channel.clone(),
                enabled_components: Vec::new(),
                updated_at: now.clone(),
            })
            .await
            .map_err(ApiError::internal)?;
    }
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_SYNC_INSTALLATION,
        record.owner_user_id.as_str(),
        Some(record.device_id.as_str()),
        record.plugin_id.as_str(),
        Some(record.release_id.as_str()),
        "success",
        BTreeMap::from([
            ("active".to_string(), json!(record.active)),
            (
                "availability_status".to_string(),
                json!(record.availability_status),
            ),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    internal_audit.succeeded();
    Ok(Json(record))
}

fn normalize_installation_payload(
    payload: &mut PluginInstallationSyncPayload,
) -> Result<(), ApiError> {
    payload.owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    payload.device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    payload.plugin_id = required_text(Some(payload.plugin_id.as_str()), "plugin_id")?;
    payload.release_id = required_text(Some(payload.release_id.as_str()), "release_id")?;
    payload.version = required_text(Some(payload.version.as_str()), "version")?;
    payload.artifact_sha256 =
        normalize_sha256(payload.artifact_sha256.as_str(), "artifact_sha256")?;
    payload.platform = required_text(Some(payload.platform.as_str()), "platform")?;
    payload.previous_release_id = payload
        .previous_release_id
        .as_deref()
        .and_then(|value| normalized(Some(value)));
    payload.installed_at = payload
        .installed_at
        .as_deref()
        .and_then(|value| normalized(Some(value)));
    if payload.active && payload.install_status != PluginInstallStatus::Installed {
        return Err(ApiError::bad_request(
            "active Plugin installation must have installed status",
        ));
    }
    Ok(())
}

fn validate_installation_release(
    payload: &PluginInstallationSyncPayload,
    plugin: &PluginCatalogRecord,
    release: &PluginReleaseRecord,
) -> Result<(), ApiError> {
    if release.plugin_id != plugin.id || payload.plugin_id != plugin.id {
        return Err(ApiError::bad_request(
            "Plugin installation identity does not match release",
        ));
    }
    if payload.version != release.version || payload.artifact_sha256 != release.artifact_sha256 {
        return Err(ApiError::conflict(
            "Plugin installation version or artifact hash does not match release",
        ));
    }
    if !release.supported_platforms.is_empty()
        && !release
            .supported_platforms
            .iter()
            .any(|platform| platform == &payload.platform)
    {
        return Err(ApiError::conflict(
            "Plugin installation platform is not supported by release",
        ));
    }
    if payload.active && release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "revoked Plugin release cannot be activated",
        ));
    }
    Ok(())
}

fn validate_component_statuses(
    statuses: &[PluginComponentStatus],
    release: &PluginReleaseRecord,
) -> Result<(), ApiError> {
    let known = release
        .components
        .iter()
        .map(|component| (component.component_key.as_str(), component.kind))
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen = HashSet::new();
    for status in statuses {
        let Some(kind) = known.get(status.component_key.as_str()) else {
            return Err(ApiError::bad_request(format!(
                "unknown Plugin component status: {}",
                status.component_key
            )));
        };
        if *kind != status.kind {
            return Err(ApiError::bad_request(format!(
                "Plugin component kind mismatch: {}",
                status.component_key
            )));
        }
        if !seen.insert(status.component_key.as_str()) {
            return Err(ApiError::bad_request(format!(
                "duplicate Plugin component status: {}",
                status.component_key
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_installations_require_installed_state() {
        let mut payload = installation_payload();
        payload.active = true;
        payload.install_status = PluginInstallStatus::Updating;
        assert!(normalize_installation_payload(&mut payload).is_err());
        payload.install_status = PluginInstallStatus::Installed;
        assert!(normalize_installation_payload(&mut payload).is_ok());
    }

    fn installation_payload() -> PluginInstallationSyncPayload {
        PluginInstallationSyncPayload {
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            platform: "macos-arm64".to_string(),
            install_status: PluginInstallStatus::Installed,
            availability_status: PluginAvailabilityStatus::Ready,
            dependency_status: PluginRequirementStatus::Satisfied,
            permission_status: PluginRequirementStatus::Satisfied,
            auth_status: PluginRequirementStatus::Satisfied,
            component_statuses: Vec::new(),
            active: false,
            previous_release_id: None,
            installed_at: None,
            last_error: None,
        }
    }
}
