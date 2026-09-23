// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use axum::extract::{Path, Query, State};
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

#[derive(Debug, Deserialize)]
pub(super) struct CompanionAgentMessagesQuery {
    before_message_id: Option<String>,
    after_message_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SendCompanionAgentMessageRequest {
    content: Option<String>,
    #[serde(default)]
    mentioned_agent_ids: Vec<String>,
    client_message_id: Option<String>,
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

pub(super) async fn get_companion_agent_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
) -> Result<Response, ApiError> {
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_agent_workspace_request",
        "/companion/agent-workspace",
        "GET",
        Value::Null,
    )
    .await
}

pub(super) async fn get_companion_agent_conversation(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, conversation_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let room_id = required_path_value(conversation_id, "conversation_id")?;
    let relay_path = format!("/companion/agent-conversations/{room_id}");
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_agent_conversation_request",
        relay_path.as_str(),
        "GET",
        json!({ "room_id": room_id }),
    )
    .await
}

pub(super) async fn list_companion_agent_messages(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, conversation_id)): Path<(String, String)>,
    Query(query): Query<CompanionAgentMessagesQuery>,
) -> Result<Response, ApiError> {
    let room_id = required_path_value(conversation_id, "conversation_id")?;
    if query.before_message_id.is_some() && query.after_message_id.is_some() {
        return Err(ApiError::bad_request(
            "before_message_id and after_message_id are mutually exclusive",
        ));
    }
    let relay_path = format!("/companion/agent-conversations/{room_id}/messages");
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_agent_messages_request",
        relay_path.as_str(),
        "GET",
        json!({
            "room_id": room_id,
            "before_message_id": query.before_message_id,
            "after_message_id": query.after_message_id,
            "limit": query.limit.unwrap_or(40).clamp(1, 100),
        }),
    )
    .await
}

pub(super) async fn send_companion_agent_message(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, conversation_id)): Path<(String, String)>,
    Json(body): Json<SendCompanionAgentMessageRequest>,
) -> Result<Response, ApiError> {
    let room_id = required_path_value(conversation_id, "conversation_id")?;
    let content = body
        .content
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("content is required"))?;
    let client_message_id = body
        .client_message_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| ApiError::bad_request("client_message_id is required"))?;
    if body.mentioned_agent_ids.len() > 32
        || body
            .mentioned_agent_ids
            .iter()
            .any(|value| value.trim().is_empty() || value.trim() != value)
    {
        return Err(ApiError::bad_request("mentioned_agent_ids is invalid"));
    }
    let relay_path = format!("/companion/agent-conversations/{room_id}/messages");
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_agent_send_message_request",
        relay_path.as_str(),
        "POST",
        json!({
            "room_id": room_id,
            "content": content,
            "mentioned_agent_ids": body.mentioned_agent_ids,
            "client_message_id": client_message_id,
        }),
    )
    .await
}

pub(super) async fn open_companion_agent_direct_conversation(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((device_id, agent_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let agent_id = required_path_value(agent_id, "agent_id")?;
    let relay_path = format!("/companion/agents/{agent_id}/direct-conversation");
    companion_relay(
        &state,
        &user,
        device_id,
        "companion_agent_open_direct_request",
        relay_path.as_str(),
        "POST",
        json!({ "agent_id": agent_id }),
    )
    .await
}

fn required_path_value(value: String, field: &str) -> Result<String, ApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field} is required")))
    } else {
        Ok(value)
    }
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
