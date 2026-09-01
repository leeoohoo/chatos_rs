// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::future::Future;

use chatos_plugin_management_sdk::PluginInstallationSyncPayload;
use serde::Deserialize;
use serde_json::Value;

use crate::state::AppState;

const MAX_SOCKET_PLUGIN_INSTALLATIONS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SocketPluginInstallationStatusMessage {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(default)]
    items: Vec<PluginInstallationSyncPayload>,
}

pub(super) fn is_plugin_installation_status_message(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|value| value == "plugin_installation_status")
}

pub(super) async fn sync_socket_plugin_installations(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    text: &str,
) -> Result<usize, String> {
    let payloads = decode_socket_payloads(owner_user_id, device_id, text)?;
    let summary = sync_plugin_installation_payloads(&payloads, |payload| async move {
        state
            .plugin_management_client
            .sync_plugin_installation(&payload)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await;
    for rejected in &summary.rejected {
        tracing::warn!(
            owner_user_id,
            device_id,
            plugin_id = rejected.plugin_id.as_str(),
            error = rejected.error.as_str(),
            "Plugin installation status item was rejected without blocking the remaining batch"
        );
    }
    Ok(summary.accepted)
}

#[derive(Debug, PartialEq, Eq)]
struct PluginInstallationSyncRejection {
    plugin_id: String,
    error: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PluginInstallationSyncSummary {
    accepted: usize,
    rejected: Vec<PluginInstallationSyncRejection>,
}

async fn sync_plugin_installation_payloads<F, Fut>(
    payloads: &[PluginInstallationSyncPayload],
    mut sync: F,
) -> PluginInstallationSyncSummary
where
    F: FnMut(PluginInstallationSyncPayload) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let mut summary = PluginInstallationSyncSummary::default();
    for payload in payloads {
        match sync(payload.clone()).await {
            Ok(()) => summary.accepted += 1,
            Err(error) => summary.rejected.push(PluginInstallationSyncRejection {
                plugin_id: payload.plugin_id.clone(),
                error,
            }),
        }
    }
    summary
}

fn decode_socket_payloads(
    owner_user_id: &str,
    device_id: &str,
    text: &str,
) -> Result<Vec<PluginInstallationSyncPayload>, String> {
    let mut message = serde_json::from_str::<SocketPluginInstallationStatusMessage>(text)
        .map_err(|error| format!("decode Plugin installation status failed: {error}"))?;
    if message.message_type != "plugin_installation_status" {
        return Err("unexpected Plugin installation status message type".to_string());
    }
    if message.items.len() > MAX_SOCKET_PLUGIN_INSTALLATIONS {
        return Err(format!(
            "Plugin installation status exceeds {MAX_SOCKET_PLUGIN_INSTALLATIONS} items"
        ));
    }
    let mut plugin_ids = BTreeSet::new();
    for item in &mut message.items {
        if !plugin_ids.insert(item.plugin_id.clone()) {
            return Err("Plugin installation status contains duplicate Plugin IDs".to_string());
        }
        item.owner_user_id = owner_user_id.to_string();
        item.device_id = device_id.to_string();
    }
    Ok(message.items)
}

#[cfg(test)]
mod tests {
    use super::{decode_socket_payloads, sync_plugin_installation_payloads};

    #[test]
    fn socket_identity_is_injected() {
        let payloads = decode_socket_payloads(
            "trusted-owner",
            "trusted-device",
            r#"{
                "type":"plugin_installation_status",
                "items":[{
                    "owner_user_id":"untrusted-owner",
                    "device_id":"untrusted-device",
                    "plugin_id":"plugin-1",
                    "release_id":"release-1",
                    "version":"1.0.0",
                    "artifact_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "platform":"macos-arm64",
                    "install_status":"installed",
                    "availability_status":"ready",
                    "dependency_status":"satisfied",
                    "permission_status":"satisfied",
                    "auth_status":"satisfied",
                    "component_statuses":[],
                    "active":true
                }]
            }"#,
        )
        .expect("decode Plugin installation status");

        assert_eq!(payloads[0].owner_user_id, "trusted-owner");
        assert_eq!(payloads[0].device_id, "trusted-device");
    }

    #[tokio::test]
    async fn one_rejected_plugin_does_not_block_later_installations() {
        let payloads = decode_socket_payloads(
            "trusted-owner",
            "trusted-device",
            r#"{
                "type":"plugin_installation_status",
                "items":[
                    {
                        "owner_user_id":"untrusted-owner",
                        "device_id":"untrusted-device",
                        "plugin_id":"computer-use-old-id",
                        "release_id":"release-1",
                        "version":"1.0.0",
                        "artifact_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "platform":"macos-arm64",
                        "install_status":"installed",
                        "availability_status":"ready",
                        "dependency_status":"satisfied",
                        "permission_status":"satisfied",
                        "auth_status":"satisfied",
                        "component_statuses":[],
                        "active":true
                    },
                    {
                        "owner_user_id":"untrusted-owner",
                        "device_id":"untrusted-device",
                        "plugin_id":"browser-current-id",
                        "release_id":"release-2",
                        "version":"1.0.0",
                        "artifact_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "platform":"macos-arm64",
                        "install_status":"installed",
                        "availability_status":"ready",
                        "dependency_status":"satisfied",
                        "permission_status":"satisfied",
                        "auth_status":"satisfied",
                        "component_statuses":[],
                        "active":true
                    }
                ]
            }"#,
        )
        .expect("decode Plugin installation status");

        let summary = sync_plugin_installation_payloads(&payloads, |payload| {
            let plugin_id = payload.plugin_id.clone();
            async move {
                if plugin_id == "computer-use-old-id" {
                    Err("Plugin not found".to_string())
                } else {
                    Ok(())
                }
            }
        })
        .await;

        assert_eq!(summary.accepted, 1);
        assert_eq!(summary.rejected.len(), 1);
        assert_eq!(summary.rejected[0].plugin_id, "computer-use-old-id");
    }
}
