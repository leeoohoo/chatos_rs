// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
#[cfg(test)]
use chatos_mcp::{system_mcp_descriptor, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, RuntimeToolDescriptor};
#[cfg(test)]
use chatos_mcp_service::MCP_ERROR_UNKNOWN_EXECUTION_STATE;
use chatos_mcp_service::{
    jsonrpc_error, jsonrpc_ok, JsonRpcRequest, JsonRpcResponse, McpToolCallCommand,
    McpToolCallResultItem, McpToolCallResultStatus, MCP_ERROR_AUTH_REQUIRED, MCP_ERROR_INTERNAL,
    MCP_ERROR_INVALID_PARAMS, MCP_ERROR_METHOD_NOT_FOUND, METHOD_INITIALIZE,
    METHOD_NOTIFICATIONS_CANCELLED, METHOD_NOTIFICATIONS_INITIALIZED, METHOD_PING,
    METHOD_TOOLS_LIST,
};
use serde_json::{json, Value};

use crate::capabilities::{route_allows_system_tool, validate_product_skill_binding};
use crate::postgres::required_timestamp;
use crate::runtime::{
    RuntimeExecutionTurnState, RuntimeInvocationRecord, RuntimeInvocationRegisterError,
    RuntimeInvocationStatus, RuntimeSessionSnapshot, RuntimeToolBatchRecord,
    RuntimeToolBatchStatus,
};
use crate::state::AppState;

#[path = "mcp/batch_recovery.rs"]
mod batch_recovery;
#[path = "mcp/cancellation.rs"]
mod cancellation;
#[path = "mcp/provider_dispatch.rs"]
mod provider_dispatch;
#[path = "mcp/registration.rs"]
mod registration;

use self::batch_recovery::is_terminal_invocation_status;
pub(crate) use self::batch_recovery::{
    persist_terminal_tool_batch_invocation_without_session, resolve_waiting_user_tool_invocation,
    resume_terminal_tool_batch_invocation,
};
use self::cancellation::{
    handle_cancel_notification, handle_cancelled_tool_call, wait_for_cancellation, DispatchResult,
};
use self::provider_dispatch::{dispatch_provider_call, route_waits_for_user};
pub(crate) use self::registration::register_tool_call_command;

pub(super) async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let token = match chatos_service_runtime::bearer_token_from_headers(&headers) {
        Ok(token) => token,
        Err(_) => {
            return Json(jsonrpc_error(
                id,
                MCP_ERROR_AUTH_REQUIRED,
                "runtime session bearer token is required",
            ))
        }
    };
    let claims = match state.runtime_grants.verify(token) {
        Ok(claims) => claims,
        Err(_) => {
            return Json(jsonrpc_error(
                id,
                MCP_ERROR_AUTH_REQUIRED,
                "runtime session bearer token is invalid or expired",
            ))
        }
    };
    let snapshot = match state.runtime_sessions.get(claims.session_id.as_str()).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return Json(jsonrpc_error(
                id,
                MCP_ERROR_AUTH_REQUIRED,
                "runtime session was not found or has expired",
            ))
        }
        Err(error) => {
            tracing::error!(
                session_id = claims.session_id.as_str(),
                error = error.as_str(),
                "load Runtime Session Snapshot failed"
            );
            return Json(jsonrpc_error(
                id,
                MCP_ERROR_INTERNAL,
                "runtime session snapshot store is unavailable",
            ));
        }
    };
    if !grant_matches_snapshot(&claims, &snapshot) {
        return Json(jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "runtime session grant does not match its route snapshot",
        ));
    }
    Json(handle_session_request(request, &snapshot, &state).await)
}

async fn handle_session_request(
    request: JsonRpcRequest,
    snapshot: &RuntimeSessionSnapshot,
    state: &AppState,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    match request.method.as_str() {
        METHOD_INITIALIZE => jsonrpc_ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "chatos-mcp-management", "version": "0.1.0"}
            }),
        ),
        METHOD_NOTIFICATIONS_INITIALIZED | METHOD_PING => jsonrpc_ok(id, json!({})),
        METHOD_NOTIFICATIONS_CANCELLED => {
            handle_cancel_notification(id, request.params, snapshot, state).await
        }
        METHOD_TOOLS_LIST => jsonrpc_ok(
            id,
            json!({
                "tools": snapshot
                    .tools
                    .iter()
                    .map(|tool| tool.definition.clone())
                    .collect::<Vec<_>>()
            }),
        ),
        other => jsonrpc_error(
            id,
            MCP_ERROR_METHOD_NOT_FOUND,
            format!("method not found: {other}"),
        ),
    }
}

#[cfg(test)]
pub(crate) async fn execute_tool_call_command(
    state: &AppState,
    command: &McpToolCallCommand,
) -> Result<chatos_mcp_service::McpToolCallResult, String> {
    let mut batch = register_tool_call_command(state, command).await?.record;
    while batch.status != RuntimeToolBatchStatus::Completed {
        let call_index = batch.next_call_index;
        let call = &batch.command.calls[call_index];
        if batch.items[call_index].is_some() {
            batch = state
                .runtime_tool_batches
                .record_terminal_item(
                    batch.batch_id.as_str(),
                    call_index,
                    batch.items[call_index]
                        .clone()
                        .expect("checked persisted command result"),
                )
                .await?;
            continue;
        }
        let snapshot = state
            .runtime_sessions
            .get(batch.session_id.as_str())
            .await?
            .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
        let tool = snapshot
            .tools
            .iter()
            .find(|tool| tool.exposed_name == call.name)
            .cloned()
            .ok_or_else(|| format!("tool not found: {}", call.name))?;
        let route = snapshot
            .routes
            .iter()
            .find(|route| route.resource_id == tool.resource_id)
            .cloned()
            .ok_or_else(|| "tool route snapshot is missing".to_string())?;
        let record = state
            .runtime_invocations
            .get_for_caller(
                call.invocation_id.as_str(),
                snapshot.caller_service.as_str(),
            )
            .await?
            .ok_or_else(|| "Runtime Invocation record is missing".to_string())?;
        execute_async_tool_call(
            state.clone(),
            snapshot.clone(),
            route,
            tool,
            call.arguments.clone(),
            call.invocation_id.clone(),
            record.mutation_may_have_started,
        )
        .await?;
        let record = state
            .runtime_invocations
            .get_for_caller(
                call.invocation_id.as_str(),
                snapshot.caller_service.as_str(),
            )
            .await?
            .ok_or_else(|| "completed Runtime Invocation record is missing".to_string())?;
        batch = state
            .runtime_tool_batches
            .record_terminal_item(
                batch.batch_id.as_str(),
                call_index,
                result_item_from_record(call, record),
            )
            .await?;
    }
    batch
        .aggregate_result()
        .ok_or_else(|| "Runtime Tool Batch aggregate result is missing".to_string())
}

pub(crate) async fn execute_tool_batch_invocation(
    state: &AppState,
    batch_id: &str,
    call_index: usize,
) -> Result<RuntimeToolBatchRecord, String> {
    let batch = state
        .runtime_tool_batches
        .get(batch_id)
        .await?
        .ok_or_else(|| "Runtime Tool Batch was not found".to_string())?;
    if batch.status == RuntimeToolBatchStatus::Completed {
        return Ok(batch);
    }
    if call_index < batch.next_call_index {
        return Ok(batch);
    }
    if batch.next_call_index != call_index {
        return Err(format!(
            "Runtime Tool Batch expected call {} but received ready call {call_index}",
            batch.next_call_index
        ));
    }
    let call = batch
        .command
        .calls
        .get(call_index)
        .ok_or_else(|| "Runtime Tool Batch call_index is out of range".to_string())?;
    if let Some(item) = batch.items.get(call_index).cloned().flatten() {
        return state
            .runtime_tool_batches
            .record_terminal_item(batch_id, call_index, item)
            .await;
    }
    let record = state
        .runtime_invocations
        .get_for_caller(
            call.invocation_id.as_str(),
            batch.command.owner_service.as_str(),
        )
        .await?;
    let Some(record) = record else {
        // A ready event can outlive its invocation because of session cleanup,
        // TTL expiry, or replayed request ids. This is terminal data
        // inconsistency, not a transient delivery failure: return a structured
        // tool error to the model and advance the FIFO instead of hot-looping
        // the RabbitMQ delivery forever.
        if let Some(snapshot) = state
            .runtime_sessions
            .get(batch.session_id.as_str())
            .await?
        {
            if let Some(run_id) = snapshot.run_id.as_deref() {
                state
                    .runtime_execution_scopes
                    .release_invocation_turn(
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_deref(),
                        run_id,
                        snapshot.execution_scope_provider(),
                        call.invocation_id.as_str(),
                    )
                    .await?;
            }
        }
        return state
            .runtime_tool_batches
            .record_terminal_item(
                batch_id,
                call_index,
                failed_command_item(
                    call,
                    MCP_ERROR_INTERNAL,
                    "MCP invocation record is unavailable; the tool call was not executed"
                        .to_string(),
                ),
            )
            .await;
    };
    if is_terminal_invocation_status(record.status) {
        return state
            .runtime_tool_batches
            .record_terminal_item(batch_id, call_index, result_item_from_record(call, record))
            .await;
    }
    if record.status == RuntimeInvocationStatus::WaitingForUser {
        return Ok(batch);
    }
    let snapshot = match state
        .runtime_sessions
        .get(batch.session_id.as_str())
        .await?
    {
        Some(snapshot) => snapshot,
        None => {
            state
                .runtime_invocations
                .close_registered_invocation(
                    call.invocation_id.as_str(),
                    record.session_id.as_str(),
                )
                .await?;
            let recovered = state
                .runtime_invocations
                .get_for_caller(
                    call.invocation_id.as_str(),
                    batch.command.owner_service.as_str(),
                )
                .await?;
            if let Some(recovered) = recovered {
                return state
                    .runtime_tool_batches
                    .record_terminal_item(
                        batch_id,
                        call_index,
                        result_item_from_record(call, recovered),
                    )
                    .await;
            }
            return state
                .runtime_tool_batches
                .record_terminal_item(
                    batch_id,
                    call_index,
                    failed_command_item(
                        call,
                        MCP_ERROR_INTERNAL,
                        "MCP runtime session expired before the tool call started".to_string(),
                    ),
                )
                .await;
        }
    };
    let tool = snapshot
        .tools
        .iter()
        .find(|tool| tool.exposed_name == call.name)
        .cloned()
        .ok_or_else(|| format!("tool not found in Runtime Session Snapshot: {}", call.name))?;
    let route = snapshot
        .routes
        .iter()
        .find(|route| route.resource_id == tool.resource_id)
        .cloned()
        .ok_or_else(|| "tool route snapshot is missing".to_string())?;
    let mutation_may_have_started = record.mutation_may_have_started;
    if route_waits_for_user(&route) {
        match state
            .runtime_execution_scopes
            .try_acquire_invocation_turn(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                snapshot
                    .run_id
                    .as_deref()
                    .ok_or_else(|| "Ask User invocation requires run_id".to_string())?,
                snapshot.execution_scope_provider(),
                call.invocation_id.as_str(),
            )
            .await?
        {
            RuntimeExecutionTurnState::Waiting => return Ok(batch),
            RuntimeExecutionTurnState::Terminal => {
                state
                    .runtime_invocations
                    .cancel_without_start(call.invocation_id.as_str())
                    .await?;
            }
            RuntimeExecutionTurnState::Acquired => {
                if record.status == RuntimeInvocationStatus::Queued
                    && !state
                        .runtime_invocations
                        .mark_running(call.invocation_id.as_str())
                        .await?
                {
                    return Ok(batch);
                }
                let waiting = match state
                    .providers
                    .start_waiting_user_call(
                        &snapshot,
                        &route,
                        tool.original_name.as_str(),
                        call.arguments.clone(),
                        call.invocation_id.as_str(),
                    )
                    .await
                {
                    Ok(waiting) => waiting,
                    Err(error) => {
                        state
                            .runtime_invocations
                            .fail(call.invocation_id.as_str(), error.code, error.message)
                            .await?;
                        if let Some(run_id) = snapshot.run_id.as_deref() {
                            state
                                .runtime_execution_scopes
                                .release_invocation_turn(
                                    snapshot.owner_user_id.as_str(),
                                    snapshot.project_id.as_deref(),
                                    run_id,
                                    snapshot.execution_scope_provider(),
                                    call.invocation_id.as_str(),
                                )
                                .await?;
                        }
                        let record = state
                            .runtime_invocations
                            .get_for_caller(
                                call.invocation_id.as_str(),
                                snapshot.caller_service.as_str(),
                            )
                            .await?
                            .ok_or_else(|| {
                                "failed Ask User Runtime Invocation record is missing".to_string()
                            })?;
                        return state
                            .runtime_tool_batches
                            .record_terminal_item(
                                batch_id,
                                call_index,
                                result_item_from_record(call, record),
                            )
                            .await;
                    }
                };
                if !state
                    .runtime_invocations
                    .mark_waiting_for_user(call.invocation_id.as_str())
                    .await?
                {
                    return Ok(batch);
                }
                return state
                    .runtime_tool_batches
                    .mark_waiting_for_user(batch_id, call_index, waiting.prompt_id)
                    .await;
            }
        }
    }
    if record.status == RuntimeInvocationStatus::Running {
        return Ok(batch);
    }
    execute_async_tool_call(
        state.clone(),
        snapshot.clone(),
        route,
        tool,
        call.arguments.clone(),
        call.invocation_id.clone(),
        mutation_may_have_started,
    )
    .await?;
    let record = state
        .runtime_invocations
        .get_for_caller(
            call.invocation_id.as_str(),
            snapshot.caller_service.as_str(),
        )
        .await?
        .ok_or_else(|| "completed MCP invocation record is missing".to_string())?;
    if !is_terminal_invocation_status(record.status) {
        return Ok(batch);
    }
    state
        .runtime_tool_batches
        .record_terminal_item(batch_id, call_index, result_item_from_record(call, record))
        .await
}

pub(crate) fn failed_command_item(
    call: &chatos_mcp_service::McpToolCallCommandItem,
    error_code: i32,
    error: String,
) -> McpToolCallResultItem {
    McpToolCallResultItem {
        invocation_id: call.invocation_id.clone(),
        tool_call_id: call.tool_call_id.clone(),
        call_index: call.call_index,
        name: call.name.clone(),
        status: McpToolCallResultStatus::Failed,
        result: None,
        error_code: Some(error_code),
        error: Some(error),
    }
}

pub(crate) fn result_item_from_record(
    call: &chatos_mcp_service::McpToolCallCommandItem,
    record: RuntimeInvocationRecord,
) -> McpToolCallResultItem {
    let status = match record.status {
        RuntimeInvocationStatus::Completed => McpToolCallResultStatus::Completed,
        RuntimeInvocationStatus::Cancelled => McpToolCallResultStatus::Cancelled,
        RuntimeInvocationStatus::UnknownExecutionState => {
            McpToolCallResultStatus::UnknownExecutionState
        }
        _ => McpToolCallResultStatus::Failed,
    };
    McpToolCallResultItem {
        invocation_id: call.invocation_id.clone(),
        tool_call_id: call.tool_call_id.clone(),
        call_index: call.call_index,
        name: call.name.clone(),
        status,
        result: record.terminal_result,
        error_code: record.terminal_error_code,
        error: record.terminal_error_message,
    }
}

fn request_id_key(id: &Value) -> Result<String, &'static str> {
    if !matches!(id, Value::String(_) | Value::Number(_)) {
        return Err("JSON-RPC request id must be a string or number");
    }
    serde_json::to_string(id).map_err(|_| "JSON-RPC request id is invalid")
}

async fn register_runtime_invocation(
    state: &AppState,
    snapshot: &RuntimeSessionSnapshot,
    invocation: RuntimeInvocationRecord,
    enqueue_scope: bool,
) -> Result<(), RuntimeInvocationRegisterError> {
    if let Some(run_id) = snapshot.run_id.as_deref() {
        state
            .runtime_execution_scopes
            .ensure_accepting_invocations(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
            )
            .await
            .map_err(|error| match error {
                crate::runtime::RuntimeExecutionScopeStoreError::Terminal => {
                    RuntimeInvocationRegisterError::SessionClosed
                }
                crate::runtime::RuntimeExecutionScopeStoreError::Unavailable(error) => {
                    RuntimeInvocationRegisterError::StoreUnavailable(error)
                }
            })?;
    }
    if let Err(error) = ensure_runtime_session_is_active(state, snapshot.session_id.as_str()).await
    {
        state.runtime_invocations.observe_register_error(&error);
        return Err(error);
    }
    let invocation_id = invocation.invocation_id.clone();
    state.runtime_invocations.register(invocation).await?;
    if enqueue_scope {
        let Some(run_id) = snapshot.run_id.as_deref() else {
            return Ok(());
        };
        if let Err(error) = state
            .runtime_execution_scopes
            .enqueue_invocation(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id.as_str(),
            )
            .await
        {
            let _ = state
                .runtime_invocations
                .close_registered_invocation(invocation_id.as_str(), snapshot.session_id.as_str())
                .await;
            return Err(match error {
                crate::runtime::RuntimeExecutionScopeStoreError::Terminal => {
                    RuntimeInvocationRegisterError::SessionClosed
                }
                crate::runtime::RuntimeExecutionScopeStoreError::Unavailable(error) => {
                    RuntimeInvocationRegisterError::StoreUnavailable(error)
                }
            });
        }
    }
    if let Err(error) = ensure_runtime_session_is_active(state, snapshot.session_id.as_str()).await
    {
        if let Err(close_error) = state
            .runtime_invocations
            .close_registered_invocation(invocation_id.as_str(), snapshot.session_id.as_str())
            .await
        {
            let error = RuntimeInvocationRegisterError::StoreUnavailable(format!(
                "close Runtime Invocation after session validation failed: {close_error}"
            ));
            state.runtime_invocations.observe_register_error(&error);
            return Err(error);
        }
        state.runtime_invocations.observe_register_error(&error);
        return Err(error);
    }
    Ok(())
}

async fn ensure_runtime_session_is_active(
    state: &AppState,
    session_id: &str,
) -> Result<(), RuntimeInvocationRegisterError> {
    match state.runtime_sessions.get(session_id).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(RuntimeInvocationRegisterError::SessionClosed),
        Err(error) => Err(RuntimeInvocationRegisterError::StoreUnavailable(format!(
            "verify Runtime Session before invocation registration failed: {error}"
        ))),
    }
}

fn record_tool_access_audit(
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    tool_name: &str,
    outcome: &str,
) {
    let event = chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: snapshot.caller_service.clone(),
        audience_service: "mcp-management-service".to_string(),
        scope: "runtime.tools.call".to_string(),
        trace_id: snapshot.trace_id.clone(),
        represented_user_id: Some(snapshot.owner_user_id.clone()),
        tenant_id: Some(snapshot.tenant_id.clone()),
        project_id: snapshot.project_id.clone(),
        resource_type: "mcp_tool".to_string(),
        resource_id: route.resource_id.clone(),
        resource_name: Some(tool_name.to_string()),
        action: "call".to_string(),
        outcome: outcome.to_string(),
    };
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            session_id = snapshot.session_id.as_str(),
            resource_id = route.resource_id.as_str(),
            tool_name,
            error = error.as_str(),
            "record MCP tool access audit failed"
        );
    }
}

include!("mcp_part01.rs");
