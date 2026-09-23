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

use super::{dispatch_companion_relay, load_owned_device, relay_response_to_http, ApiError};

const COMPANION_RELAY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
pub(super) struct ResolveCompanionResourceRequest {
    resource_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ResolveCompanionApprovalRequest {
    decision: Option<String>,
}

pub(super) async fn list_companion_resources(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
) -> Result<Response, ApiError> {
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_resources_request",
        "/companion/resources",
        "GET",
        Value::Null,
    )
    .await
}

pub(super) async fn resolve_companion_resource(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(body): Json<ResolveCompanionResourceRequest>,
) -> Result<Response, ApiError> {
    let resource_id = body
        .resource_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("resource_id is required"))?;
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_resolve_resource_request",
        "/companion/resources/resolve",
        "POST",
        json!({ "resource_id": resource_id }),
    )
    .await
}

pub(super) async fn list_companion_approvals(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
) -> Result<Response, ApiError> {
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_approvals_request",
        "/companion/approvals",
        "GET",
        Value::Null,
    )
    .await
}

pub(super) async fn resolve_companion_approval(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, approval_id)): Path<(String, String)>,
    Json(body): Json<ResolveCompanionApprovalRequest>,
) -> Result<Response, ApiError> {
    let approval_id = approval_id.trim().to_string();
    if approval_id.is_empty() {
        return Err(ApiError::bad_request("approval_id is required"));
    }
    let decision = body
        .decision
        .map(|value| value.trim().to_string())
        .filter(|value| matches!(value.as_str(), "accept" | "acceptForSession" | "decline"))
        .ok_or_else(|| ApiError::bad_request("decision is invalid"))?;
    let relay_path = format!("/companion/approvals/{approval_id}/resolve");
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_resolve_approval_request",
        relay_path.as_str(),
        "POST",
        json!({ "approval_id": approval_id, "decision": decision }),
    )
    .await
}

async fn companion_relay(
    state: &AppState,
    user: &CurrentUser,
    device_id: String,
    message_type: &str,
    path: &str,
    method: &str,
    body: Value,
) -> Result<Response, ApiError> {
    load_owned_device(state, user, device_id.as_str(), true).await?;
    let request = RelayRequest {
        message_type: message_type.to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id: String::new(),
        method: method.to_string(),
        path: path.to_string(),
        headers: BTreeMap::new(),
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let client_session_id = user
        .token_jti
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(user.user_id.as_str());
    let response = dispatch_companion_relay(
        state,
        request,
        COMPANION_RELAY_TIMEOUT.min(state.config.relay_request_timeout),
        client_session_id,
    )
    .await?;
    Ok(relay_response_to_http(response))
}
