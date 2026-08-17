// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
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
pub(super) struct WorkspaceDirectoryCreateRelayRequest {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WorkspaceDirectoryListRelayQuery {
    path: Option<String>,
}

pub(super) async fn workspace_directory_list_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, workspace_id)): Path<(String, String)>,
    Query(query): Query<WorkspaceDirectoryListRelayQuery>,
) -> Result<Response, ApiError> {
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(".");

    let request = RelayRequest {
        message_type: "workspace_directory_list_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id: workspace_id.clone(),
        method: "GET".to_string(),
        path: format!("/api/local/runtime/workspaces/{workspace_id}/directories"),
        headers: BTreeMap::new(),
        body: json!({ "path": path }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn workspace_directory_create_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, workspace_id)): Path<(String, String)>,
    Json(req): Json<WorkspaceDirectoryCreateRelayRequest>,
) -> Result<Response, ApiError> {
    let path = required_text(req.path, "path")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "workspace_directory_create_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id: workspace_id.clone(),
        method: "POST".to_string(),
        path: format!("/api/local/runtime/workspaces/{workspace_id}/directories"),
        headers: BTreeMap::new(),
        body: json!({ "path": path }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn workspace_filesystem_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, workspace_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let operation = body
        .get("operation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("workspace filesystem operation is required"))?;
    if !matches!(
        operation,
        "list"
            | "read"
            | "search_entries"
            | "search_content"
            | "create_directory"
            | "create_file"
            | "write_file"
            | "delete"
            | "move"
    ) {
        return Err(ApiError::bad_request(
            "unsupported workspace filesystem operation",
        ));
    }

    let request = RelayRequest {
        message_type: "workspace_filesystem_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id: workspace_id.clone(),
        method: "POST".to_string(),
        path: format!("/api/local/runtime/workspaces/{workspace_id}/filesystem"),
        headers: BTreeMap::new(),
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}
