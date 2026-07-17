// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Deserialize)]
pub(super) struct StopLocalChatRequest {
    conversation_id: String,
    turn_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct StopLocalChatResponse {
    success: bool,
    message: &'static str,
    conversation_id: String,
    turn_id: Option<String>,
}

pub(super) async fn stop_chat(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<StopLocalChatRequest>,
) -> Result<Json<StopLocalChatResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let conversation_id = required(request.conversation_id, "conversation_id")?;
    let requested_turn_id = normalize_optional(request.turn_id);
    let database = runtime.local_database()?;
    if database
        .get_session(conversation_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .is_none()
    {
        return Err(LocalRuntimeApiError::not_found(
            "local_runtime_session_not_found",
            "Local runtime session was not found",
        ));
    }

    let registry_cancelled = runtime
        .turn_control
        .cancel(conversation_id.as_str(), requested_turn_id.as_deref());
    let persisted_turn_id = database
        .request_turn_cancel(
            owner.owner_user_id.as_str(),
            conversation_id.as_str(),
            requested_turn_id.as_deref(),
        )
        .await?;
    let success = registry_cancelled || persisted_turn_id.is_some();
    Ok(Json(StopLocalChatResponse {
        success,
        message: if success {
            "Local chat cancellation requested"
        } else {
            "No matching local chat turn is running"
        },
        conversation_id,
        turn_id: persisted_turn_id.or(requested_turn_id),
    }))
}

fn required(value: String, field: &'static str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{field} is required"),
        ));
    }
    Ok(value)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
