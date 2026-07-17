// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use super::internal_auth::{
    require_project_internal_request, PROJECT_READ_SCOPE, PROJECT_SERVICE_CALLER,
    TASK_RUNNER_CALLER,
};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse};
use crate::services::runtime_environment::{
    apply_program_managed_image_policy, default_runtime_environment_for_project,
    refresh_environment_variable_values,
};
use crate::state::AppState;

const SERVER_NAME: &str = "project_runtime_environment";
const PROTOCOL_VERSION: &str = "2024-11-05";
const TOOL_NAME: &str = "get_project_runtime_environment_info";
const TASK_RUNNER_PROJECT_ID_HEADER: &str = "x-task-runner-project-id";

pub(in crate::api) async fn project_runtime_environment_mcp_entrypoint(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(err) = require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER, PROJECT_SERVICE_CALLER],
        PROJECT_READ_SCOPE,
    ) {
        return Json(mcp_server::jsonrpc_error_response(
            err.status,
            id,
            err.message,
        ));
    }
    if let Err(message) = ensure_project_header_matches(&headers, project_id.as_str()) {
        return Json(mcp_server::jsonrpc_error_response(
            StatusCode::FORBIDDEN,
            id,
            message,
        ));
    }
    Json(handle_jsonrpc(state, project_id, request).await)
}

async fn handle_jsonrpc(
    state: AppState,
    project_id: String,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": {
                "name": SERVER_NAME,
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": { "tools": {} }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": chatos_mcp_runtime::project_runtime_environment_info_tool_definitions()
        })),
        "tools/call" => call_tool(&state, project_id.as_str(), request.params).await,
        method => Err(format!("unsupported MCP method: {method}")),
    };
    match result {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        Err(message) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(crate::mcp_server::JsonRpcError {
                code: -32000,
                message,
            }),
        },
    }
}

async fn call_tool(
    state: &AppState,
    project_id: &str,
    params: Option<Value>,
) -> Result<Value, String> {
    let params = params.unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "tools/call params.name is required".to_string())?;
    if name != TOOL_NAME {
        return Err(format!("Tool not found: {name}"));
    }
    let project = state
        .store
        .get_project(project_id)
        .await?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    let mut environment = state
        .store
        .get_project_runtime_environment(project_id)
        .await?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));
    refresh_environment_variable_values(&mut environment);
    let mut images = state
        .store
        .list_project_runtime_environment_images(project_id)
        .await?;
    for image in &mut images {
        apply_program_managed_image_policy(image);
    }
    Ok(tool_result(json!({
        "project_id": project_id,
        "environment": environment,
        "images": images,
    })))
}

fn tool_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "_structured_result": payload,
        "isError": false
    })
}

fn ensure_project_header_matches(headers: &HeaderMap, project_id: &str) -> Result<(), String> {
    let Some(header_project_id) = headers
        .get(TASK_RUNNER_PROJECT_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if header_project_id == project_id.trim() {
        Ok(())
    } else {
        Err("x-task-runner-project-id does not match request project id".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn task_runner_project_header_must_match_route() {
        let mut headers = HeaderMap::new();
        headers.insert(
            TASK_RUNNER_PROJECT_ID_HEADER,
            HeaderValue::from_static("project-1"),
        );
        assert!(ensure_project_header_matches(&headers, "project-1").is_ok());
        assert!(ensure_project_header_matches(&headers, "project-2").is_err());
    }
}
