// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::{json, Value};

use super::LocalPluginOAuthConnection;

#[derive(Serialize)]
struct SocketPluginOAuthStatusItem<'a> {
    plugin_id: &'a str,
    release_id: &'a str,
    component_key: &'a str,
    provider: &'a str,
    scopes: &'a [String],
    connected: bool,
    expires_at: Option<&'a str>,
    account_display: Option<&'a str>,
}

pub(crate) fn oauth_status_message(connections: &[LocalPluginOAuthConnection]) -> Value {
    let items = connections
        .iter()
        .map(|connection| SocketPluginOAuthStatusItem {
            plugin_id: connection.plugin_id.as_str(),
            release_id: connection.release_id.as_str(),
            component_key: connection.component_key.as_str(),
            provider: connection.provider.as_str(),
            scopes: connection.scopes.as_slice(),
            connected: connection.connected,
            expires_at: connection
                .connected
                .then_some(connection.expires_at.as_deref())
                .flatten(),
            account_display: connection
                .connected
                .then_some(connection.account_display.as_deref())
                .flatten(),
        })
        .collect::<Vec<_>>();
    json!({
        "type": "plugin_oauth_status",
        "items": items,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::oauth_status_message;
    use crate::plugins::LocalPluginOAuthConnection;

    #[test]
    fn oauth_status_message_contains_only_non_sensitive_connection_state() {
        let message = oauth_status_message(&[LocalPluginOAuthConnection {
            id: "local-connection-id".to_string(),
            owner_user_id: "owner".to_string(),
            device_id: "device".to_string(),
            plugin_id: "plugin".to_string(),
            release_id: "release".to_string(),
            component_key: "app".to_string(),
            provider: "provider".to_string(),
            resource: "https://api.example.com".to_string(),
            scopes: vec!["read".to_string()],
            connected: true,
            needs_auth: false,
            expires_at: Some("2026-07-22T12:00:00Z".to_string()),
            account_display: Some("Demo Account".to_string()),
            updated_at: "2026-07-22T11:00:00Z".to_string(),
        }]);
        assert_eq!(
            message.get("type").and_then(Value::as_str),
            Some("plugin_oauth_status")
        );
        let item = message
            .pointer("/items/0")
            .and_then(Value::as_object)
            .expect("OAuth status item");
        let keys = item.keys().cloned().collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            [
                "account_display",
                "component_key",
                "connected",
                "expires_at",
                "plugin_id",
                "provider",
                "release_id",
                "scopes",
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        );
        let serialized = message.to_string();
        for forbidden in [
            "access_token",
            "refresh_token",
            "code_verifier",
            "vault",
            "resource",
            "owner",
            "device",
            "local-connection-id",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }
}
