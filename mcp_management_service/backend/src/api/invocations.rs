// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{RuntimeInvocationResponse, RuntimeInvocationStatus};
use serde_json::{json, Value};

use crate::auth::{require_internal_request_identity, InternalRequestIdentity};
use crate::error::ApiError;
use crate::state::AppState;

pub(super) async fn cancel_runtime_invocation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invocation_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let identity =
        require_internal_request_identity(&state.config, &headers, "runtime.invocations.cancel")?;
    let trace_id = identity.require_signed_trace_id()?.to_string();
    let invocation_id = validated_invocation_id(invocation_id.as_str())?;
    let record = state
        .runtime_invocations
        .request_cancel_by_invocation(invocation_id, identity.caller.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime invocation was not found or has expired"))?;
    if record.status == crate::runtime::RuntimeInvocationStatus::CancelRequested {
        if let Err(error) = state
            .async_tool_dispatch
            .publish_cancellation(record.invocation_id.as_str())
            .await
        {
            tracing::error!(
                invocation_id = record.invocation_id.as_str(),
                error = %error,
                "publish Runtime Invocation cancellation event failed"
            );
            return Err(ApiError::internal(
                "runtime invocation cancellation event is unavailable",
            ));
        }
    }
    let status = super::mcp::cancel_response_status(&record);
    record_invocation_audit(&identity, trace_id, &record, "cancel", status);
    Ok(Json(json!({
        "invocation_id": record.invocation_id,
        "status": status,
    })))
}

pub(super) async fn get_runtime_invocation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invocation_id): Path<String>,
) -> Result<Json<RuntimeInvocationResponse>, ApiError> {
    let identity =
        require_internal_request_identity(&state.config, &headers, "runtime.invocations.read")?;
    let trace_id = identity.require_signed_trace_id()?.to_string();
    let invocation_id = validated_invocation_id(invocation_id.as_str())?;
    let record = state
        .runtime_invocations
        .get_for_caller(invocation_id, identity.caller.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime invocation was not found or has expired"))?;
    record_invocation_audit(&identity, trace_id, &record, "read", "succeeded");
    Ok(Json(runtime_invocation_response(record)))
}

pub(super) async fn notify_waiting_user_resolved(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(prompt_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let identity = require_internal_request_identity(
        &state.config,
        &headers,
        "runtime.invocations.resolve_user",
    )?;
    identity.require_signed_trace_id()?;
    state
        .async_tool_dispatch
        .publish_invocation_terminal(prompt_id.as_str(), Some(prompt_id.as_str()))
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(json!({"prompt_id": prompt_id, "accepted": true})))
}

fn record_invocation_audit(
    identity: &InternalRequestIdentity,
    trace_id: String,
    record: &crate::runtime::RuntimeInvocationRecord,
    action: &str,
    outcome: &str,
) {
    let event = chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: identity.caller.clone(),
        audience_service: "mcp-management-service".to_string(),
        scope: format!("runtime.invocations.{action}"),
        trace_id,
        represented_user_id: Some(record.owner_user_id.clone()),
        tenant_id: Some(record.tenant_id.clone()),
        project_id: Some(record.project_id.clone()),
        resource_type: "mcp_runtime_invocation".to_string(),
        resource_id: record.invocation_id.clone(),
        resource_name: Some(record.exposed_tool_name.clone()),
        action: action.to_string(),
        outcome: outcome.to_string(),
    };
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            invocation_id = record.invocation_id.as_str(),
            error = error.as_str(),
            "record MCP Runtime Invocation audit failed"
        );
    }
}

fn validated_invocation_id(invocation_id: &str) -> Result<&str, ApiError> {
    let invocation_id = invocation_id.trim();
    if invocation_id.is_empty() || invocation_id.len() > 160 {
        return Err(ApiError::bad_request("invocation id is invalid"));
    }
    Ok(invocation_id)
}

fn runtime_invocation_response(
    record: crate::runtime::RuntimeInvocationRecord,
) -> RuntimeInvocationResponse {
    RuntimeInvocationResponse {
        invocation_id: record.invocation_id,
        session_id: record.session_id,
        caller_service: record.caller_service,
        resource_id: record.resource_id,
        exposed_tool_name: record.exposed_tool_name,
        original_tool_name: (!record.original_tool_name.trim().is_empty())
            .then_some(record.original_tool_name),
        status: match record.status {
            crate::runtime::RuntimeInvocationStatus::Queued => RuntimeInvocationStatus::Queued,
            crate::runtime::RuntimeInvocationStatus::Running => RuntimeInvocationStatus::Running,
            crate::runtime::RuntimeInvocationStatus::WaitingForUser => {
                RuntimeInvocationStatus::WaitingForUser
            }
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
        created_at_unix_ms: record.created_at_unix_ms,
        started_at_unix_ms: record.started_at_unix_ms,
        completed_at_unix_ms: record.completed_at_unix_ms,
        terminal_result: record.terminal_result,
        terminal_error_code: record.terminal_error_code,
        terminal_error_message: record.terminal_error_message,
        file_modification_outcome: record
            .file_modification_outcome
            .map(|outcome| outcome.as_str().to_string()),
    }
}
