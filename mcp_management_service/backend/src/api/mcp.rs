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
    McpToolCallResult, McpToolCallResultItem, McpToolCallResultStatus, MCP_ERROR_AUTH_REQUIRED,
    MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS, MCP_ERROR_METHOD_NOT_FOUND, METHOD_INITIALIZE,
    METHOD_NOTIFICATIONS_CANCELLED, METHOD_NOTIFICATIONS_INITIALIZED, METHOD_PING,
    METHOD_TOOLS_LIST,
};
use mongodb::bson::DateTime;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::capabilities::route_allows_system_tool;
use crate::runtime::{
    RuntimeExecutionTurnState, RuntimeInvocationRecord, RuntimeInvocationRegisterError,
    RuntimeInvocationStatus, RuntimeSessionSnapshot,
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

pub(crate) async fn execute_tool_call_command(
    state: &AppState,
    command: &McpToolCallCommand,
) -> Result<McpToolCallResult, String> {
    let claims = state
        .runtime_grants
        .verify(command.runtime_token.trim())
        .map_err(|_| "runtime session bearer token is invalid or expired".to_string())?;
    let snapshot = state
        .runtime_sessions
        .get(claims.session_id.as_str())
        .await?
        .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
    if !grant_matches_snapshot(&claims, &snapshot) {
        return Err("runtime session grant does not match its route snapshot".to_string());
    }
    if command.calls.is_empty() || command.calls.len() > 128 {
        return Err("MCP tool call command must contain between 1 and 128 calls".to_string());
    }
    if command.batch_id.trim().is_empty() || command.batch_id.len() > 200 {
        return Err("MCP tool call command batch_id is invalid".to_string());
    }

    struct RegisteredCall {
        call_index: usize,
        route: ResolvedMcpRoute,
        tool: RuntimeToolDescriptor,
        arguments: Value,
        mutation_may_have_started: bool,
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
                return Err(format!(
                    "MCP invocation {} is already active; retry the command after it becomes terminal",
                    call.invocation_id
                ));
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
            Ok(()) => registered.push(RegisteredCall {
                call_index,
                route,
                tool,
                arguments: call.arguments.clone(),
                mutation_may_have_started,
            }),
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
                snapshot.project_id.as_str(),
                run_id,
                snapshot.project_context.workspace_provider,
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

    for registered_call in registered {
        let call = &command.calls[registered_call.call_index];
        execute_async_tool_call(
            state.clone(),
            snapshot.clone(),
            registered_call.route,
            registered_call.tool,
            registered_call.arguments,
            call.invocation_id.clone(),
            registered_call.mutation_may_have_started,
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
        results[registered_call.call_index] = Some(result_item_from_record(call, record));
    }

    Ok(McpToolCallResult {
        event_id: format!("mcp_batch_result_{}", Uuid::new_v4().simple()),
        batch_id: command.batch_id.clone(),
        session_id: snapshot.session_id.clone(),
        run_id: snapshot.run_id.clone(),
        items: results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                result.unwrap_or_else(|| {
                    failed_command_item(
                        &command.calls[index],
                        MCP_ERROR_INTERNAL,
                        "MCP invocation did not reach a terminal state".to_string(),
                    )
                })
            })
            .collect(),
    })
}

fn failed_command_item(
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

fn result_item_from_record(
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
                snapshot.project_id.as_str(),
                run_id,
                snapshot.project_context.workspace_provider,
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
                snapshot.project_id.as_str(),
                run_id,
                snapshot.project_context.workspace_provider,
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
        project_id: Some(snapshot.project_id.clone()),
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
                snapshot.project_id.as_str(),
                run_id,
                snapshot.project_context.workspace_provider,
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
                    snapshot.project_id.as_str(),
                    run_id,
                    snapshot.project_context.workspace_provider,
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
                            snapshot.project_id.as_str(),
                            run_id,
                            snapshot.project_context.workspace_provider,
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
                            snapshot.project_id.as_str(),
                            run_id,
                            snapshot.project_context.workspace_provider,
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
                            snapshot.project_id.as_str(),
                            run_id,
                            snapshot.project_context.workspace_provider,
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
                snapshot.project_id.as_str(),
                run_id,
                snapshot.project_context.workspace_provider,
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
                        snapshot.project_id.as_str(),
                        run_id,
                        snapshot.project_context.workspace_provider,
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
                        snapshot.project_id.as_str(),
                        run_id,
                        snapshot.project_context.workspace_provider,
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
