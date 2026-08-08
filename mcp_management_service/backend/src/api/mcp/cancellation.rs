// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::{
    jsonrpc_error, jsonrpc_ok, CancelledNotificationParams, JsonRpcResponse, MCP_ERROR_INTERNAL,
    MCP_ERROR_INVALID_PARAMS, MCP_ERROR_INVOCATION_CANCELLED, MCP_ERROR_UNKNOWN_EXECUTION_STATE,
};

use crate::providers::ProviderCancelOutcome;
use crate::runtime::{RuntimeInvocationRecord, RuntimeInvocationStatus, RuntimeSessionSnapshot};
use crate::state::AppState;

use super::{record_tool_access_audit, request_id_key};

pub(super) enum DispatchResult {
    Completed(Result<crate::providers::ProviderCallOutcome, crate::providers::ProviderCallError>),
    CancelRequested,
    RegistryFailed(String),
}

pub(super) async fn wait_for_cancellation(
    state: &AppState,
    invocation_id: &str,
) -> Result<(), String> {
    state
        .runtime_invocations
        .wait_for_cancellation(invocation_id)
        .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_cancelled_tool_call(
    id: Value,
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    exposed_tool_name: &str,
    invocation_id: &str,
    mutation_may_have_started: bool,
    duration_ms: u64,
    state: &AppState,
) -> JsonRpcResponse {
    let provider_outcome = state
        .providers
        .cancel_invocation(snapshot, route, invocation_id)
        .await;
    let (status, terminal_status, code, message) = match provider_outcome {
        Ok(ProviderCancelOutcome::Cancelled) => (
            "cancelled",
            Some(RuntimeInvocationStatus::Cancelled),
            MCP_ERROR_INVOCATION_CANCELLED,
            "invocation_cancelled",
        ),
        Ok(ProviderCancelOutcome::CancelRequested | ProviderCancelOutcome::NotSupported)
        | Err(_)
            if mutation_may_have_started =>
        {
            (
                "unknown_execution_state",
                Some(RuntimeInvocationStatus::UnknownExecutionState),
                MCP_ERROR_UNKNOWN_EXECUTION_STATE,
                "unknown_execution_state",
            )
        }
        Ok(ProviderCancelOutcome::CancelRequested | ProviderCancelOutcome::NotSupported)
        | Err(_) => (
            "cancel_requested",
            None,
            MCP_ERROR_INVOCATION_CANCELLED,
            "cancel_requested",
        ),
    };
    record_tool_access_audit(snapshot, route, exposed_tool_name, status);
    if let Some(terminal_status) = terminal_status {
        if let Err(error) = state
            .runtime_invocations
            .finish_cancellation(invocation_id, terminal_status)
            .await
        {
            tracing::error!(
                invocation_id,
                error = error.as_str(),
                "persist Runtime Invocation cancellation outcome failed"
            );
        }
    }
    if let Err(error) = provider_outcome {
        tracing::warn!(
            invocation_id,
            error_code = error.code,
            error = error.message.as_str(),
            "Provider cancellation propagation failed"
        );
    }
    tracing::info!(
        invocation_id,
        session_id = snapshot.session_id.as_str(),
        resource_id = route.resource_id.as_str(),
        exposed_tool_name,
        provider_kind = route.provider_kind.as_str(),
        duration_ms,
        status,
        cancel_outcome = status,
        "MCP Provider invocation cancellation completed"
    );
    jsonrpc_error(id, code, message)
}

pub(super) async fn handle_cancel_notification(
    id: Value,
    params: Value,
    snapshot: &RuntimeSessionSnapshot,
    state: &AppState,
) -> JsonRpcResponse {
    let params = match CancelledNotificationParams::parse(params) {
        Ok(params) => params,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    let request_id_key = match request_id_key(&params.request_id) {
        Ok(value) => value,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    let record = match state
        .runtime_invocations
        .request_cancel_by_request(snapshot.session_id.as_str(), request_id_key.as_str())
        .await
    {
        Ok(record) => record,
        Err(error) => {
            tracing::error!(
                session_id = snapshot.session_id.as_str(),
                error = error.as_str(),
                "request Runtime Invocation cancellation failed"
            );
            return jsonrpc_error(
                id,
                MCP_ERROR_INTERNAL,
                "runtime invocation registry is unavailable",
            );
        }
    };
    let Some(record) = record else {
        return jsonrpc_ok(id, json!({"status": "invocation_not_found"}));
    };
    if record.status == RuntimeInvocationStatus::CancelRequested {
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
            return jsonrpc_error(
                id,
                MCP_ERROR_INTERNAL,
                "runtime invocation cancellation event is unavailable",
            );
        }
    }
    jsonrpc_ok(
        id,
        json!({
            "invocationId": record.invocation_id,
            "status": cancel_response_status(&record),
        }),
    )
}

pub(super) fn cancel_response_status(record: &RuntimeInvocationRecord) -> &'static str {
    match record.status {
        RuntimeInvocationStatus::Queued
        | RuntimeInvocationStatus::Running
        | RuntimeInvocationStatus::WaitingForUser
        | RuntimeInvocationStatus::CancelRequested => {
            if record.mutation_may_have_started && !record.cancel_supported {
                "unknown_execution_state"
            } else {
                "cancel_requested"
            }
        }
        RuntimeInvocationStatus::Completed => "already_completed",
        RuntimeInvocationStatus::Failed => "already_failed",
        RuntimeInvocationStatus::Cancelled => "cancelled",
        RuntimeInvocationStatus::UnknownExecutionState => "unknown_execution_state",
    }
}
