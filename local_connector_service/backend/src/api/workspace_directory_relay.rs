// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::response::Response;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
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
