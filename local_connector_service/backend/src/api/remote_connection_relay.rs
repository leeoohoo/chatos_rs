// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::Response;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::CurrentUser;
use crate::relay::RelayRequest;
use crate::state::AppState;

use super::{
    dispatch_relay, relay_response_to_http, required_text, validate_device_workspace, ApiError,
};

#[derive(Debug, Deserialize)]
pub(super) struct RemoteConnectionTestRelayRequest {
    workspace_id: Option<String>,
    connection: Option<Value>,
    verification_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteConnectionCommandRelayRequest {
    workspace_id: Option<String>,
    connection: Option<Value>,
    command: Option<String>,
    timeout_ms: Option<u64>,
    verification_code: Option<String>,
}

pub(super) async fn remote_connection_test_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<RemoteConnectionTestRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let connection = req
        .connection
        .filter(Value::is_object)
        .ok_or_else(|| ApiError::bad_request("connection is required"))?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "remote_connection_test_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/remote-connections/test".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "connection": connection,
            "verification_code": req.verification_code,
        }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let relay_timeout = state
        .config
        .relay_request_timeout
        .max(Duration::from_secs(20));
    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn remote_connection_command_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<RemoteConnectionCommandRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let connection = req
        .connection
        .filter(Value::is_object)
        .ok_or_else(|| ApiError::bad_request("connection is required"))?;
    let command = required_text(req.command, "command")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let timeout_ms = req.timeout_ms.unwrap_or(30_000).clamp(1_000, 600_000);
    let relay_timeout = state
        .config
        .relay_request_timeout
        .max(Duration::from_millis(timeout_ms.saturating_add(5_000)));
    let request = RelayRequest {
        message_type: "remote_connection_command_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/remote-connections/command".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "connection": connection,
            "command": command,
            "timeout_ms": timeout_ms,
            "verification_code": req.verification_code,
        }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_object_connection_payloads() {
        let request = RemoteConnectionTestRelayRequest {
            workspace_id: Some("workspace-1".to_string()),
            connection: Some(json!("not-an-object")),
            verification_code: None,
        };

        assert!(!request.connection.is_some_and(|value| value.is_object()));
    }
}
