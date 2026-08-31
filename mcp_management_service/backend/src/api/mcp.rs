// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
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
use mongodb::bson::DateTime;
use serde_json::{json, Value};

use crate::capabilities::route_allows_system_tool;
use crate::runtime::{
    RuntimeExecutionTurnState, RuntimeInvocationRecord, RuntimeInvocationRegisterError,
    RuntimeInvocationStatus, RuntimeSessionSnapshot, RuntimeToolBatchRecord,
    RuntimeToolBatchStatus,
};
use crate::state::AppState;

#[path = "mcp/cancellation.rs"]
mod cancellation;

use self::cancellation::{
    handle_cancel_notification, handle_cancelled_tool_call, wait_for_cancellation, DispatchResult,
};

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

pub(crate) struct RegisteredToolBatch {
    pub record: RuntimeToolBatchRecord,
}

pub(crate) async fn register_tool_call_command(
    state: &AppState,
    command: &McpToolCallCommand,
) -> Result<RegisteredToolBatch, String> {
    let snapshot = state
        .runtime_sessions
        .get(command.mcp_runtime_session_ref.trim())
        .await?
        .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
    command.validate()?;
    if command.owner_service != snapshot.caller_service || command.agent_key != snapshot.agent_key {
        return Err(
            "MCP tool call command identity does not match its runtime session".to_string(),
        );
    }
    if command.calls.is_empty() || command.calls.len() > 128 {
        return Err("MCP tool call command must contain between 1 and 128 calls".to_string());
    }
    if command.batch_id.trim().is_empty() || command.batch_id.len() > 200 {
        return Err("MCP tool call command batch_id is invalid".to_string());
    }
    if let Some(existing) = state
        .runtime_tool_batches
        .get(command.batch_id.as_str())
        .await?
    {
        if serde_json::to_value(&existing.command).map_err(|error| error.to_string())?
            != serde_json::to_value(command).map_err(|error| error.to_string())?
        {
            return Err("Runtime Tool Batch id conflicts with a different command".to_string());
        }
        return Ok(RegisteredToolBatch { record: existing });
    }

    struct RegisteredCall {
        call_index: usize,
    }

    let mut results = vec![None; command.calls.len()];
    let mut registered = Vec::new();
    let mut seen_tool_call_ids = BTreeSet::new();
    let mut seen_invocation_ids = BTreeSet::new();
    for (call_index, call) in command.calls.iter().enumerate() {
        let item_error = if call.call_index != call_index {
            Some("call_index must match the calls array order".to_string())
        } else if call.tool_call_id.trim().is_empty()
            || !seen_tool_call_ids.insert(call.tool_call_id.clone())
        {
            Some("tool_call_id is empty or duplicated".to_string())
        } else if call.invocation_id.trim().is_empty()
            || !seen_invocation_ids.insert(call.invocation_id.clone())
        {
            Some("invocation_id is empty or duplicated".to_string())
        } else if let Some(error) = call.preflight_error.clone() {
            Some(error)
        } else if !call.arguments.is_object() {
            Some("tool arguments must be an object".to_string())
        } else {
            None
        };
        if let Some(error) = item_error {
            results[call_index] = Some(failed_command_item(call, MCP_ERROR_INVALID_PARAMS, error));
            continue;
        }
        if let Some(existing) = state
            .runtime_invocations
            .get_for_caller(
                call.invocation_id.as_str(),
                snapshot.caller_service.as_str(),
            )
            .await?
        {
            if existing.session_id != snapshot.session_id
                || existing.request_id_key
                    != serde_json::to_string(&Value::String(call.tool_call_id.clone()))
                        .map_err(|error| error.to_string())?
                || existing.exposed_tool_name != call.name
            {
                results[call_index] = Some(failed_command_item(
                    call,
                    MCP_ERROR_INVALID_PARAMS,
                    "invocation identity conflicts with an existing call".to_string(),
                ));
            } else if matches!(
                existing.status,
                RuntimeInvocationStatus::Completed
                    | RuntimeInvocationStatus::Failed
                    | RuntimeInvocationStatus::Cancelled
                    | RuntimeInvocationStatus::UnknownExecutionState
            ) {
                results[call_index] = Some(result_item_from_record(call, existing));
            } else {
                registered.push(RegisteredCall { call_index });
            }
            continue;
        }
        let Some(tool) = snapshot
            .tools
            .iter()
            .find(|tool| tool.exposed_name == call.name)
            .cloned()
        else {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_INVALID_PARAMS,
                format!("tool not found: {}", call.name),
            ));
            continue;
        };
        let Some(route) = snapshot
            .routes
            .iter()
            .find(|route| route.resource_id == tool.resource_id)
            .cloned()
        else {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_INTERNAL,
                "tool route snapshot is missing".to_string(),
            ));
            continue;
        };
        if !route_allows_system_tool(&route, tool.original_name.as_str()) {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_AUTH_REQUIRED,
                "tool is blocked by the immutable read-only route policy".to_string(),
            ));
            continue;
        }
        if route.provider_kind == McpProviderKind::Unavailable {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_INTERNAL,
                format!("provider unavailable: {}", route.reason),
            ));
            continue;
        }
        let mutation_may_have_started = route.allow_writes
            && tool
                .definition
                .pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool)
                != Some(true);
        let invocation = RuntimeInvocationRecord {
            invocation_id: call.invocation_id.clone(),
            session_id: snapshot.session_id.clone(),
            request_id_key: serde_json::to_string(&Value::String(call.tool_call_id.clone()))
                .map_err(|error| error.to_string())?,
            caller_service: snapshot.caller_service.clone(),
            tenant_id: snapshot.tenant_id.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            project_id: snapshot.project_id.clone(),
            device_id: snapshot.device_id.clone(),
            resource_id: route.resource_id.clone(),
            exposed_tool_name: tool.exposed_name.clone(),
            original_tool_name: tool.original_name.clone(),
            mutation_may_have_started,
            cancel_supported: route.cancel_supported,
            status: RuntimeInvocationStatus::Queued,
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            started_at_unix_ms: None,
            completed_at_unix_ms: None,
            terminal_result: None,
            terminal_error_code: None,
            terminal_error_message: None,
            file_modification_outcome: None,
            expires_at: DateTime::from_millis(snapshot.expires_at_unix.saturating_mul(1_000)),
            expires_at_unix: snapshot.expires_at_unix,
        };
        match register_runtime_invocation(state, &snapshot, invocation, false).await {
            Ok(()) => registered.push(RegisteredCall { call_index }),
            Err(error) => {
                results[call_index] = Some(failed_command_item(
                    call,
                    MCP_ERROR_INTERNAL,
                    format!("register MCP invocation failed: {error}"),
                ));
            }
        }
    }

    if let Some(run_id) = snapshot.run_id.as_deref() {
        let invocations = registered
            .iter()
            .map(|registered| {
                (
                    command.calls[registered.call_index].invocation_id.clone(),
                    registered.call_index,
                )
            })
            .collect::<Vec<_>>();
        if let Err(error) = state
            .runtime_execution_scopes
            .enqueue_invocation_batch(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                command.batch_id.as_str(),
                invocations.as_slice(),
            )
            .await
        {
            for registered in &registered {
                let call = &command.calls[registered.call_index];
                let _ = state
                    .runtime_invocations
                    .discard_queued_registration(
                        call.invocation_id.as_str(),
                        snapshot.session_id.as_str(),
                    )
                    .await;
                results[registered.call_index] = Some(failed_command_item(
                    call,
                    MCP_ERROR_INTERNAL,
                    format!("enqueue MCP invocation failed: {error}"),
                ));
            }
            registered.clear();
        }
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let record = RuntimeToolBatchRecord {
        batch_id: command.batch_id.clone(),
        command: command.clone(),
        session_id: snapshot.session_id.clone(),
        status: RuntimeToolBatchStatus::Active,
        next_call_index: 0,
        items: results,
        invocation_ids: command
            .calls
            .iter()
            .map(|call| call.invocation_id.clone())
            .collect(),
        waiting_user_prompt_ids: vec![None; command.calls.len()],
        pending_event: None,
        revision: 0,
        created_at_unix_ms: now_ms,
        updated_at_unix_ms: now_ms,
        expires_at: DateTime::from_millis(snapshot.expires_at_unix.saturating_mul(1_000)),
        expires_at_unix: snapshot.expires_at_unix,
    };
    let record = state.runtime_tool_batches.insert_or_get(record).await?;
    Ok(RegisteredToolBatch { record })
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

pub(crate) async fn resolve_waiting_user_tool_invocation(
    state: &AppState,
    prompt_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    let Some(batch) = state
        .runtime_tool_batches
        .find_by_waiting_user_prompt(prompt_id)
        .await?
    else {
        return Ok(None);
    };
    let Some(call_index) = batch
        .waiting_user_prompt_ids
        .iter()
        .position(|item| item.as_deref() == Some(prompt_id))
    else {
        return Ok(None);
    };
    let call = &batch.command.calls[call_index];
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
        .ok_or_else(|| format!("tool not found in Runtime Session Snapshot: {}", call.name))?;
    let route = snapshot
        .routes
        .iter()
        .find(|route| route.resource_id == tool.resource_id)
        .cloned()
        .ok_or_else(|| "tool route snapshot is missing".to_string())?;
    let Some(result) = state
        .providers
        .resolve_waiting_user_call(&snapshot, &route, prompt_id, call.invocation_id.as_str())
        .await
        .map_err(|error| error.message)?
    else {
        return Ok(Some(batch));
    };
    state
        .runtime_invocations
        .complete(call.invocation_id.as_str(), result)
        .await?;
    resume_terminal_tool_batch_invocation(state, call.invocation_id.as_str()).await
}

pub(crate) async fn resume_terminal_tool_batch_invocation(
    state: &AppState,
    invocation_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    let Some(batch) = state
        .runtime_tool_batches
        .find_by_invocation(invocation_id)
        .await?
    else {
        return Ok(None);
    };
    let Some(call_index) = batch
        .command
        .calls
        .iter()
        .position(|call| call.invocation_id == invocation_id)
    else {
        return Ok(None);
    };
    let call = &batch.command.calls[call_index];
    let record = state
        .runtime_invocations
        .get_for_caller(invocation_id, batch.command.owner_service.as_str())
        .await?
        .ok_or_else(|| "resolved Runtime Tool Batch invocation record is missing".to_string())?;
    if !is_terminal_invocation_status(record.status) {
        return Ok(Some(batch));
    }
    let snapshot = state
        .runtime_sessions
        .get(batch.session_id.as_str())
        .await?
        .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
    let next_invocation_id = if let Some(run_id) = snapshot.run_id.as_deref() {
        state
            .runtime_execution_scopes
            .release_invocation_turn_and_next(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id,
            )
            .await?
            .next_invocation_id
    } else {
        None
    };
    let batch = state
        .runtime_tool_batches
        .record_terminal_item(
            batch.batch_id.as_str(),
            call_index,
            result_item_from_record(call, record),
        )
        .await?;
    if let Some(next_invocation_id) = next_invocation_id {
        return state
            .runtime_tool_batches
            .ensure_invocation_ready_for(next_invocation_id.as_str())
            .await
            .map(Some);
    }
    Ok(Some(batch))
}

pub(crate) async fn persist_terminal_tool_batch_invocation_without_session(
    state: &AppState,
    invocation_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    let Some(batch) = state
        .runtime_tool_batches
        .find_by_invocation(invocation_id)
        .await?
    else {
        return Ok(None);
    };
    let Some(call_index) = batch
        .command
        .calls
        .iter()
        .position(|call| call.invocation_id == invocation_id)
    else {
        return Ok(None);
    };
    let call = &batch.command.calls[call_index];
    let record = state
        .runtime_invocations
        .get_for_caller(invocation_id, batch.command.owner_service.as_str())
        .await?
        .ok_or_else(|| "recovered Runtime Invocation record is missing".to_string())?;
    if !is_terminal_invocation_status(record.status) {
        return Ok(Some(batch));
    }
    state
        .runtime_tool_batches
        .record_terminal_item(
            batch.batch_id.as_str(),
            call_index,
            result_item_from_record(call, record),
        )
        .await
        .map(Some)
}

fn is_terminal_invocation_status(status: RuntimeInvocationStatus) -> bool {
    matches!(
        status,
        RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState
    )
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

async fn dispatch_provider_call(
    state: &AppState,
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    tool: &RuntimeToolDescriptor,
    arguments: Value,
    invocation_id: &str,
) -> (DispatchResult, u64) {
    let started = Instant::now();
    let mut acquired_turn = false;
    let mut cancelled_before_start = false;
    let mut deferred_turn = false;
    let mut coordination_error = None;
    if let Some(run_id) = snapshot.run_id.as_deref() {
        match state
            .runtime_execution_scopes
            .try_acquire_invocation_turn(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id,
            )
            .await
        {
            Ok(RuntimeExecutionTurnState::Acquired) => acquired_turn = true,
            Ok(RuntimeExecutionTurnState::Terminal) => cancelled_before_start = true,
            Ok(RuntimeExecutionTurnState::Waiting) => {
                match state
                    .runtime_invocations
                    .cancellation_requested(invocation_id)
                    .await
                {
                    Ok(true) => cancelled_before_start = true,
                    Ok(false) => {
                        deferred_turn = true;
                        coordination_error = Some(
                            "MCP invocation is waiting for its persisted run FIFO turn; defer the command delivery"
                                .to_string(),
                        )
                    }
                    Err(error) => coordination_error = Some(error),
                }
            }
            Err(error) => coordination_error = Some(error),
        }
    }
    if let Some(run_id) = snapshot.run_id.as_deref() {
        if (cancelled_before_start || coordination_error.is_some()) && !deferred_turn {
            if let Err(error) = state
                .runtime_execution_scopes
                .release_invocation_turn(
                    snapshot.owner_user_id.as_str(),
                    snapshot.project_id.as_deref(),
                    run_id,
                    snapshot.execution_scope_provider(),
                    invocation_id,
                )
                .await
            {
                coordination_error.get_or_insert(error);
            }
        }
    }
    if let Some(error) = coordination_error {
        return (
            DispatchResult::RegistryFailed(error),
            started.elapsed().as_millis() as u64,
        );
    }
    if cancelled_before_start {
        if let Err(error) = state
            .runtime_invocations
            .cancel_without_start(invocation_id)
            .await
        {
            return (
                DispatchResult::RegistryFailed(error),
                started.elapsed().as_millis() as u64,
            );
        }
        return (
            DispatchResult::CancelledBeforeStart,
            started.elapsed().as_millis() as u64,
        );
    }
    if snapshot.run_id.is_some() {
        match state.runtime_invocations.mark_running(invocation_id).await {
            Ok(true) => {}
            Ok(false) => {
                return (
                    DispatchResult::AlreadyRunning,
                    started.elapsed().as_millis() as u64,
                );
            }
            Err(error) => {
                if let Some(run_id) = snapshot.run_id.as_deref() {
                    let _ = state
                        .runtime_execution_scopes
                        .release_invocation_turn(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            snapshot.execution_scope_provider(),
                            invocation_id,
                        )
                        .await;
                }
                return (
                    DispatchResult::RegistryFailed(error),
                    started.elapsed().as_millis() as u64,
                );
            }
        }
    }
    if route_waits_for_user(route) {
        match state
            .runtime_invocations
            .mark_waiting_for_user(invocation_id)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                if acquired_turn {
                    let run_id = snapshot
                        .run_id
                        .as_deref()
                        .expect("acquired run invocation turn requires run_id");
                    if let Err(error) = state
                        .runtime_execution_scopes
                        .release_invocation_turn(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            snapshot.execution_scope_provider(),
                            invocation_id,
                        )
                        .await
                    {
                        return (
                            DispatchResult::RegistryFailed(error),
                            started.elapsed().as_millis() as u64,
                        );
                    }
                }
                return (
                    DispatchResult::CancelRequested,
                    started.elapsed().as_millis() as u64,
                );
            }
            Err(error) => {
                if acquired_turn {
                    let run_id = snapshot
                        .run_id
                        .as_deref()
                        .expect("acquired run invocation turn requires run_id");
                    if let Err(release_error) = state
                        .runtime_execution_scopes
                        .release_invocation_turn(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            snapshot.execution_scope_provider(),
                            invocation_id,
                        )
                        .await
                    {
                        return (
                            DispatchResult::RegistryFailed(format!(
                                "{error}; release invocation turn failed: {release_error}"
                            )),
                            started.elapsed().as_millis() as u64,
                        );
                    }
                }
                return (
                    DispatchResult::RegistryFailed(error),
                    started.elapsed().as_millis() as u64,
                );
            }
        }
    }
    let dispatch = {
        let outcome = state.providers.call_tool(
            snapshot,
            route,
            tool.original_name.as_str(),
            arguments,
            invocation_id,
        );
        tokio::pin!(outcome);
        tokio::select! {
            outcome = &mut outcome => {
                match outcome {
                    Ok(success) => match state.runtime_invocations.complete(invocation_id, success.result.clone()).await {
                        Ok(true) => DispatchResult::Completed(Ok(success)),
                        Ok(false) => DispatchResult::CancelRequested,
                        Err(error) => DispatchResult::RegistryFailed(error),
                    },
                    Err(error) => match state.runtime_invocations.fail(invocation_id, error.code, error.message.clone()).await {
                        Ok(true) => DispatchResult::Completed(Err(error)),
                        Ok(false) => DispatchResult::CancelRequested,
                        Err(registry_error) => DispatchResult::RegistryFailed(registry_error),
                    },
                }
            }
            cancellation = wait_for_cancellation(state, invocation_id) => {
                match cancellation {
                    Ok(()) => DispatchResult::CancelRequested,
                    Err(error) => DispatchResult::RegistryFailed(error),
                }
            }
        }
    };
    if acquired_turn {
        let run_id = snapshot
            .run_id
            .as_deref()
            .expect("acquired run invocation turn requires run_id");
        if let Err(error) = state
            .runtime_execution_scopes
            .release_invocation_turn(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id,
            )
            .await
        {
            return (
                DispatchResult::RegistryFailed(error),
                started.elapsed().as_millis() as u64,
            );
        }
    }
    (dispatch, started.elapsed().as_millis() as u64)
}

fn route_waits_for_user(route: &ResolvedMcpRoute) -> bool {
    route.resource_id == system_mcp_descriptor(SystemMcpKey::AskUser).resource_id
}

pub(crate) async fn execute_async_tool_call(
    state: AppState,
    snapshot: Arc<RuntimeSessionSnapshot>,
    route: ResolvedMcpRoute,
    tool: RuntimeToolDescriptor,
    arguments: Value,
    invocation_id: String,
    mutation_may_have_started: bool,
) -> Result<(), String> {
    let Some(record) = state
        .runtime_invocations
        .get_for_caller(invocation_id.as_str(), snapshot.caller_service.as_str())
        .await?
    else {
        return Ok(());
    };
    match record.status {
        RuntimeInvocationStatus::Completed
        | RuntimeInvocationStatus::Failed
        | RuntimeInvocationStatus::Cancelled
        | RuntimeInvocationStatus::UnknownExecutionState => {
            if let Some(run_id) = snapshot.run_id.as_deref() {
                state
                    .runtime_execution_scopes
                    .release_invocation_turn(
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_deref(),
                        run_id,
                        snapshot.execution_scope_provider(),
                        invocation_id.as_str(),
                    )
                    .await?;
            }
            return Ok(());
        }
        RuntimeInvocationStatus::Running | RuntimeInvocationStatus::WaitingForUser => {
            return Ok(());
        }
        RuntimeInvocationStatus::CancelRequested if record.started_at_unix_ms.is_some() => {
            return Ok(());
        }
        RuntimeInvocationStatus::CancelRequested => {
            state
                .runtime_invocations
                .cancel_without_start(invocation_id.as_str())
                .await?;
            if let Some(run_id) = snapshot.run_id.as_deref() {
                state
                    .runtime_execution_scopes
                    .release_invocation_turn(
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_deref(),
                        run_id,
                        snapshot.execution_scope_provider(),
                        invocation_id.as_str(),
                    )
                    .await?;
            }
            return Ok(());
        }
        RuntimeInvocationStatus::Queued => {}
    }
    if snapshot.run_id.is_none() {
        match state
            .runtime_invocations
            .mark_running(invocation_id.as_str())
            .await
        {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(error) => {
                tracing::error!(
                    invocation_id = invocation_id.as_str(),
                    error = error.as_str(),
                    "mark queued Runtime Invocation as running failed"
                );
                return Err(format!(
                    "mark queued Runtime Invocation as running failed: {error}"
                ));
            }
        }
    }
    let (dispatch, duration_ms) = dispatch_provider_call(
        &state,
        &snapshot,
        &route,
        &tool,
        arguments,
        invocation_id.as_str(),
    )
    .await;
    match dispatch {
        DispatchResult::AlreadyRunning => {}
        DispatchResult::CancelledBeforeStart => {
            record_tool_access_audit(
                &snapshot,
                &route,
                tool.exposed_name.as_str(),
                "cancelled_before_start",
            );
        }
        DispatchResult::CancelRequested => {
            let _ = handle_cancelled_tool_call(
                Value::Null,
                &snapshot,
                &route,
                tool.exposed_name.as_str(),
                invocation_id.as_str(),
                mutation_may_have_started,
                duration_ms,
                &state,
            )
            .await;
        }
        DispatchResult::RegistryFailed(error) => {
            record_tool_access_audit(
                &snapshot,
                &route,
                tool.exposed_name.as_str(),
                "registry_failed",
            );
            tracing::error!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                error = error.as_str(),
                status = "registry_failed",
                "async MCP Provider invocation coordination failed"
            );
        }
        DispatchResult::Completed(Ok(outcome)) => {
            record_tool_access_audit(&snapshot, &route, tool.exposed_name.as_str(), "succeeded");
            tracing::info!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                resource_id = route.resource_id.as_str(),
                exposed_tool_name = tool.exposed_name.as_str(),
                provider_kind = route.provider_kind.as_str(),
                duration_ms,
                result_bytes = outcome.response_bytes,
                status = "succeeded",
                mode = "async",
                "async MCP Provider invocation completed"
            );
        }
        DispatchResult::Completed(Err(error)) => {
            record_tool_access_audit(&snapshot, &route, tool.exposed_name.as_str(), "failed");
            tracing::warn!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                resource_id = route.resource_id.as_str(),
                exposed_tool_name = tool.exposed_name.as_str(),
                provider_kind = route.provider_kind.as_str(),
                duration_ms,
                error_code = error.code,
                status = "failed",
                mode = "async",
                "async MCP Provider invocation failed"
            );
        }
    }
    Ok(())
}

pub(super) fn cancel_response_status(record: &RuntimeInvocationRecord) -> &'static str {
    cancellation::cancel_response_status(record)
}

fn grant_matches_snapshot(
    claims: &crate::runtime::RuntimeGrantClaims,
    snapshot: &RuntimeSessionSnapshot,
) -> bool {
    let claim_resource_ids = claims
        .allowed_resource_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let snapshot_resource_ids = snapshot
        .routes
        .iter()
        .map(|route| route.resource_id.as_str())
        .collect::<BTreeSet<_>>();
    claims.session_id == snapshot.session_id
        && claims.sub == snapshot.caller_service
        && claims.trace_id == snapshot.trace_id
        && claims.tenant_id == snapshot.tenant_id
        && claims.owner_user_id == snapshot.owner_user_id
        && claims.agent_key == snapshot.agent_key
        && claims.task_profile == snapshot.task_profile
        && claims.project_id == snapshot.project_id
        && claims.device_id == snapshot.device_id
        && claims.run_id == snapshot.run_id
        && claims.turn_id == snapshot.turn_id
        && claims.task_id == snapshot.task_id
        && claims.source_session_id == snapshot.source_session_id
        && claims.source_user_message_id == snapshot.source_user_message_id
        && claims.contact_agent_id == snapshot.contact_agent_id
        && claims.default_model_config_id == snapshot.default_model_config_id
        && claims.expected_project_task_ids == snapshot.expected_project_task_ids
        && claims.policy_revision == snapshot.policy_revision
        && claims.route_revision == snapshot.route_revision
        && i64::try_from(claims.exp).ok() == Some(snapshot.expires_at_unix)
        && claim_resource_ids == snapshot_resource_ids
}

#[cfg(test)]
mod tests;
