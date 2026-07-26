// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use chatos_plugin_management_sdk::PluginOAuthStatusSyncPayload;
use serde::Deserialize;
use serde_json::Value;

use crate::state::AppState;

const MAX_SOCKET_OAUTH_CONNECTIONS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SocketPluginOAuthStatusMessage {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(default)]
    items: Vec<SocketPluginOAuthStatusItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SocketPluginOAuthStatusItem {
    plugin_id: String,
    release_id: String,
    component_key: String,
    provider: String,
    #[serde(default)]
    scopes: Vec<String>,
    connected: bool,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    account_display: Option<String>,
}

pub(super) fn is_plugin_oauth_status_message(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|value| value == "plugin_oauth_status")
}

pub(super) async fn sync_socket_plugin_oauth_statuses(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    text: &str,
) -> Result<usize, String> {
    let payloads = decode_socket_payloads(owner_user_id, device_id, text)?;
    for payload in &payloads {
        state
            .plugin_management_client
            .sync_plugin_oauth_status(payload)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(payloads.len())
}

fn decode_socket_payloads(
    owner_user_id: &str,
    device_id: &str,
    text: &str,
) -> Result<Vec<PluginOAuthStatusSyncPayload>, String> {
    let message = serde_json::from_str::<SocketPluginOAuthStatusMessage>(text)
        .map_err(|error| format!("decode Plugin OAuth status failed: {error}"))?;
    if message.message_type != "plugin_oauth_status" {
        return Err("unexpected Plugin OAuth status message type".to_string());
    }
    if message.items.len() > MAX_SOCKET_OAUTH_CONNECTIONS {
        return Err(format!(
            "Plugin OAuth status exceeds {MAX_SOCKET_OAUTH_CONNECTIONS} items"
        ));
    }
    let mut identities = BTreeSet::new();
    let mut payloads = Vec::with_capacity(message.items.len());
    for item in message.items {
        let identity = format!(
            "{}\n{}\n{}",
            item.plugin_id, item.component_key, item.provider
        );
        if !identities.insert(identity) {
            return Err("Plugin OAuth status contains duplicate connection identities".to_string());
        }
        payloads.push(PluginOAuthStatusSyncPayload {
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            plugin_id: item.plugin_id,
            release_id: item.release_id,
            component_key: item.component_key,
            provider: item.provider,
            scopes: item.scopes,
            connected: item.connected,
            expires_at: item.connected.then_some(item.expires_at).flatten(),
            account_display: item.connected.then_some(item.account_display).flatten(),
        });
    }
    Ok(payloads)
}

#[cfg(test)]
mod tests {
    use super::decode_socket_payloads;

    #[test]
    fn socket_identity_is_injected_and_secret_fields_are_rejected() {
        let payloads = decode_socket_payloads(
            "trusted-owner",
            "trusted-device",
            r#"{
                "type":"plugin_oauth_status",
                "items":[{
                    "plugin_id":"plugin",
                    "release_id":"release",
                    "component_key":"app",
                    "provider":"demo",
                    "scopes":["read"],
                    "connected":true,
                    "expires_at":"2026-07-22T12:00:00Z",
                    "account_display":"Demo"
                }]
            }"#,
        )
        .expect("decode Plugin OAuth socket status");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].owner_user_id, "trusted-owner");
        assert_eq!(payloads[0].device_id, "trusted-device");

        let error = decode_socket_payloads(
            "trusted-owner",
            "trusted-device",
            r#"{
                "type":"plugin_oauth_status",
                "items":[{
                    "plugin_id":"plugin",
                    "release_id":"release",
                    "component_key":"app",
                    "provider":"demo",
                    "connected":true,
                    "access_token":"must-not-cross-relay"
                }]
            }"#,
        )
        .expect_err("secret-bearing OAuth status must be rejected");
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn disconnected_socket_status_drops_stale_account_metadata() {
        let payloads = decode_socket_payloads(
            "owner",
            "device",
            r#"{
                "type":"plugin_oauth_status",
                "items":[{
                    "plugin_id":"plugin",
                    "release_id":"release",
                    "component_key":"app",
                    "provider":"demo",
                    "connected":false,
                    "expires_at":"stale",
                    "account_display":"stale"
                }]
            }"#,
        )
        .expect("decode disconnected OAuth status");
        assert!(payloads[0].expires_at.is_none());
        assert!(payloads[0].account_display.is_none());
    }
}
