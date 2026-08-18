// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chatos_mcp::code_maintainer::{classify_file_modification_error, FileModificationOutcome};
use chatos_mcp_service::{HostCapabilityPolicy, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER};
use reqwest::Method;
use serde_json::{json, Value};

use super::internal_auth::{
    record_project_internal_resource_access, require_project_internal_request,
    ProjectInternalRequestIdentity, ProjectInternalResourceAudit, CHATOS_CALLER,
    MCP_MANAGEMENT_CALLER, PROJECT_HARNESS_SCOPE, PROJECT_SERVICE_CALLER, TASK_RUNNER_CALLER,
};
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse};
use crate::models::{ProjectImportStatus, ProjectRecord, ProjectStatus};
use crate::state::AppState;
use crate::trace_context::InternalTraceContextExt;
use chatos_service_runtime::http_body::{read_response_json_limited, JSON_BODY_LIMIT_BYTES};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};

mod client;
mod path_policy;
mod session;
mod text_edit;
mod tool_definitions;
mod tools;

use self::client::HarnessApiAccessResponse;
use self::session::{store_for_project, SharedEditSessionStore};
use self::tool_definitions::tool_definitions;
use self::tools::{
    tool_abort_edit_session, tool_commit_edit_session, tool_list_dir, tool_open_edit_session,
    tool_read_file_range, tool_read_file_raw, tool_search_text, tool_stage_edit_batch,
};

const SERVER_NAME: &str = "harness_code";
const PROTOCOL_VERSION: &str = "2024-11-05";
const TASK_RUNNER_PROJECT_ID_HEADER: &str = "x-task-runner-project-id";
const MCP_MANAGEMENT_RUN_ID_HEADER: &str = "x-mcp-management-run-id";
const MCP_MANAGEMENT_BRANCH_REF_HEADER: &str = "x-mcp-management-harness-branch-ref";
const DEFAULT_MAX_WRITE_BYTES: i64 = 5 * 1024 * 1024;

#[derive(Debug)]
struct HarnessMcpContext {
    project_id: String,
    repo_path: String,
    branch_ref: String,
    access: HarnessApiAccessResponse,
    client: reqwest::Client,
    enabled_tools: HostCapabilityPolicy,
    run_id: Option<String>,
    session_store: SharedEditSessionStore,
}

pub(in crate::api) async fn harness_project_mcp_entrypoint(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let identity = match require_project_internal_request(
        &state.config,
        &headers,
        &[
            CHATOS_CALLER,
            TASK_RUNNER_CALLER,
            PROJECT_SERVICE_CALLER,
            MCP_MANAGEMENT_CALLER,
        ],
        PROJECT_HARNESS_SCOPE,
    ) {
        Ok(identity) => identity,
        Err(err) => {
            return Json(mcp_server::jsonrpc_error_response(
                err.status,
                id,
                err.message,
            ));
        }
    };
    let write_tool = requested_harness_write_tool(&request).map(ToOwned::to_owned);
    let represented_user_id = represented_user_id_from_headers(&headers);
    if let Err(message) = ensure_project_header_matches(&headers, project_id.as_str()) {
        record_harness_write_audit(
            &identity,
            represented_user_id.as_deref(),
            project_id.as_str(),
            write_tool.as_deref(),
            "failed",
        );
        return Json(mcp_server::jsonrpc_error_response(
            StatusCode::FORBIDDEN,
            id,
            message,
        ));
    }
    let response = handle_harness_jsonrpc(
        state,
        project_id.clone(),
        identity.caller_service.as_str(),
        headers,
        request,
    )
    .await;
    let outcome = if response.error.is_some() {
        "failed"
    } else {
        "succeeded"
    };
    record_harness_write_audit(
        &identity,
        represented_user_id.as_deref(),
        project_id.as_str(),
        write_tool.as_deref(),
        outcome,
    );
    Json(response)
}

fn requested_harness_write_tool(request: &JsonRpcRequest) -> Option<&str> {
    if request.method != "tools/call" {
        return None;
    }
    request
        .params
        .as_ref()
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| {
            matches!(
                *name,
                "open_edit_session"
                    | "stage_edit_batch"
                    | "commit_edit_session"
                    | "abort_edit_session"
            )
        })
}

fn represented_user_id_from_headers(headers: &HeaderMap) -> Option<String> {
    [
        "x-task-runner-owner-user-id",
        "x-mcp-management-owner-user-id",
    ]
    .into_iter()
    .find_map(|key| header_text(headers, key))
}

fn record_harness_write_audit(
    identity: &ProjectInternalRequestIdentity,
    represented_user_id: Option<&str>,
    project_id: &str,
    tool_name: Option<&str>,
    outcome: &str,
) {
    let Some(tool_name) = tool_name else {
        return;
    };
    record_project_internal_resource_access(
        identity,
        ProjectInternalResourceAudit {
            represented_user_id,
            project_id: Some(project_id),
            resource_type: "project_workspace",
            resource_id: project_id,
            resource_name: Some(tool_name),
            action: tool_name,
            outcome,
        },
    );
}

async fn handle_harness_jsonrpc(
    state: AppState,
    project_id: String,
    caller_service: &str,
    headers: HeaderMap,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let enabled_tools = enabled_harness_tools_from_headers(&headers);
    let run_id = header_text(&headers, MCP_MANAGEMENT_RUN_ID_HEADER);
    let branch_ref = header_text(&headers, MCP_MANAGEMENT_BRANCH_REF_HEADER);
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
        "tools/call" => {
            match build_harness_mcp_context(
                state,
                project_id,
                caller_service,
                enabled_tools,
                run_id,
                branch_ref,
            )
            .await
            {
                Ok(ctx) => {
                    call_harness_tool(&ctx, request.params.unwrap_or_else(|| json!({}))).await
                }
                Err(err) => Err(err),
            }
        }
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
    caller_service: &str,
    enabled_tools: HostCapabilityPolicy,
    run_id: Option<String>,
    branch_ref: Option<String>,
) -> Result<HarnessMcpContext, String> {
    if !enabled_tools.code_read && !enabled_tools.code_write {
        return Err("project workspace has no enabled capabilities".to_string());
    }
    if enabled_tools.terminal {
        return Err("TerminalController cannot be routed to the Harness provider".to_string());
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
        .ok_or_else(|| "project source workspace is not available".to_string())?
        .to_string();
    let manual_project_access = caller_service == CHATOS_CALLER;
    let default_branch = project
        .harness_default_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    let branch_ref =
        resolve_harness_branch_ref(default_branch, branch_ref.as_deref(), manual_project_access)?;
    validate_harness_branch_target(
        &project,
        run_id.as_deref(),
        branch_ref.as_str(),
        enabled_tools.code_write,
        manual_project_access,
    )?;
    let owner_user_id = project_owner_user_id(&project)?;
    let access = fetch_harness_api_access(&state, owner_user_id.as_str()).await?;
    ensure_harness_space_matches(&project, &access)?;
    let client = build_http_client(HttpClientTimeouts::new(
        state.config.user_service_request_timeout,
    ))
    .map_err(|err| format!("build project workspace tool client failed: {err}"))?;
    let session_store =
        store_for_project(project_id.as_str(), repo_path.as_str(), branch_ref.as_str());
    Ok(HarnessMcpContext {
        project_id,
        repo_path,
        branch_ref,
        access,
        client,
        enabled_tools,
        run_id,
        session_store,
    })
}

fn ensure_harness_project_ready(project: &ProjectRecord) -> Result<(), String> {
    if project.status == ProjectStatus::Archived {
        return Err("project is archived".to_string());
    }
    if project.import_status != ProjectImportStatus::Ready {
        return Err(format!(
            "project source import is not ready: {}",
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
        return Err("project source workspace is not available".to_string());
    }
    Ok(())
}

fn validate_harness_branch_target(
    project: &ProjectRecord,
    run_id: Option<&str>,
    branch_ref: &str,
    code_write: bool,
    manual_project_access: bool,
) -> Result<(), String> {
    let default_branch = project
        .harness_default_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    validate_harness_branch_ref(
        default_branch,
        run_id,
        branch_ref,
        code_write,
        manual_project_access,
    )
}

fn resolve_harness_branch_ref(
    default_branch: &str,
    requested_branch_ref: Option<&str>,
    manual_project_access: bool,
) -> Result<String, String> {
    if let Some(branch_ref) = requested_branch_ref
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(branch_ref.to_string());
    }
    if manual_project_access {
        return Ok(default_branch.to_string());
    }
    Err("Harness MCP requires a frozen branch target".to_string())
}

fn validate_harness_branch_ref(
    default_branch: &str,
    run_id: Option<&str>,
    branch_ref: &str,
    code_write: bool,
    manual_project_access: bool,
) -> Result<(), String> {
    if branch_ref == default_branch {
        return if code_write && !manual_project_access {
            Err("Harness write capability requires a Task Run branch".to_string())
        } else {
            Ok(())
        };
    }
    let run_id = run_id
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
        .ok_or_else(|| "Harness run branch requires a valid Task Run id".to_string())?;
    let expected = format!("chatos/runs/{run_id}");
    if branch_ref != expected {
        return Err("Harness branch target does not belong to this Task Run".to_string());
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
    Err("project source access owner does not match project owner".to_string())
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
            .user_service_internal_base_url
            .trim()
            .trim_end_matches('/'),
        urlencoding::encode(owner_user_id.trim())
    );
    let response = crate::user_model_runtime_client::signed_user_service_request(
        state
            .config
            .user_service_internal_http_client
            .request(Method::GET, endpoint),
        secret,
        crate::user_model_runtime_client::HARNESS_ACCESS_READ_SCOPE,
    )?
    .with_internal_trace_context()
    .send()
    .await
    .map_err(|err| format!("project source access request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!(
            "project source access request failed: {status} {text}"
        ));
    }
    read_response_json_limited::<HarnessApiAccessResponse>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("parse project source access response failed: {err}"))
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
    let invocation = match name {
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
        "search_text" => {
            ensure_read_allowed(ctx)?;
            tool_search_text(ctx, &arguments).await
        }
        "open_edit_session" => {
            ensure_write_allowed(ctx)?;
            tool_open_edit_session(ctx, &arguments).await
        }
        "stage_edit_batch" => {
            ensure_write_allowed(ctx)?;
            tool_stage_edit_batch(ctx, &arguments).await
        }
        "commit_edit_session" => {
            ensure_write_allowed(ctx)?;
            tool_commit_edit_session(ctx, &arguments).await
        }
        "abort_edit_session" => {
            ensure_write_allowed(ctx)?;
            tool_abort_edit_session(ctx, &arguments).await
        }
        other => Err(format!("Tool not found: {other}")),
    };
    if matches!(
        name,
        "open_edit_session" | "stage_edit_batch" | "commit_edit_session" | "abort_edit_session"
    ) {
        record_file_modification_outcome(ctx, name, &invocation);
    }
    invocation
}

fn record_file_modification_outcome(
    ctx: &HarnessMcpContext,
    tool: &str,
    invocation: &Result<Value, String>,
) {
    let (outcome, success, changed, changed_target_count) = match invocation {
        Ok(value) => {
            let payload = value.get("_structured_result").unwrap_or(value);
            let changed = payload
                .get("changed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let outcome = payload
                .get("outcome")
                .and_then(Value::as_str)
                .unwrap_or_else(|| FileModificationOutcome::from_changed(changed).as_str());
            let changed_target_count = payload
                .get("changed_target_count")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(changed));
            (outcome, true, changed, changed_target_count)
        }
        Err(error) => {
            let outcome = classify_file_modification_error(error);
            (outcome.as_str(), outcome.is_success(), false, 0)
        }
    };
    tracing::info!(
        event = "file_modification_outcome",
        source = "project_harness",
        tool,
        project_id = ctx.project_id,
        run_id = ctx.run_id.as_deref().unwrap_or(""),
        outcome,
        success,
        changed,
        changed_target_count,
        "file modification completed"
    );
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
        Err("CodeMaintainer read capability is not enabled for this task".to_string())
    }
}

fn ensure_write_allowed(ctx: &HarnessMcpContext) -> Result<(), String> {
    if ctx.enabled_tools.code_write {
        Ok(())
    } else {
        Err("CodeMaintainer write capability is not enabled for this task".to_string())
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

fn tool_structured_result(payload: Value, message: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": message
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

    #[test]
    fn only_workspace_mutations_are_selected_for_internal_audit() {
        let request = |name: &str| JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "tools/call".to_string(),
            params: Some(json!({ "name": name, "arguments": { "content": "secret" } })),
        };

        assert_eq!(
            requested_harness_write_tool(&request("stage_edit_batch")),
            Some("stage_edit_batch")
        );
        assert_eq!(
            requested_harness_write_tool(&request("read_file_raw")),
            None
        );
        assert_eq!(requested_harness_write_tool(&request("unknown")), None);
    }

    #[test]
    fn task_runtime_write_requires_run_branch_but_manual_project_edits_use_default_branch() {
        assert!(validate_harness_branch_ref("main", Some("run-1"), "main", false, false).is_ok());
        assert!(validate_harness_branch_ref("main", Some("run-1"), "main", true, false).is_err());
        assert!(validate_harness_branch_ref("main", None, "main", true, true).is_ok());
        assert!(validate_harness_branch_ref(
            "main",
            Some("run-1"),
            "chatos/runs/run-1",
            true,
            false,
        )
        .is_ok());
        assert!(validate_harness_branch_ref(
            "main",
            Some("run-1"),
            "chatos/runs/run-2",
            false,
            false,
        )
        .is_err());
    }

    #[test]
    fn only_manual_project_access_can_derive_the_default_branch() {
        assert_eq!(
            resolve_harness_branch_ref("main", None, true).unwrap(),
            "main"
        );
        assert!(resolve_harness_branch_ref("main", None, false).is_err());
        assert_eq!(
            resolve_harness_branch_ref("main", Some("chatos/runs/run-1"), false).unwrap(),
            "chatos/runs/run-1"
        );
    }
}
