// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{RuntimeInvocationResponse, RuntimeInvocationStatus};
use serde_json::{json, Value};

use crate::auth::require_internal_request;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) async fn cancel_runtime_invocation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invocation_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let caller_service =
        require_internal_request(&state.config, &headers, "runtime.invocations.cancel")?;
    let invocation_id = invocation_id.trim();
    if invocation_id.is_empty() || invocation_id.len() > 160 {
        return Err(ApiError::bad_request("invocation id is invalid"));
    }
    let record = state
        .runtime_invocations
        .request_cancel_by_invocation(invocation_id, caller_service.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime invocation was not found or has expired"))?;
    Ok(Json(json!({
        "invocation_id": record.invocation_id,
        "status": super::mcp::cancel_response_status(&record),
    })))
}

pub(super) async fn get_runtime_invocation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invocation_id): Path<String>,
) -> Result<Json<RuntimeInvocationResponse>, ApiError> {
    let caller_service =
        require_internal_request(&state.config, &headers, "runtime.invocations.read")?;
    let invocation_id = invocation_id.trim();
    if invocation_id.is_empty() || invocation_id.len() > 160 {
        return Err(ApiError::bad_request("invocation id is invalid"));
    }
    let record = state
        .runtime_invocations
        .get_for_caller(invocation_id, caller_service.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime invocation was not found or has expired"))?;
    Ok(Json(RuntimeInvocationResponse {
        invocation_id: record.invocation_id,
        session_id: record.session_id,
        caller_service: record.caller_service,
        resource_id: record.resource_id,
        exposed_tool_name: record.exposed_tool_name,
        status: match record.status {
            crate::runtime::RuntimeInvocationStatus::Queued => RuntimeInvocationStatus::Queued,
            crate::runtime::RuntimeInvocationStatus::Running => RuntimeInvocationStatus::Running,
            crate::runtime::RuntimeInvocationStatus::CancelRequested => {
                RuntimeInvocationStatus::CancelRequested
            }
            crate::runtime::RuntimeInvocationStatus::Completed => {
                RuntimeInvocationStatus::Completed
            }
            crate::runtime::RuntimeInvocationStatus::Failed => RuntimeInvocationStatus::Failed,
            crate::runtime::RuntimeInvocationStatus::Cancelled => {
                RuntimeInvocationStatus::Cancelled
            }
            crate::runtime::RuntimeInvocationStatus::UnknownExecutionState => {
                RuntimeInvocationStatus::UnknownExecutionState
            }
        },
        async_execution: record.async_execution,
        created_at_unix_ms: record.created_at_unix_ms,
        started_at_unix_ms: record.started_at_unix_ms,
        completed_at_unix_ms: record.completed_at_unix_ms,
        terminal_result: record.terminal_result,
        terminal_error_code: record.terminal_error_code,
        terminal_error_message: record.terminal_error_message,
    }))
}
