// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

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
    for payload in &payloads {
        state
            .plugin_management_client
            .sync_plugin_installation(payload)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(payloads.len())
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
    use super::decode_socket_payloads;

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
}
