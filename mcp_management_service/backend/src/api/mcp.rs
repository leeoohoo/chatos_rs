// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::time::Instant;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::McpProviderKind;
use chatos_mcp_service::{
    jsonrpc_error, jsonrpc_ok, JsonRpcRequest, JsonRpcResponse, MCP_ERROR_AUTH_REQUIRED,
    MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS, MCP_ERROR_METHOD_NOT_FOUND, METHOD_INITIALIZE,
    METHOD_NOTIFICATIONS_INITIALIZED, METHOD_PING, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::capabilities::route_allows_system_tool;
use crate::runtime::RuntimeSessionSnapshot;
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
    let Some(snapshot) = state.runtime_sessions.get(claims.session_id.as_str()).await else {
        return Json(jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "runtime session was not found or has expired",
        ));
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
    let started = Instant::now();
    let outcome = state
        .providers
        .call_tool(
            snapshot,
            route,
            tool.original_name.as_str(),
            arguments,
            invocation_id.as_str(),
        )
        .await;
    let duration_ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(outcome) => {
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
        Err(error) => {
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
        && claims.project_id == snapshot.project_id
        && claims.run_id == snapshot.run_id
        && claims.turn_id == snapshot.turn_id
        && claims.task_id == snapshot.task_id
        && claims.source_session_id == snapshot.source_session_id
        && claims.source_user_message_id == snapshot.source_user_message_id
        && claims.default_model_config_id == snapshot.default_model_config_id
        && claims.expected_project_task_ids == snapshot.expected_project_task_ids
        && claims.policy_revision == snapshot.policy_revision
        && claims.route_revision == snapshot.route_revision
        && i64::try_from(claims.exp).ok() == Some(snapshot.expires_at_unix)
        && claim_resource_ids == snapshot_resource_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
        RuntimeToolDescriptor, SandboxProviderKind, WorkspaceProviderKind,
    };

    fn snapshot() -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: None,
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                execution_plane: ExecutionPlane::Cloud,
                workspace_provider: WorkspaceProviderKind::Harness,
                workspace: None,
                sandbox_provider: SandboxProviderKind::Cloud,
                sandbox_pairing_id: None,
                source_type: Some("cloud".to_string()),
                revision: "project-revision".to_string(),
            },
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: vec![ResolvedMcpRoute {
                resource_id: "mcp-1".to_string(),
                server_name: "demo".to_string(),
                provider_kind: McpProviderKind::ExternalHttp,
                provider_ref: Some("mcp-resource:mcp-1".to_string()),
                tool_namespace: "demo".to_string(),
                allow_writes: false,
                retry_class: McpRetryClass::IdempotentRead,
                cancel_supported: true,
                reason: "test".to_string(),
            }],
            tools: vec![RuntimeToolDescriptor {
                exposed_name: "demo_search".to_string(),
                original_name: "search".to_string(),
                resource_id: "mcp-1".to_string(),
                definition: json!({"name": "demo_search", "inputSchema": {"type": "object"}}),
            }],
            external_http_bindings: Default::default(),
            cloud_stdio_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: i64::MAX,
        }
    }

    #[tokio::test]
    async fn tools_list_returns_only_session_namespaced_tools() {
        let state = AppState::new(crate::config::AppConfig::test()).unwrap();
        let response = handle_session_request(
            JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(1)),
                method: METHOD_TOOLS_LIST.to_string(),
                params: json!({}),
            },
            &snapshot(),
            &state,
        )
        .await;
        assert_eq!(
            response
                .result
                .as_ref()
                .and_then(|value| value.pointer("/tools/0/name"))
                .and_then(Value::as_str),
            Some("demo_search")
        );
    }

    #[tokio::test]
    async fn tools_call_dispatches_the_original_name_to_project_service() {
        async fn provider(Json(request): Json<Value>) -> Json<Value> {
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": {
                    "called": request.pointer("/params/name"),
                    "arguments": request.pointer("/params/arguments")
                }
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(provider)))
                .await
                .unwrap();
        });
        let mut config = crate::config::AppConfig::test();
        config.project_service_base_url = format!("http://{address}");
        let state = AppState::new(config).unwrap();
        let mut snapshot = snapshot();
        snapshot.routes = vec![ResolvedMcpRoute {
            resource_id: "builtin_project_management".to_string(),
            server_name: "project_management_service".to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some("project_management_service".to_string()),
            tool_namespace: "project_management_service".to_string(),
            allow_writes: true,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: true,
            reason: "test".to_string(),
        }];
        snapshot.tools = vec![RuntimeToolDescriptor {
            exposed_name: "project_management_service_list_requirements".to_string(),
            original_name: "list_requirements".to_string(),
            resource_id: "builtin_project_management".to_string(),
            definition: json!({
                "name": "project_management_service_list_requirements",
                "inputSchema": {"type": "object"}
            }),
        }];
        let response = handle_session_request(
            JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(2)),
                method: METHOD_TOOLS_CALL.to_string(),
                params: json!({
                    "name": "project_management_service_list_requirements",
                    "arguments": {"status": "draft"}
                }),
            },
            &snapshot,
            &state,
        )
        .await;
        assert_eq!(
            response.result,
            Some(json!({
                "called": "list_requirements",
                "arguments": {"status": "draft"}
            }))
        );
        server.abort();
    }

    #[test]
    fn runtime_grant_rejects_scope_and_resource_drift() {
        let snapshot = snapshot();
        let claims = crate::runtime::RuntimeGrantClaims {
            iss: "mcp-management-service".to_string(),
            sub: snapshot.caller_service.clone(),
            aud: "mcp-management-runtime".to_string(),
            session_id: snapshot.session_id.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            agent_key: snapshot.agent_key.clone(),
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            turn_id: snapshot.turn_id.clone(),
            task_id: snapshot.task_id.clone(),
            source_session_id: snapshot.source_session_id.clone(),
            source_user_message_id: snapshot.source_user_message_id.clone(),
            default_model_config_id: snapshot.default_model_config_id.clone(),
            expected_project_task_ids: snapshot.expected_project_task_ids.clone(),
            policy_revision: snapshot.policy_revision.clone(),
            route_revision: snapshot.route_revision.clone(),
            allowed_resource_ids: vec!["mcp-1".to_string()],
            iat: 1,
            exp: usize::try_from(snapshot.expires_at_unix).unwrap(),
        };
        assert!(grant_matches_snapshot(&claims, &snapshot));

        let mut wrong_task = claims.clone();
        wrong_task.task_id = Some("another-task".to_string());
        assert!(!grant_matches_snapshot(&wrong_task, &snapshot));

        let mut extra_resource = claims;
        extra_resource
            .allowed_resource_ids
            .push("unconfigured-mcp".to_string());
        assert!(!grant_matches_snapshot(&extra_resource, &snapshot));
    }
}
