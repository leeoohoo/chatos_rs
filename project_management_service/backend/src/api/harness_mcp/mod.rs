// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chatos_builtin_tools::code_maintainer::{apply_patch_limited, ApplyPatchResult};
use chatos_mcp_service::{HostCapabilityPolicy, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER};
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};

use super::sync::require_project_sync_secret;
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

use self::client::{
    commit_file_actions, ensure_action_count, fetch_harness_content, read_harness_file,
    HarnessApiAccessResponse, HarnessCommitAction, HarnessFile,
};
use self::patch_targets::{collect_patch_targets, patch_error_with_recovery, PatchTarget};
use self::path_policy::optional_repo_path;
use self::tool_definitions::tool_definitions;
use self::tools::{
    tool_append_file, tool_delete_path, tool_edit_file, tool_list_dir, tool_read_file_range,
    tool_read_file_raw, tool_search_text, tool_write_file,
};

const SERVER_NAME: &str = "harness_code";
const PROTOCOL_VERSION: &str = "2024-11-05";
const TASK_RUNNER_PROJECT_ID_HEADER: &str = "x-task-runner-project-id";
const USER_SERVICE_INTERNAL_SECRET_HEADER: &str = "x-user-service-internal-secret";
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
