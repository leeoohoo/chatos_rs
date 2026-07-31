// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use super::{
    BeginPluginCloudOAuthAuthorizationRequest, BeginPluginCloudOAuthAuthorizationResponse,
    PluginArtifactCreateRequest, PluginArtifactReadMode, PluginArtifactReadRequest,
    PluginArtifactUpdateRequest, PluginUiBridgeMethod, PluginUiBridgeRequest,
    ResolvedPluginMcpCloudCredentials, UpdateUserPluginPreferenceResponse,
    UserPluginPreferenceRecord, PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1,
};

#[test]
fn resolved_cloud_credentials_debug_output_is_redacted() {
    let resolved = ResolvedPluginMcpCloudCredentials {
        credential_snapshot_sha256: "a".repeat(64),
        headers: BTreeMap::from([(
            "authorization".to_string(),
            "Bearer top-secret-token".to_string(),
        )]),
        environment: BTreeMap::from([("API_TOKEN".to_string(), "top-secret-token".to_string())]),
        oauth_connection_id: Some("oauth-1".to_string()),
    };
    let debug = format!("{resolved:?}");
    assert!(!debug.contains("top-secret-token"));
    assert!(debug.contains("header_count"));
    assert!(debug.contains("environment_count"));
}

#[test]
fn cloud_oauth_authorization_debug_output_redacts_client_secret() {
    let request = BeginPluginCloudOAuthAuthorizationRequest {
        provider: "figma".to_string(),
        scopes: vec!["files:read".to_string()],
        authorization_server: None,
        client_id: Some("client-1".to_string()),
        client_secret: Some("top-secret-client-value".to_string()),
        token_endpoint_auth_method: Some("client_secret_basic".to_string()),
    };
    let debug = format!("{request:?}");
    assert!(!debug.contains("top-secret-client-value"));
    assert!(debug.contains("[redacted]"));
}

#[test]
fn cloud_oauth_authorization_response_debug_output_redacts_state_url() {
    let response = BeginPluginCloudOAuthAuthorizationResponse {
        flow_id: "flow-1".to_string(),
        authorization_url: "https://auth.example.com/authorize?state=top-secret-state".to_string(),
        callback_origin: "https://plugins.example.com".to_string(),
        expires_at: "2026-08-01T00:10:00Z".to_string(),
    };
    let debug = format!("{response:?}");
    assert!(!debug.contains("top-secret-state"));
    assert!(debug.contains("[redacted]"));
}

#[test]
fn preference_update_response_preserves_the_authoritative_disable_transition() {
    let response = UpdateUserPluginPreferenceResponse {
        preference: UserPluginPreferenceRecord {
            owner_user_id: "owner-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            enabled: false,
            auto_update: false,
            release_channel: "stable".to_string(),
            enabled_components: Vec::new(),
            updated_at: "2026-07-26T00:00:00Z".to_string(),
        },
        previous_enabled: Some(true),
        disabled_transition: true,
    };

    let encoded = serde_json::to_value(&response).expect("serialize preference response");
    let decoded: UpdateUserPluginPreferenceResponse =
        serde_json::from_value(encoded).expect("deserialize preference response");
    assert_eq!(decoded, response);
}

#[test]
fn plugin_ui_bridge_request_uses_dotted_capability_names_and_closed_schema() {
    let request: PluginUiBridgeRequest = serde_json::from_value(serde_json::json!({
        "type": PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1,
        "protocol_version": 1,
        "adapter_session_id": "adapter-1",
        "host_session_nonce": "nonce-1",
        "request_id": "request-1",
        "method": "host.context.read",
        "payload": {}
    }))
    .expect("decode bridge request");
    assert_eq!(request.method, PluginUiBridgeMethod::HostContextRead);

    assert!(
        serde_json::from_value::<PluginUiBridgeRequest>(serde_json::json!({
            "type": PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1,
            "protocol_version": 1,
            "adapter_session_id": "adapter-1",
            "host_session_nonce": "nonce-1",
            "request_id": "request-1",
            "method": "host.context.read",
            "payload": {},
            "unexpected": true
        }))
        .is_err()
    );
}

#[test]
fn plugin_artifact_read_contract_is_closed_and_mode_scoped() {
    let request: PluginArtifactReadRequest = serde_json::from_value(serde_json::json!({
        "access": {
            "run_id": "run-1",
            "plugin_id": "plugin-1",
            "release_id": "release-1",
            "artifact_sha256": "a".repeat(64),
            "component_key": "workbench",
            "adapter_session_id": "ui-session-1",
            "ui_snapshot_sha256": "b".repeat(64)
        },
        "artifact_id": format!("pa_{}", "c".repeat(32)),
        "mode": "download"
    }))
    .expect("decode Artifact read request");
    assert_eq!(request.mode, PluginArtifactReadMode::Download);

    assert!(
        serde_json::from_value::<PluginArtifactReadRequest>(serde_json::json!({
            "access": {
                "run_id": "run-1",
                "plugin_id": "plugin-1",
                "release_id": "release-1",
                "artifact_sha256": "a".repeat(64),
                "component_key": "workbench",
                "adapter_session_id": "ui-session-1",
                "ui_snapshot_sha256": "b".repeat(64),
                "unexpected": true
            },
            "artifact_id": format!("pa_{}", "c".repeat(32)),
            "mode": "inline"
        }))
        .is_err()
    );
}

#[test]
fn plugin_artifact_write_contracts_are_closed_and_optimistic() {
    let access = serde_json::json!({
        "run_id": "run-1",
        "plugin_id": "plugin-1",
        "release_id": "release-1",
        "artifact_sha256": "a".repeat(64),
        "component_key": "workbench",
        "adapter_session_id": "ui-session-1",
        "ui_snapshot_sha256": "b".repeat(64)
    });
    let create = serde_json::from_value::<PluginArtifactCreateRequest>(serde_json::json!({
        "access": access,
        "display_name": "report.json",
        "media_type": "application/json",
        "body_base64": "e30="
    }))
    .expect("decode Artifact create request");
    assert_eq!(create.display_name, "report.json");

    let update = serde_json::from_value::<PluginArtifactUpdateRequest>(serde_json::json!({
        "access": create.access,
        "artifact_id": format!("pa_{}", "c".repeat(32)),
        "expected_sha256": "d".repeat(64),
        "body_base64": "eyJvayI6dHJ1ZX0="
    }))
    .expect("decode Artifact update request");
    assert_eq!(update.expected_sha256, "d".repeat(64));

    assert!(
        serde_json::from_value::<PluginArtifactUpdateRequest>(serde_json::json!({
            "access": update.access,
            "artifact_id": update.artifact_id,
            "expected_sha256": update.expected_sha256,
            "body_base64": update.body_base64,
            "overwrite": true
        }))
        .is_err()
    );
}
