// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use base64::Engine as _;
use chatos_builtin_tools::code_maintainer::{apply_patch_limited, ApplyPatchResult};
use chatos_mcp_service::{HostCapabilityPolicy, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};

use super::sync::require_project_sync_secret;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse};
use crate::models::{ProjectImportStatus, ProjectRecord, ProjectStatus};
use crate::state::AppState;

mod tool_definitions;

use self::tool_definitions::tool_definitions;

const SERVER_NAME: &str = "harness_code";
const PROTOCOL_VERSION: &str = "2024-11-05";
const TASK_RUNNER_PROJECT_ID_HEADER: &str = "x-task-runner-project-id";
const USER_SERVICE_INTERNAL_SECRET_HEADER: &str = "x-user-service-internal-secret";
const DEFAULT_MAX_FILE_BYTES: i64 = 256 * 1024;
const DEFAULT_MAX_WRITE_BYTES: i64 = 5 * 1024 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 40;
const MAX_SEARCH_FILES: usize = 2_000;
const MAX_SEARCH_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMMIT_ACTIONS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
struct HarnessApiAccessResponse {
    base_url: String,
    access_token: String,
    #[serde(default)]
    space_identifier: String,
}

#[derive(Debug, Deserialize)]
struct HarnessContentResponse {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: Value,
}

#[derive(Debug, Deserialize)]
struct HarnessFileContent {
    #[serde(default)]
    encoding: String,
    #[serde(default)]
    data: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    data_size: i64,
}

#[derive(Debug, Deserialize)]
struct HarnessDirContent {
    #[serde(default)]
    entries: Vec<HarnessContentInfo>,
}

#[derive(Debug, Deserialize)]
struct HarnessContentInfo {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
struct HarnessListPathsResponse {
    #[serde(default)]
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HarnessCommitRequest {
    title: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    actions: Vec<HarnessCommitAction>,
}

#[derive(Debug, Serialize)]
struct HarnessCommitAction {
    action: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<String>,
}

#[derive(Debug, Clone)]
struct HarnessFile {
    path: String,
    size: i64,
    sha256: String,
    harness_blob_sha: String,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatchTarget {
    before_path: String,
    after_path: String,
}

#[derive(Debug)]
struct HarnessMcpContext {
    project_id: String,
    repo_path: String,
    access: HarnessApiAccessResponse,
    client: reqwest::Client,
    enabled_tools: HostCapabilityPolicy,
}

#[derive(Debug)]
struct HarnessRequestError {
    status: Option<StatusCode>,
    message: String,
}

impl HarnessRequestError {
    fn from_message(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    fn is_not_found(&self) -> bool {
        self.status == Some(StatusCode::NOT_FOUND)
            || self.message.to_ascii_lowercase().contains("not found")
    }
}

impl std::fmt::Display for HarnessRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(status) = self.status {
            write!(f, "{status} {}", self.message)
        } else {
            f.write_str(self.message.as_str())
        }
    }
}

pub(in crate::api) async fn harness_project_mcp_entrypoint(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(err) = require_project_sync_secret(&state, &headers) {
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
    let response = client
        .request(Method::GET, endpoint)
        .header(USER_SERVICE_INTERNAL_SECRET_HEADER, secret)
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

async fn tool_read_file_raw(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let with_line_numbers = args
        .get("with_line_numbers")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let file = read_harness_file(ctx, path.as_str()).await?;
    let mut payload = file_payload(&file, with_line_numbers);
    payload["harness"] = json!({
        "project_id": ctx.project_id,
        "repo_path": ctx.repo_path,
        "blob_sha": file.harness_blob_sha
    });
    Ok(tool_text_result(payload))
}

async fn tool_read_file_range(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let start_line = args
        .get("start_line")
        .and_then(Value::as_u64)
        .ok_or_else(|| "start_line is required".to_string())? as usize;
    let end_line = args
        .get("end_line")
        .and_then(Value::as_u64)
        .ok_or_else(|| "end_line is required".to_string())? as usize;
    let with_numbers = args
        .get("with_line_numbers")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file = read_harness_file(ctx, path.as_str()).await?;
    let lines = normalized_lines(file.content.as_str());
    let total_lines = lines.len();
    let start = start_line.max(1);
    let end = end_line.min(total_lines.max(1));
    let selected = if start <= end_line {
        lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| {
                let line_no = idx + 1;
                (line_no >= start && line_no <= end_line).then(|| {
                    if with_numbers {
                        format!("{line_no}: {line}")
                    } else {
                        line.clone()
                    }
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    Ok(tool_text_result(json!({
        "path": file.path,
        "size_bytes": file.size,
        "sha256": file.sha256,
        "harness_blob_sha": file.harness_blob_sha,
        "start_line": start,
        "end_line": end,
        "total_lines": total_lines,
        "content": selected.join("\n")
    })))
}

async fn tool_list_dir(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = optional_repo_path(args.get("path").and_then(Value::as_str), true)?;
    let max_entries = args
        .get("max_entries")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 1000) as usize)
        .unwrap_or(200);
    let content = fetch_harness_content(ctx, path.as_str())
        .await
        .map_err(|err| err.to_string())?;
    if content.kind != "dir" {
        return Err("Target is not a directory.".to_string());
    }
    let dir: HarnessDirContent = serde_json::from_value(content.content)
        .map_err(|err| format!("parse Harness directory content failed: {err}"))?;
    let entries = dir
        .entries
        .into_iter()
        .take(max_entries)
        .map(|entry| {
            json!({
                "name": if entry.name.is_empty() { path_name(entry.path.as_str()) } else { entry.name },
                "path": entry.path,
                "type": entry.kind,
                "size": 0,
                "mtime_ms": 0
            })
        })
        .collect::<Vec<_>>();
    Ok(tool_text_result(json!({ "entries": entries })))
}

async fn tool_search_text(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let pattern = args
        .get("pattern")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "pattern is required".to_string())?;
    let scope = optional_repo_path(args.get("path").and_then(Value::as_str), true)?;
    let limit = args
        .get("max_results")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 500) as usize)
        .unwrap_or(DEFAULT_SEARCH_LIMIT);
    let paths = list_harness_paths(ctx).await?;
    let mut results = Vec::new();
    let mut visited_files = 0usize;
    let mut visited_bytes = 0usize;
    for file_path in paths
        .files
        .into_iter()
        .filter(|path| path_matches_scope(path, scope.as_str()))
    {
        if results.len() >= limit {
            break;
        }
        visited_files += 1;
        if visited_files > MAX_SEARCH_FILES {
            break;
        }
        let file = match read_harness_file(ctx, file_path.as_str()).await {
            Ok(file) => file,
            Err(_) => continue,
        };
        visited_bytes = visited_bytes.saturating_add(file.content.len());
        if visited_bytes > MAX_SEARCH_TOTAL_BYTES {
            break;
        }
        for (idx, line) in normalized_lines(file.content.as_str())
            .into_iter()
            .enumerate()
        {
            if line.contains(pattern) {
                results.push(json!({
                    "path": file.path,
                    "line": idx + 1,
                    "text": truncate_search_text(line.as_str())
                }));
                if results.len() >= limit {
                    break;
                }
            }
        }
    }
    Ok(tool_text_result(json!({
        "count": results.len(),
        "results": results,
        "scanned_files": visited_files,
        "scanned_bytes": visited_bytes
    })))
}

async fn tool_write_file(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let content = required_string(args, "content")?;
    ensure_write_size(content)?;
    let (action, old_sha) = existing_file_sha_for_write(ctx, path.as_str()).await?;
    let commit = commit_single_file_action(
        ctx,
        action.as_str(),
        path.as_str(),
        Some(content),
        old_sha,
        format!("Chatos: write {path}").as_str(),
    )
    .await?;
    let payload = write_result_payload(ctx, path.as_str(), content, action.as_str(), commit);
    Ok(tool_text_result(payload))
}

async fn tool_append_file(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let append_content = required_string(args, "content")?;
    let existing = match read_harness_file(ctx, path.as_str()).await {
        Ok(file) => Some(file),
        Err(err) if err.contains("not found") || err.contains("404") => None,
        Err(err) => return Err(err),
    };
    let mut next = existing
        .as_ref()
        .map(|file| file.content.clone())
        .unwrap_or_default();
    next.push_str(append_content);
    ensure_write_size(next.as_str())?;
    let action = if existing.is_some() {
        "UPDATE"
    } else {
        "CREATE"
    };
    let old_sha = existing.map(|file| file.harness_blob_sha);
    let commit = commit_single_file_action(
        ctx,
        action,
        path.as_str(),
        Some(next.as_str()),
        old_sha,
        format!("Chatos: append {path}").as_str(),
    )
    .await?;
    let payload = write_result_payload(ctx, path.as_str(), next.as_str(), action, commit);
    Ok(tool_text_result(payload))
}

async fn tool_edit_file(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let old_text = required_string(args, "old_text")?;
    let new_text = required_string(args, "new_text")?;
    let file = read_harness_file(ctx, path.as_str()).await?;
    let edit = apply_text_edit(file.content.as_str(), args, old_text, new_text)?;
    ensure_write_size(edit.content.as_str())?;
    let commit = commit_single_file_action(
        ctx,
        "UPDATE",
        path.as_str(),
        Some(edit.content.as_str()),
        Some(file.harness_blob_sha),
        format!("Chatos: edit {path}").as_str(),
    )
    .await?;
    let mut payload =
        write_result_payload(ctx, path.as_str(), edit.content.as_str(), "UPDATE", commit);
    payload["match"] = edit.info;
    Ok(tool_text_result(payload))
}

async fn tool_delete_path(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let path = required_file_path(args)?;
    match fetch_harness_content(ctx, path.as_str()).await {
        Ok(content) if content.kind == "dir" => delete_harness_directory(ctx, path.as_str()).await,
        Ok(content) => {
            let action = HarnessCommitAction {
                action: "DELETE".to_string(),
                path: path.clone(),
                payload: None,
                encoding: None,
                sha: non_empty(content.sha),
            };
            let commit =
                commit_file_actions(ctx, format!("Chatos: delete {path}").as_str(), vec![action])
                    .await?;
            Ok(tool_text_result(json!({
                "result": {
                    "path": path,
                    "deleted": true,
                    "exists_after_delete": false,
                    "already_absent": false
                },
                "harness": {
                    "project_id": ctx.project_id,
                    "repo_path": ctx.repo_path,
                    "action": "DELETE",
                    "commit": commit
                }
            })))
        }
        Err(err) if err.is_not_found() => Ok(tool_text_result(json!({
            "result": {
                "path": path,
                "deleted": false,
                "exists_after_delete": false,
                "already_absent": true
            },
            "message": "Path already absent. No Harness commit was created.",
            "hint": "Verify the exact path with list_dir before retrying delete."
        }))),
        Err(err) => Err(err.to_string()),
    }
}

async fn delete_harness_directory(ctx: &HarnessMcpContext, path: &str) -> Result<Value, String> {
    let paths = list_harness_paths(ctx).await?;
    let files = paths
        .files
        .into_iter()
        .filter(|file_path| path_matches_scope(file_path, path))
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(tool_text_result(json!({
            "result": {
                "path": path,
                "deleted": false,
                "exists_after_delete": false,
                "already_absent": true
            },
            "message": "Directory has no tracked files. No Harness commit was created."
        })));
    }
    ensure_action_count(files.len())?;
    let actions = files
        .iter()
        .map(|file_path| HarnessCommitAction {
            action: "DELETE".to_string(),
            path: file_path.clone(),
            payload: None,
            encoding: None,
            sha: None,
        })
        .collect::<Vec<_>>();
    let commit = commit_file_actions(
        ctx,
        format!("Chatos: delete directory {path}").as_str(),
        actions,
    )
    .await?;
    Ok(tool_text_result(json!({
        "result": {
            "path": path,
            "deleted": true,
            "exists_after_delete": false,
            "already_absent": false,
            "deleted_files": files
        },
        "harness": {
            "project_id": ctx.project_id,
            "repo_path": ctx.repo_path,
            "action": "DELETE_DIRECTORY",
            "commit": commit
        }
    })))
}

async fn tool_apply_patch(ctx: &HarnessMcpContext, args: &Value) -> Result<Value, String> {
    let patch = required_string(args, "patch")?;
    if patch.trim().is_empty() {
        return Err("patch is required".to_string());
    }
    let targets = collect_patch_targets(patch)?;
    if targets.is_empty() {
        return Err("patch does not contain any file targets".to_string());
    }
    ensure_action_count(targets.len())?;
    let temp_root = create_temp_patch_dir(ctx.project_id.as_str())?;
    let result =
        apply_patch_from_harness(ctx, temp_root.as_path(), patch, targets.as_slice()).await;
    let _ = std::fs::remove_dir_all(temp_root.as_path());
    let (applied, actions) = result?;
    if actions.is_empty() {
        return Ok(tool_text_result(json!({
            "result": applied,
            "message": "Patch produced no Harness file changes. No commit was created."
        })));
    }
    ensure_action_count(actions.len())?;
    let changed_paths = actions
        .iter()
        .map(|action| action.path.clone())
        .collect::<Vec<_>>();
    let commit = commit_file_actions(ctx, "Chatos: apply patch", actions).await?;
    Ok(tool_text_result(json!({
        "result": applied,
        "harness": {
            "project_id": ctx.project_id,
            "repo_path": ctx.repo_path,
            "action": "APPLY_PATCH",
            "changed_paths": changed_paths,
            "commit": commit
        }
    })))
}

async fn existing_file_sha_for_write(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<(String, Option<String>), String> {
    match fetch_harness_content(ctx, path).await {
        Ok(content) if content.kind == "dir" => Err("Target is a directory.".to_string()),
        Ok(content) => Ok(("UPDATE".to_string(), non_empty(content.sha))),
        Err(err) if err.is_not_found() => Ok(("CREATE".to_string(), None)),
        Err(err) => Err(err.to_string()),
    }
}

async fn apply_patch_from_harness(
    ctx: &HarnessMcpContext,
    temp_root: &FsPath,
    patch: &str,
    targets: &[PatchTarget],
) -> Result<(ApplyPatchResult, Vec<HarnessCommitAction>), String> {
    let mut existing_by_path = BTreeMap::new();
    for path in unique_patch_read_paths(targets) {
        match read_harness_file(ctx, path.as_str()).await {
            Ok(file) => {
                write_temp_file(temp_root, path.as_str(), file.content.as_str())?;
                existing_by_path.insert(path, file);
            }
            Err(err) if err.contains("not found") || err.contains("404") => {}
            Err(err) => return Err(err),
        }
    }
    ensure_move_targets_do_not_exist(ctx, targets, &existing_by_path).await?;

    let applied = apply_patch_limited(temp_root, patch, true, DEFAULT_MAX_WRITE_BYTES)
        .map_err(|err| patch_error_with_recovery(err.as_str()))?;
    let actions = patch_commit_actions(temp_root, &applied, targets, &existing_by_path)?;
    Ok((applied, actions))
}

async fn ensure_move_targets_do_not_exist(
    ctx: &HarnessMcpContext,
    targets: &[PatchTarget],
    existing_by_path: &BTreeMap<String, HarnessFile>,
) -> Result<(), String> {
    for target in targets
        .iter()
        .filter(|target| target.before_path != target.after_path)
    {
        if existing_by_path.contains_key(target.after_path.as_str()) {
            return Err(format!(
                "Patch move target already exists in Harness repo: {}",
                target.after_path
            ));
        }
        match fetch_harness_content(ctx, target.after_path.as_str()).await {
            Ok(_) => {
                return Err(format!(
                    "Patch move target already exists in Harness repo: {}",
                    target.after_path
                ));
            }
            Err(err) if err.is_not_found() => {}
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(())
}

fn patch_commit_actions(
    temp_root: &FsPath,
    applied: &ApplyPatchResult,
    targets: &[PatchTarget],
    existing_by_path: &BTreeMap<String, HarnessFile>,
) -> Result<Vec<HarnessCommitAction>, String> {
    let moved_from = targets
        .iter()
        .filter(|target| target.before_path != target.after_path)
        .map(|target| target.before_path.clone())
        .collect::<BTreeSet<_>>();
    let mut actions_by_path = BTreeMap::new();

    for path in &applied.deleted {
        let path = optional_repo_path(Some(path.as_str()), false)?;
        if existing_by_path.contains_key(path.as_str()) {
            insert_patch_action(
                &mut actions_by_path,
                HarnessCommitAction {
                    action: "DELETE".to_string(),
                    path: path.clone(),
                    payload: None,
                    encoding: None,
                    sha: existing_by_path
                        .get(path.as_str())
                        .map(|file| file.harness_blob_sha.clone()),
                },
            )?;
        }
    }
    for path in moved_from {
        if existing_by_path.contains_key(path.as_str()) {
            insert_patch_action(
                &mut actions_by_path,
                HarnessCommitAction {
                    action: "DELETE".to_string(),
                    path: path.clone(),
                    payload: None,
                    encoding: None,
                    sha: existing_by_path
                        .get(path.as_str())
                        .map(|file| file.harness_blob_sha.clone()),
                },
            )?;
        }
    }
    for path in applied.updated.iter().chain(applied.added.iter()) {
        let path = optional_repo_path(Some(path.as_str()), false)?;
        let content = read_temp_file(temp_root, path.as_str())?;
        ensure_write_size(content.as_str())?;
        let existing = existing_by_path.get(path.as_str());
        insert_patch_action(
            &mut actions_by_path,
            HarnessCommitAction {
                action: if existing.is_some() {
                    "UPDATE".to_string()
                } else {
                    "CREATE".to_string()
                },
                path: path.clone(),
                payload: Some(content),
                encoding: Some("utf8".to_string()),
                sha: existing.map(|file| file.harness_blob_sha.clone()),
            },
        )?;
    }
    Ok(actions_by_path.into_values().collect())
}

fn insert_patch_action(
    actions_by_path: &mut BTreeMap<String, HarnessCommitAction>,
    action: HarnessCommitAction,
) -> Result<(), String> {
    if actions_by_path.contains_key(action.path.as_str()) {
        return Err(format!(
            "Patch produced multiple conflicting actions for {}",
            action.path
        ));
    }
    actions_by_path.insert(action.path.clone(), action);
    Ok(())
}

fn unique_patch_read_paths(targets: &[PatchTarget]) -> Vec<String> {
    targets
        .iter()
        .map(|target| target.before_path.clone())
        .chain(targets.iter().map(|target| target.after_path.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn create_temp_patch_dir(project_id: &str) -> Result<PathBuf, String> {
    let safe_project_id = project_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .take(64)
        .collect::<String>();
    let dir = std::env::temp_dir().join(format!(
        "chatos-harness-mcp-patch-{}-{}",
        safe_project_id,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(dir.as_path()).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn write_temp_file(root: &FsPath, rel_path: &str, content: &str) -> Result<(), String> {
    let path = temp_repo_path(root, rel_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, content).map_err(|err| err.to_string())
}

fn read_temp_file(root: &FsPath, rel_path: &str) -> Result<String, String> {
    let path = temp_repo_path(root, rel_path)?;
    let metadata = std::fs::metadata(path.as_path()).map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Err(format!("Patch output is not a file: {rel_path}"));
    }
    if metadata.len() as i64 > DEFAULT_MAX_WRITE_BYTES {
        return Err(format!(
            "Patch output file too large: {} bytes",
            metadata.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|err| err.to_string())
}

fn temp_repo_path(root: &FsPath, rel_path: &str) -> Result<PathBuf, String> {
    let path = optional_repo_path(Some(rel_path), false)?;
    let mut out = root.to_path_buf();
    for part in path.split('/') {
        out.push(part);
    }
    Ok(out)
}

async fn read_harness_file(ctx: &HarnessMcpContext, path: &str) -> Result<HarnessFile, String> {
    let content = fetch_harness_content(ctx, path)
        .await
        .map_err(|err| err.to_string())?;
    if content.kind != "file" {
        return Err("Target is not a file.".to_string());
    }
    let path = if content.path.trim().is_empty() {
        path.to_string()
    } else {
        content.path
    };
    let file_content: HarnessFileContent = serde_json::from_value(content.content)
        .map_err(|err| format!("parse Harness file content failed: {err}"))?;
    let bytes = decode_harness_file_content(&file_content)?;
    if file_content.size > DEFAULT_MAX_FILE_BYTES {
        return Err(format!("File too large ({} bytes).", file_content.size));
    }
    if bytes.len() as i64 > DEFAULT_MAX_FILE_BYTES {
        return Err(format!("File too large ({} bytes).", bytes.len()));
    }
    if file_content.data_size > 0 && file_content.size > file_content.data_size {
        return Err(format!(
            "File too large or truncated ({} bytes).",
            file_content.size
        ));
    }
    if bytes.iter().any(|byte| *byte == 0) {
        return Err("Binary file not supported.".to_string());
    }
    let text = String::from_utf8_lossy(bytes.as_slice()).to_string();
    Ok(HarnessFile {
        path,
        size: file_content.size.max(bytes.len() as i64),
        sha256: sha256_hex(bytes.as_slice()),
        harness_blob_sha: content.sha,
        content: text,
    })
}

async fn fetch_harness_content(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<HarnessContentResponse, HarnessRequestError> {
    let endpoint = harness_repo_url(
        ctx.access.base_url.as_str(),
        ctx.repo_path.as_str(),
        "content",
        Some(path),
    );
    harness_request_json::<HarnessContentResponse, ()>(
        &ctx.client,
        Method::GET,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        None,
    )
    .await
}

async fn list_harness_paths(ctx: &HarnessMcpContext) -> Result<HarnessListPathsResponse, String> {
    let endpoint = format!(
        "{}/api/v1/repos/{}/+/paths?include_directories=true",
        ctx.access.base_url.trim().trim_end_matches('/'),
        encode_path_segments(ctx.repo_path.as_str())
    );
    harness_request_json::<HarnessListPathsResponse, ()>(
        &ctx.client,
        Method::GET,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        None,
    )
    .await
    .map_err(|err| err.to_string())
}

async fn commit_single_file_action(
    ctx: &HarnessMcpContext,
    action: &str,
    path: &str,
    payload: Option<&str>,
    sha: Option<String>,
    title: &str,
) -> Result<Value, String> {
    let action_payload = HarnessCommitAction {
        action: action.to_string(),
        path: path.to_string(),
        payload: payload.map(ToOwned::to_owned),
        encoding: payload.map(|_| "utf8".to_string()),
        sha,
    };
    commit_file_actions(ctx, title, vec![action_payload]).await
}

async fn commit_file_actions(
    ctx: &HarnessMcpContext,
    title: &str,
    actions: Vec<HarnessCommitAction>,
) -> Result<Value, String> {
    ensure_action_count(actions.len())?;
    let body = HarnessCommitRequest {
        title: title.to_string(),
        message: format!(
            "Applied by Chatos Project Service for project {}",
            ctx.project_id
        ),
        branch: None,
        actions,
    };
    let endpoint = format!(
        "{}/api/v1/repos/{}/+/commits/",
        ctx.access.base_url.trim().trim_end_matches('/'),
        encode_path_segments(ctx.repo_path.as_str())
    );
    harness_request_json::<Value, _>(
        &ctx.client,
        Method::POST,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        Some(&body),
    )
    .await
    .map_err(|err| err.to_string())
}

fn ensure_action_count(count: usize) -> Result<(), String> {
    if count > MAX_COMMIT_ACTIONS {
        Err(format!(
            "Harness commit action count exceeds limit: {count} > {MAX_COMMIT_ACTIONS}"
        ))
    } else {
        Ok(())
    }
}

async fn harness_request_json<TResp, TBody>(
    client: &reqwest::Client,
    method: Method,
    endpoint: &str,
    bearer_token: &str,
    body: Option<&TBody>,
) -> Result<TResp, HarnessRequestError>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let mut request = client
        .request(method, endpoint)
        .bearer_auth(bearer_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| HarnessRequestError::from_message(err.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(HarnessRequestError {
            status: Some(status),
            message: text,
        });
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| HarnessRequestError::from_message(err.to_string()))
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

fn required_file_path(args: &Value) -> Result<String, String> {
    let value = args.get("path").and_then(Value::as_str);
    optional_repo_path(value, false)
}

fn optional_repo_path(value: Option<&str>, allow_root: bool) -> Result<String, String> {
    let raw = value.unwrap_or(".");
    let trimmed = raw.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err("path must be relative to the Harness repo root".to_string());
    }
    if trimmed.contains('\0') {
        return Err("path contains a null byte".to_string());
    }
    let normalized = trimmed.replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err("path must not contain ..".to_string());
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        if allow_root {
            Ok(String::new())
        } else {
            Err("path is required".to_string())
        }
    } else {
        Ok(parts.join("/"))
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

fn collect_patch_targets(patch: &str) -> Result<Vec<PatchTarget>, String> {
    let text = patch.replace("\r\n", "\n");
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut targets = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let before_path = optional_repo_path(Some(path), false)?;
            let mut after_path = before_path.clone();
            i += 1;
            while i < lines.len() {
                let current = lines[i];
                if is_patch_boundary(current) {
                    break;
                }
                if let Some(dest) = current.strip_prefix("*** Move to: ") {
                    after_path = optional_repo_path(Some(dest), false)?;
                }
                i += 1;
            }
            targets.push(PatchTarget {
                before_path,
                after_path,
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let path = optional_repo_path(Some(path), false)?;
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            i += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let path = optional_repo_path(Some(path), false)?;
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            i += 1;
            continue;
        }
        if let Some(path) = parse_loose_update_header(line) {
            let path = optional_repo_path(Some(path.as_str()), false)?;
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            i += 1;
            continue;
        }
        i += 1;
    }
    targets.sort_by(|left, right| {
        (&left.before_path, &left.after_path).cmp(&(&right.before_path, &right.after_path))
    });
    targets.dedup();
    Ok(targets)
}

fn parse_loose_update_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    for prefix in ["Update File --- ", "Update File: "] {
        let Some(path) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let path = path.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    None
}

fn is_patch_boundary(line: &str) -> bool {
    line.starts_with("*** Update File: ")
        || line.starts_with("*** Add File: ")
        || line.starts_with("*** Delete File: ")
        || line.starts_with("*** End Patch")
}

fn patch_error_with_recovery(error: &str) -> String {
    if error.contains("Patch context not found in file.")
        || error.contains("old_text not found in file.")
    {
        let hint = json!({
            "error": error,
            "recovery": {
                "recommended_next_tools": [
                    "read_file_raw",
                    "read_file_range"
                ],
                "guidance": "Patch context is stale. Re-read target files from Harness and regenerate the patch with exact current lines."
            }
        });
        serde_json::to_string(&hint).unwrap_or_else(|_| error.to_string())
    } else {
        error.to_string()
    }
}

fn harness_repo_url(
    base_url: &str,
    repo_path: &str,
    operation: &str,
    path: Option<&str>,
) -> String {
    let mut url = format!(
        "{}/api/v1/repos/{}/+/{operation}",
        base_url.trim().trim_end_matches('/'),
        encode_path_segments(repo_path)
    );
    if let Some(path) = path {
        url.push('/');
        if !path.is_empty() {
            url.push_str(encode_path_segments(path).as_str());
        }
    }
    url
}

fn encode_path_segments(path: &str) -> String {
    path.trim()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_harness_file_content(content: &HarnessFileContent) -> Result<Vec<u8>, String> {
    if content.encoding.eq_ignore_ascii_case("base64") {
        return base64::engine::general_purpose::STANDARD
            .decode(content.data.as_bytes())
            .map_err(|err| format!("decode Harness base64 content failed: {err}"));
    }
    Ok(content.data.as_bytes().to_vec())
}

fn normalized_lines(content: &str) -> Vec<String> {
    content
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn file_payload(file: &HarnessFile, with_line_numbers: bool) -> Value {
    let lines = normalized_lines(file.content.as_str());
    let mut payload = json!({
        "path": file.path,
        "size_bytes": file.size,
        "sha256": file.sha256,
        "harness_blob_sha": file.harness_blob_sha,
        "line_count": lines.len(),
        "ends_with_newline": file.content.ends_with('\n'),
        "content": file.content
    });
    if with_line_numbers {
        payload["numbered_lines"] = Value::Array(
            lines
                .iter()
                .enumerate()
                .map(|(idx, text)| {
                    json!({
                        "line": idx + 1,
                        "text": text
                    })
                })
                .collect(),
        );
    }
    payload
}

fn write_result_payload(
    ctx: &HarnessMcpContext,
    path: &str,
    content: &str,
    action: &str,
    commit: Value,
) -> Value {
    let changed_blob_sha = changed_file_blob_sha(&commit, path);
    json!({
        "result": {
            "bytes": content.len() as i64,
            "sha256": sha256_hex(content.as_bytes()),
            "path": path
        },
        "harness": {
            "project_id": ctx.project_id,
            "repo_path": ctx.repo_path,
            "action": action,
            "branch": "default",
            "changed_blob_sha": changed_blob_sha,
            "commit": commit
        }
    })
}

fn changed_file_blob_sha(commit: &Value, path: &str) -> Option<String> {
    commit
        .get("changed_files")
        .and_then(Value::as_array)?
        .iter()
        .find(|item| item.get("path").and_then(Value::as_str) == Some(path))
        .and_then(|item| item.get("blob_sha").and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn path_name(path: &str) -> String {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn path_matches_scope(path: &str, scope: &str) -> bool {
    if scope.is_empty() {
        return true;
    }
    path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn truncate_search_text(value: &str) -> String {
    const LIMIT: usize = 500;
    if value.chars().count() <= LIMIT {
        return value.to_string();
    }
    let mut text = value.chars().take(LIMIT).collect::<String>();
    text.push_str("...");
    text
}

#[derive(Debug)]
struct TextEditResult {
    content: String,
    info: Value,
}

fn apply_text_edit(
    content: &str,
    args: &Value,
    old_text: &str,
    new_text: &str,
) -> Result<TextEditResult, String> {
    let start_line = args
        .get("start_line")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let end_line = args
        .get("end_line")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let before_context = args.get("before_context").and_then(Value::as_str);
    let after_context = args.get("after_context").and_then(Value::as_str);
    let expected_matches = args
        .get("expected_matches")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let mut matches = Vec::new();
    for (start, _) in content.match_indices(old_text) {
        let end = start + old_text.len();
        if let Some(min_line) = start_line {
            if byte_line_number(content, start) < min_line {
                continue;
            }
        }
        if let Some(max_line) = end_line {
            if byte_line_number(content, end) > max_line {
                continue;
            }
        }
        if let Some(before) = before_context {
            if !content[..start].ends_with(before) {
                continue;
            }
        }
        if let Some(after) = after_context {
            if !content[end..].starts_with(after) {
                continue;
            }
        }
        matches.push((start, end));
    }
    if let Some(expected) = expected_matches {
        if matches.len() != expected {
            return Err(format!(
                "expected_matches mismatch: expected {expected}, found {}",
                matches.len()
            ));
        }
    }
    if matches.is_empty() {
        return Err("old_text not found in file.".to_string());
    }
    if matches.len() > 1 {
        return Err(format!(
            "old_text matched {} locations; provide before_context/after_context or start_line/end_line",
            matches.len()
        ));
    }
    let (start, end) = matches[0];
    let mut next = String::with_capacity(content.len() - old_text.len() + new_text.len());
    next.push_str(&content[..start]);
    next.push_str(new_text);
    next.push_str(&content[end..]);
    Ok(TextEditResult {
        content: next,
        info: json!({
            "replacements": 1,
            "start_line": byte_line_number(content, start),
            "end_line": byte_line_number(content, end),
            "old_text_bytes": old_text.len(),
            "new_text_bytes": new_text.len()
        }),
    })
}

fn byte_line_number(content: &str, byte_idx: usize) -> usize {
    content[..byte_idx.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
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

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
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
    fn repo_paths_reject_parent_traversal() {
        assert!(optional_repo_path(Some("../secret"), false).is_err());
        assert!(optional_repo_path(Some("/secret"), false).is_err());
        assert_eq!(
            optional_repo_path(Some("src/./main.rs"), false).unwrap(),
            "src/main.rs"
        );
    }

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
    fn edit_requires_unique_match() {
        let args = json!({
            "old_text": "hello",
            "new_text": "hi"
        });
        let err = apply_text_edit("hello\nhello\n", &args, "hello", "hi").unwrap_err();
        assert!(err.contains("matched 2 locations"));
    }

    #[test]
    fn patch_targets_include_multiple_sections_and_moves() {
        let patch = r#"*** Begin Patch
*** Update File: src/a.rs
@@
-old
+new
*** Update File: src/b.rs
*** Move to: src/c.rs
@@
-b
+c
*** Add File: src/d.rs
+hello
*** Delete File: src/e.rs
*** End Patch
"#;

        let targets = collect_patch_targets(patch).expect("targets");

        assert_eq!(
            targets,
            vec![
                PatchTarget {
                    before_path: "src/a.rs".to_string(),
                    after_path: "src/a.rs".to_string(),
                },
                PatchTarget {
                    before_path: "src/b.rs".to_string(),
                    after_path: "src/c.rs".to_string(),
                },
                PatchTarget {
                    before_path: "src/d.rs".to_string(),
                    after_path: "src/d.rs".to_string(),
                },
                PatchTarget {
                    before_path: "src/e.rs".to_string(),
                    after_path: "src/e.rs".to_string(),
                },
            ]
        );
    }

    #[test]
    fn harness_commit_request_omits_branch_to_use_repo_default() {
        let body = HarnessCommitRequest {
            title: "test".to_string(),
            message: "message".to_string(),
            branch: None,
            actions: vec![HarnessCommitAction {
                action: "CREATE".to_string(),
                path: "README.md".to_string(),
                payload: Some("hello".to_string()),
                encoding: Some("utf8".to_string()),
                sha: None,
            }],
        };

        let value = serde_json::to_value(body).expect("serialize body");

        assert!(value.get("branch").is_none());
    }
}
