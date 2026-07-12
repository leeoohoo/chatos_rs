// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chatos_mcp_service::{HostCapabilityPolicy, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER};
use reqwest::Method;
use serde_json::{json, Value};

use super::internal_auth::{
    require_project_internal_request, CHATOS_CALLER, PROJECT_HARNESS_SCOPE, PROJECT_SERVICE_CALLER,
    TASK_RUNNER_CALLER,
};
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse};
use crate::models::{ProjectImportStatus, ProjectRecord, ProjectStatus};
use crate::state::AppState;

mod client;
mod patch_targets;
mod path_policy;
mod text_edit;
mod tool_definitions;
mod tools;

use self::client::HarnessApiAccessResponse;
use self::tool_definitions::tool_definitions;
use self::tools::{
    tool_append_file, tool_apply_patch, tool_delete_path, tool_edit_file, tool_list_branches,
    tool_list_dir, tool_read_file_range, tool_read_file_raw, tool_search_text, tool_write_file,
};

const SERVER_NAME: &str = "harness_code";
const PROTOCOL_VERSION: &str = "2024-11-05";
const TASK_RUNNER_PROJECT_ID_HEADER: &str = "x-task-runner-project-id";
const DEFAULT_MAX_WRITE_BYTES: i64 = 5 * 1024 * 1024;

#[derive(Debug)]
struct HarnessMcpContext {
    project_id: String,
    repo_path: String,
    access: HarnessApiAccessResponse,
    client: reqwest::Client,
    enabled_tools: HostCapabilityPolicy,
}

pub(in crate::api) async fn harness_project_mcp_entrypoint(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(err) = require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER, PROJECT_SERVICE_CALLER],
        PROJECT_HARNESS_SCOPE,
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
    Json(handle_harness_jsonrpc(state, project_id, headers, request).await)
}

async fn handle_harness_jsonrpc(
    state: AppState,
    project_id: String,
    headers: HeaderMap,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let enabled_tools = enabled_harness_tools_from_headers(&headers);
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": {
                "name": SERVER_NAME,
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions(&enabled_tools) })),
        "tools/call" => match build_harness_mcp_context(state, project_id, enabled_tools).await {
            Ok(ctx) => call_harness_tool(&ctx, request.params.unwrap_or_else(|| json!({}))).await,
            Err(err) => Err(err),
        },
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

async fn build_harness_mcp_context(
    state: AppState,
    project_id: String,
    enabled_tools: HostCapabilityPolicy,
) -> Result<HarnessMcpContext, String> {
    if !enabled_tools.code_read && !enabled_tools.code_write {
        return Err("Harness MCP has no enabled CodeMaintainer capabilities".to_string());
    }
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(|err| format!("load project failed: {err}"))?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    ensure_harness_project_ready(&project)?;
    let repo_path = project
        .harness_repo_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "project is missing harness_repo_path".to_string())?
        .to_string();
    let owner_user_id = project_owner_user_id(&project)?;
    let access = fetch_harness_api_access(&state, owner_user_id.as_str()).await?;
    ensure_harness_space_matches(&project, &access)?;
    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build Harness MCP HTTP client failed: {err}"))?;
    Ok(HarnessMcpContext {
        project_id,
        repo_path,
        access,
        client,
        enabled_tools,
    })
}

fn ensure_harness_project_ready(project: &ProjectRecord) -> Result<(), String> {
    if project.status == ProjectStatus::Archived {
        return Err("project is archived".to_string());
    }
    if project.import_status != ProjectImportStatus::Ready {
        return Err(format!(
            "project Harness import is not ready: {}",
            project.import_status.as_str()
        ));
    }
    if project
        .harness_repo_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err("project is not linked to a Harness repo".to_string());
    }
    Ok(())
}

fn project_owner_user_id(project: &ProjectRecord) -> Result<String, String> {
    project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "project owner user id is missing".to_string())
}

fn ensure_harness_space_matches(
    project: &ProjectRecord,
    access: &HarnessApiAccessResponse,
) -> Result<(), String> {
    let Some(project_space) = project
        .harness_space_identifier
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if access.space_identifier.trim().is_empty() || access.space_identifier.trim() == project_space
    {
        return Ok(());
    }
    Err("Harness access token owner does not match project Harness space".to_string())
}

async fn fetch_harness_api_access(
    state: &AppState,
    owner_user_id: &str,
) -> Result<HarnessApiAccessResponse, String> {
    let secret = state
        .config
        .user_service_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET is not configured".to_string()
        })?;
    let endpoint = format!(
        "{}/api/internal/harness/users/{}/access",
        state
            .config
            .user_service_base_url
            .trim()
            .trim_end_matches('/'),
        urlencoding::encode(owner_user_id.trim())
    );
    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response = crate::user_model_runtime_client::signed_user_service_request(
        client.request(Method::GET, endpoint),
        secret,
        crate::user_model_runtime_client::HARNESS_ACCESS_READ_SCOPE,
    )?
    .send()
    .await
    .map_err(|err| format!("user_service Harness access request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!(
            "user_service Harness access request failed: {status} {text}"
        ));
    }
    response
        .json::<HarnessApiAccessResponse>()
        .await
        .map_err(|err| format!("parse user_service Harness access response failed: {err}"))
}

async fn call_harness_tool(ctx: &HarnessMcpContext, params: Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "tools/call params.name is required".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "read_file_raw" => {
            ensure_read_allowed(ctx)?;
            tool_read_file_raw(ctx, &arguments).await
        }
        "read_file_range" => {
            ensure_read_allowed(ctx)?;
            tool_read_file_range(ctx, &arguments).await
        }
        "list_dir" => {
            ensure_read_allowed(ctx)?;
            tool_list_dir(ctx, &arguments).await
        }
        "list_branches" => {
            ensure_read_allowed(ctx)?;
            tool_list_branches(ctx, &arguments).await
        }
        "search_text" => {
            ensure_read_allowed(ctx)?;
            tool_search_text(ctx, &arguments).await
        }
        "write_file" => {
            ensure_write_allowed(ctx)?;
            tool_write_file(ctx, &arguments).await
        }
        "edit_file" => {
            ensure_write_allowed(ctx)?;
            tool_edit_file(ctx, &arguments).await
        }
        "append_file" => {
            ensure_write_allowed(ctx)?;
            tool_append_file(ctx, &arguments).await
        }
        "delete_path" => {
            ensure_write_allowed(ctx)?;
            tool_delete_path(ctx, &arguments).await
        }
        "apply_patch" => {
            ensure_write_allowed(ctx)?;
            tool_apply_patch(ctx, &arguments).await
        }
        other => Err(format!("Tool not found: {other}")),
    }
}

fn enabled_harness_tools_from_headers(headers: &HeaderMap) -> HostCapabilityPolicy {
    header_text(headers, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER)
        .map(|raw| HostCapabilityPolicy::from_header_value(raw.as_str()))
        .unwrap_or_default()
}

fn ensure_project_header_matches(headers: &HeaderMap, project_id: &str) -> Result<(), String> {
    let Some(header_project_id) = header_text(headers, TASK_RUNNER_PROJECT_ID_HEADER) else {
        return Ok(());
    };
    if header_project_id == project_id.trim() {
        Ok(())
    } else {
        Err("x-task-runner-project-id does not match request project id".to_string())
    }
}

fn ensure_read_allowed(ctx: &HarnessMcpContext) -> Result<(), String> {
    if ctx.enabled_tools.code_read {
        Ok(())
    } else {
        Err("Harness MCP read capability is not enabled for this task".to_string())
    }
}

fn ensure_write_allowed(ctx: &HarnessMcpContext) -> Result<(), String> {
    if ctx.enabled_tools.code_write {
        Ok(())
    } else {
        Err("Harness MCP write capability is not enabled for this task".to_string())
    }
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required"))
}

fn ensure_write_size(content: &str) -> Result<(), String> {
    if content.len() as i64 > DEFAULT_MAX_WRITE_BYTES {
        Err("Write exceeds max-write-bytes limit.".to_string())
    } else {
        Ok(())
    }
}

fn tool_text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "_structured_result": payload,
        "isError": false
    })
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

trait ProjectImportStatusExt {
    fn as_str(&self) -> &'static str;
}

impl ProjectImportStatusExt for ProjectImportStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ProjectImportStatus::None => "none",
            ProjectImportStatus::Pending => "pending",
            ProjectImportStatus::Importing => "importing",
            ProjectImportStatus::Ready => "ready",
            ProjectImportStatus::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_tools_write_implies_read() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
            "CodeMaintainerWrite".parse().unwrap(),
        );
        let enabled = enabled_harness_tools_from_headers(&headers);
        assert!(enabled.code_read);
        assert!(enabled.code_write);
    }
}
