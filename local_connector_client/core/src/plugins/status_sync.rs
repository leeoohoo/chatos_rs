// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginAvailabilityStatus, PluginComponentStatus, PluginInstallStatus,
    PluginInstallationSyncPayload, PluginRequirementStatus,
};
use chrono::Utc;
use serde_json::{json, Value};

use super::LocalPluginStatusSnapshot;

pub(crate) fn installation_status_message(snapshot: &LocalPluginStatusSnapshot) -> Value {
    let checked_at = Utc::now().to_rfc3339();
    let items = snapshot
        .registry
        .plugins
        .values()
        .filter_map(|plugin| {
            let active_version = plugin.active_version.as_deref()?;
            let version = plugin.versions.get(active_version)?;
            let component_statuses = version
                .inventory
                .components
                .iter()
                .map(|component| PluginComponentStatus {
                    component_key: component.component_key.clone(),
                    kind: component.kind,
                    availability_status: PluginAvailabilityStatus::Ready,
                    last_error: None,
                    last_checked_at: checked_at.clone(),
                })
                .collect();
            Some(PluginInstallationSyncPayload {
                owner_user_id: String::new(),
                device_id: String::new(),
                plugin_id: plugin.plugin_id.clone(),
                release_id: version.release_id.clone(),
                version: version.version.clone(),
                artifact_sha256: version.artifact_sha256.clone(),
                platform: local_platform().to_string(),
                install_status: PluginInstallStatus::Installed,
                availability_status: PluginAvailabilityStatus::Ready,
                dependency_status: PluginRequirementStatus::Satisfied,
                permission_status: PluginRequirementStatus::Satisfied,
                auth_status: PluginRequirementStatus::Satisfied,
                component_statuses,
                active: true,
                previous_release_id: plugin.previous_version.as_deref().and_then(|previous| {
                    plugin
                        .versions
                        .get(previous)
                        .map(|version| version.release_id.clone())
                }),
                installed_at: Some(version.installed_at.clone()),
                last_error: None,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "type": "plugin_installation_status",
        "items": items,
    })
}

fn local_platform() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x86_64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-arm64"
    } else {
        "unsupported"
    }
}
