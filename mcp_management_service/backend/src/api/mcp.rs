// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::time::Duration;
use std::time::Instant;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::McpProviderKind;
use chatos_mcp_service::{
    jsonrpc_error, jsonrpc_ok, CancelledNotificationParams, JsonRpcRequest, JsonRpcResponse,
    MCP_ERROR_AUTH_REQUIRED, MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS,
    MCP_ERROR_INVOCATION_CANCELLED, MCP_ERROR_METHOD_NOT_FOUND, MCP_ERROR_UNKNOWN_EXECUTION_STATE,
    METHOD_INITIALIZE, METHOD_NOTIFICATIONS_CANCELLED, METHOD_NOTIFICATIONS_INITIALIZED,
    METHOD_PING, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST,
};
use mongodb::bson::DateTime;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::capabilities::route_allows_system_tool;
use crate::providers::ProviderCancelOutcome;
use crate::runtime::{RuntimeInvocationRecord, RuntimeInvocationStatus, RuntimeSessionSnapshot};
use crate::state::AppState;

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
        METHOD_TOOLS_CALL => handle_tool_call(id, request.params, snapshot, state).await,
        other => jsonrpc_error(
            id,
            MCP_ERROR_METHOD_NOT_FOUND,
            format!("method not found: {other}"),
        ),
    }
}

async fn handle_tool_call(
    id: Value,
    params: Value,
    snapshot: &RuntimeSessionSnapshot,
    state: &AppState,
) -> JsonRpcResponse {
    let request_id_key = match request_id_key(&id) {
        Ok(value) => value,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(name) = name else {
        return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, "tools/call.name is required");
    };
    let Some(tool) = snapshot.tools.iter().find(|tool| tool.exposed_name == name) else {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            format!("tool not found: {name}"),
        );
    };
    let Some(route) = snapshot
        .routes
        .iter()
        .find(|route| route.resource_id == tool.resource_id)
    else {
        return jsonrpc_error(id, MCP_ERROR_INTERNAL, "tool route snapshot is missing");
    };
    if !route_allows_system_tool(route, tool.original_name.as_str()) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "tool is blocked by the immutable read-only route policy",
        );
    }
    if route.provider_kind == McpProviderKind::Unavailable {
        return jsonrpc_error(
            id,
            MCP_ERROR_INTERNAL,
            format!("provider unavailable: {}", route.reason),
        );
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "tools/call.arguments must be an object",
        );
    }
    let invocation_id = format!("mcp_invocation_{}", Uuid::new_v4().simple());
    let mutation_may_have_started = route.allow_writes
        && tool
            .definition
            .pointer("/annotations/readOnlyHint")
            .and_then(Value::as_bool)
            != Some(true);
    let invocation = RuntimeInvocationRecord {
        invocation_id: invocation_id.clone(),
        session_id: snapshot.session_id.clone(),
        request_id_key,
        caller_service: snapshot.caller_service.clone(),
        resource_id: route.resource_id.clone(),
        exposed_tool_name: tool.exposed_name.clone(),
        mutation_may_have_started,
        cancel_supported: route.cancel_supported,
        status: RuntimeInvocationStatus::Running,
        created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
        expires_at: DateTime::from_millis(snapshot.expires_at_unix.saturating_mul(1_000)),
        expires_at_unix: snapshot.expires_at_unix,
    };
    if let Err(error) = state.runtime_invocations.register(invocation).await {
        tracing::error!(
            invocation_id = invocation_id.as_str(),
            session_id = snapshot.session_id.as_str(),
            error = error.as_str(),
            "register Runtime Invocation failed"
        );
        return jsonrpc_error(
            id,
            MCP_ERROR_INTERNAL,
            "runtime invocation registry is unavailable or request id is already active",
        );
    }
    let started = Instant::now();
    let dispatch = {
        let outcome = state.providers.call_tool(
            snapshot,
            route,
            tool.original_name.as_str(),
            arguments,
            invocation_id.as_str(),
        );
        tokio::pin!(outcome);
        tokio::select! {
            outcome = &mut outcome => {
                match state.runtime_invocations.finish_if_running(invocation_id.as_str()).await {
                    Ok(true) => DispatchResult::Completed(outcome),
                    Ok(false) => DispatchResult::CancelRequested,
                    Err(error) => DispatchResult::RegistryFailed(error),
                }
            }
            cancellation = wait_for_cancellation(state, invocation_id.as_str()) => {
                match cancellation {
                    Ok(()) => DispatchResult::CancelRequested,
                    Err(error) => DispatchResult::RegistryFailed(error),
                }
            }
        }
    };
    let duration_ms = started.elapsed().as_millis() as u64;
    match dispatch {
        DispatchResult::CancelRequested => {
            handle_cancelled_tool_call(
                id,
                snapshot,
                route,
                tool.exposed_name.as_str(),
                invocation_id.as_str(),
                mutation_may_have_started,
                duration_ms,
                state,
            )
            .await
        }
        DispatchResult::RegistryFailed(error) => {
            tracing::error!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                error = error.as_str(),
                status = "registry_failed",
                "Runtime Invocation coordination failed"
            );
            jsonrpc_error(
                id,
                MCP_ERROR_INTERNAL,
                "runtime invocation registry is unavailable",
            )
        }
        DispatchResult::Completed(Ok(outcome)) => {
            tracing::info!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                resource_id = route.resource_id.as_str(),
                exposed_tool_name = tool.exposed_name.as_str(),
                provider_kind = route.provider_kind.as_str(),
                duration_ms,
                result_bytes = outcome.response_bytes,
                status = "succeeded",
                "MCP Provider invocation completed"
            );
            jsonrpc_ok(id, outcome.result)
        }
        DispatchResult::Completed(Err(error)) => {
            tracing::warn!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                resource_id = route.resource_id.as_str(),
                exposed_tool_name = tool.exposed_name.as_str(),
                provider_kind = route.provider_kind.as_str(),
                duration_ms,
                error_code = error.code,
                status = "failed",
                "MCP Provider invocation failed"
            );
            jsonrpc_error(id, error.code, error.message)
        }
    }
}

enum DispatchResult {
    Completed(Result<crate::providers::ProviderCallOutcome, crate::providers::ProviderCallError>),
    CancelRequested,
    RegistryFailed(String),
}

async fn wait_for_cancellation(state: &AppState, invocation_id: &str) -> Result<(), String> {
    loop {
        if state
            .runtime_invocations
            .cancellation_requested(invocation_id)
            .await?
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_cancelled_tool_call(
    id: Value,
    snapshot: &RuntimeSessionSnapshot,
    route: &chatos_mcp_management_sdk::ResolvedMcpRoute,
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

async fn handle_cancel_notification(
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
        RuntimeInvocationStatus::Running | RuntimeInvocationStatus::CancelRequested => {
            if record.mutation_may_have_started && !record.cancel_supported {
                "unknown_execution_state"
            } else {
                "cancel_requested"
            }
        }
        RuntimeInvocationStatus::Completed => "already_completed",
        RuntimeInvocationStatus::Cancelled => "cancelled",
        RuntimeInvocationStatus::UnknownExecutionState => "unknown_execution_state",
    }
}

fn request_id_key(id: &Value) -> Result<String, &'static str> {
    if !matches!(id, Value::String(_) | Value::Number(_)) {
        return Err("JSON-RPC request id must be a string or number");
    }
    serde_json::to_string(id).map_err(|_| "JSON-RPC request id is invalid")
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
        && claims.owner_user_id == snapshot.owner_user_id
        && claims.agent_key == snapshot.agent_key
        && claims.task_profile == snapshot.task_profile
        && claims.project_id == snapshot.project_id
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
