// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;

use crate::local_runtime::memory::{local_memory_review_status, run_local_memory_review};
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

pub(super) async fn run_review(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required_session_id(session_id)?;
    let result =
        run_local_memory_review(&runtime, owner.owner_user_id.as_str(), session_id.as_str())
            .await
            .map_err(review_error)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "conversation_id": session_id,
        "conversationId": session_id,
        "project_id": result.project_id,
        "result": result,
    })))
}

pub(super) async fn review_status(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required_session_id(session_id)?;
    let result =
        local_memory_review_status(&runtime, owner.owner_user_id.as_str(), session_id.as_str())
            .await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "conversation_id": session_id,
        "conversationId": session_id,
        "project_id": result.project_id,
        "result": result,
    })))
}

fn review_error(error: anyhow::Error) -> LocalRuntimeApiError {
    let message = error.to_string();
    if message.contains("already running") || message.contains("chat is active") {
        LocalRuntimeApiError::conflict("local_memory_review_running", message)
    } else {
        LocalRuntimeApiError::bad_gateway("local_memory_review_failed", message)
    }
}

fn required_session_id(value: String) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            "session_id is required",
        ));
    }
    Ok(value)
}
