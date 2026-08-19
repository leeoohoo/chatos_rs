// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_mcp_service::{
    BUILTIN_KIND_CODE_MAINTAINER_READ, BUILTIN_KIND_TERMINAL_CONTROLLER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use tracing::warn;

use crate::api::projects::memory_sync::{sync_active_project, sync_archived_project};
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::core::validation::normalize_non_empty;
use crate::models::project::{Project, ProjectService};
use crate::models::remote_connection::RemoteConnection;
use crate::services::realtime::publish_projects_updated;
use crate::services::user_settings::{get_effective_user_settings, local_project_creation_enabled};
use crate::services::{access_token_scope, project_management_api_client};

mod connector_client;
mod directory_payload;
mod project_reconciliation;
mod root_path;
mod terminal_relay;
mod types;

use connector_client::{
    connector_delete_json, connector_get_json, connector_post_json,
    connector_post_json_with_headers, connector_post_json_with_timeout,
    local_connector_mcp_relay_path,
};
pub(crate) use connector_client::{local_connector_tls_connector, local_connector_websocket_url};
use directory_payload::local_connector_directory_list_payload;
pub(crate) use project_reconciliation::reconcile_local_connector_project;
pub(crate) use root_path::{
    local_connector_display_path, local_connector_root_path, parse_local_connector_root_path,
    LocalConnectorRootRef,
};
use root_path::{
    local_relative_basename, sanitize_optional_local_relative_path,
    sanitize_required_local_relative_path,
};
pub(crate) use terminal_relay::{
    close_local_terminal_session, create_local_terminal_session, send_local_terminal_input,
};
use types::{
    CreateLocalConnectorProjectRequest, CreateLocalDirectoryRequest, CreateProjectBindingRequest,
    DeviceQuery, LocalConnectorDevice, LocalConnectorDirectoryCreateResponse,
    LocalConnectorProjectBinding, LocalConnectorWorkspace, LocalFsQuery, McpToolCallParams,
    McpToolCallRequest, RelayWorkspaceDirectoryCreateRequest, WorkspaceQuery,
};
const LOCAL_CONNECTOR_BINDING_MODE_MCP: &str = "local_mcp";
const LOCAL_CONNECTOR_BINDING_MODE_TERMINAL: &str = "local_terminal";
const LOCAL_CONNECTOR_DEVICE_ONLINE: &str = "online";
const LOCAL_CONNECTOR_WORKSPACE_ACTIVE: &str = "active";
const LOCAL_HARNESS_IMPORT_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
pub(crate) const LOCAL_CONNECTOR_BUILTIN_CODE_READ: &str = BUILTIN_KIND_CODE_MAINTAINER_READ;
pub(crate) const LOCAL_CONNECTOR_BUILTIN_TERMINAL: &str = BUILTIN_KIND_TERMINAL_CONTROLLER;
pub fn router() -> Router {
    Router::new()
        .route("/api/local-connectors/devices", get(list_devices))
        .route("/api/local-connectors/workspaces", get(list_workspaces))
        .route("/api/local-connectors/fs/list", get(list_directory))
        .route("/api/local-connectors/fs/mkdir", post(create_directory))
        .route("/api/local-connectors/projects", post(create_project))
        .route(
            "/api/local-connectors/terminal/exec",
            post(terminal_relay::exec_terminal_command),
        )
}

pub(crate) async fn test_remote_connection_via_connector(
    connection: &RemoteConnection,
    verification_code: Option<&str>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/remote-connections/test",
        urlencoding::encode(connection.local_connector_device_id.as_str())
    );
    connector_post_json(
        path.as_str(),
        &json!({
            "workspace_id": connection.local_connector_workspace_id,
            "connection": remote_connection_execution_payload(connection),
            "verification_code": verification_code,
        }),
    )
    .await
}

pub(crate) async fn run_remote_command_via_connector(
    connection: &RemoteConnection,
    command: &str,
    timeout: Duration,
    verification_code: Option<&str>,
) -> Result<String, String> {
    let path = format!(
        "/api/local-connectors/relay/{}/remote-connections/command",
        urlencoding::encode(connection.local_connector_device_id.as_str())
    );
    let timeout_ms = timeout.as_millis().clamp(1_000, 600_000) as u64;
    let response = connector_post_json_with_timeout::<Value, _>(
        path.as_str(),
        &json!({
            "workspace_id": connection.local_connector_workspace_id,
            "connection": remote_connection_execution_payload(connection),
            "command": command,
            "timeout_ms": timeout_ms,
            "verification_code": verification_code,
        }),
        timeout.saturating_add(Duration::from_secs(10)),
    )
    .await
    .map_err(connector_remote_execution_error)?;
    response
        .get("output")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Local Connector 远程命令响应缺少 output".to_string())
}

pub(crate) async fn close_remote_terminal_via_connector(
    connection: &RemoteConnection,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/remote-connections/terminal/close",
        urlencoding::encode(connection.local_connector_device_id.as_str())
    );
    connector_post_json(
        path.as_str(),
        &json!({
            "workspace_id": connection.local_connector_workspace_id,
            "terminal_session_id": connection.id,
        }),
    )
    .await
}

pub(crate) async fn remote_sftp_via_connector(
    connection: &RemoteConnection,
    operation: &str,
    payload: Value,
    verification_code: Option<&str>,
    timeout: Duration,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/remote-connections/sftp",
        urlencoding::encode(connection.local_connector_device_id.as_str())
    );
    let mut body = match payload {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    body.insert(
        "workspace_id".to_string(),
        Value::String(connection.local_connector_workspace_id.clone()),
    );
    body.insert(
        "connection_id".to_string(),
        Value::String(connection.id.clone()),
    );
    body.insert(
        "operation".to_string(),
        Value::String(operation.to_string()),
    );
    body.insert(
        "connection".to_string(),
        remote_connection_execution_payload(connection),
    );
    if let Some(code) = verification_code {
        body.insert(
            "verification_code".to_string(),
            Value::String(code.to_string()),
        );
    }
    connector_post_json_with_timeout(
        path.as_str(),
        &Value::Object(body),
        timeout.max(Duration::from_secs(20)),
    )
    .await
}

pub(crate) fn remote_connection_execution_payload(connection: &RemoteConnection) -> Value {
    json!({
        "host": connection.host,
        "port": connection.port,
        "username": connection.username,
        "auth_type": connection.auth_type,
        "password": connection.password,
        "private_key_path": connection.private_key_path,
        "certificate_path": connection.certificate_path,
        "host_key_policy": connection.host_key_policy,
        "jump_enabled": connection.jump_enabled,
        "jump_host": connection.jump_host,
        "jump_port": connection.jump_port,
        "jump_username": connection.jump_username,
        "jump_private_key_path": connection.jump_private_key_path,
        "jump_certificate_path": connection.jump_certificate_path,
        "jump_password": connection.jump_password,
    })
}

pub(crate) fn connector_remote_execution_error(error: (StatusCode, Json<Value>)) -> String {
    let (_, Json(value)) = error;
    if value.get("code").and_then(Value::as_str) == Some("second_factor_required") {
        let prompt = value
            .get("challenge_prompt")
            .and_then(Value::as_str)
            .unwrap_or("请输入验证码 / OTP");
        return format!("__CHATOS_SECOND_FACTOR_REQUIRED__:{prompt}");
    }
    value
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| value.get("detail").and_then(Value::as_str))
        .unwrap_or("Local Connector 远程执行失败")
        .to_string()
}

async fn list_devices(
    auth: AuthUser,
    Query(query): Query<DeviceQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = resolve_user_id(query.user_id, &auth) {
        return err;
    }
    match connector_get_json::<Vec<LocalConnectorDevice>>("/api/local-connectors/devices", &[])
        .await
    {
        Ok(devices) => (StatusCode::OK, Json(json!(devices))),
        Err(err) => err,
    }
}

async fn list_workspaces(
    auth: AuthUser,
    Query(query): Query<WorkspaceQuery>,
) -> (StatusCode, Json<Value>) {
    let _ = auth;
    let devices =
        match connector_get_json::<Vec<LocalConnectorDevice>>("/api/local-connectors/devices", &[])
            .await
        {
            Ok(devices) => devices,
            Err(err) => return err,
        };
    let online_device_ids = devices
        .iter()
        .filter(|device| device.status == LOCAL_CONNECTOR_DEVICE_ONLINE)
        .map(|device| device.id.clone())
        .collect::<HashSet<_>>();
    if let Some(device_id) = query.device_id.as_deref() {
        if !devices.iter().any(|device| device.id == device_id) {
            return error(
                StatusCode::NOT_FOUND,
                "Local Connector device 不存在或不属于当前用户",
            );
        }
        if !online_device_ids.contains(device_id) {
            return (StatusCode::OK, Json(json!([])));
        }
    }
    let query_params = query
        .device_id
        .as_deref()
        .map(|device_id| vec![("device_id", device_id.to_string())])
        .unwrap_or_default();
    match connector_get_json::<Vec<LocalConnectorWorkspace>>(
        "/api/local-connectors/workspaces",
        query_params.as_slice(),
    )
    .await
    {
        Ok(workspaces) => {
            let visible = workspaces
                .into_iter()
                .filter(|workspace| {
                    online_device_ids.contains(workspace.device_id.as_str())
                        && workspace.status == LOCAL_CONNECTOR_WORKSPACE_ACTIVE
                })
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(json!(visible)))
        }
        Err(err) => err,
    }
}

async fn list_directory(
    auth: AuthUser,
    Query(query): Query<LocalFsQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = resolve_user_id(query.user_id, &auth) {
        return err;
    }
    let device_id = match required_text(query.device_id, "device_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    let workspace_id = match required_text(query.workspace_id, "workspace_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    if let Err(err) = load_owned_workspace(device_id.as_str(), workspace_id.as_str()).await {
        return err;
    }
    let path = match sanitize_optional_local_relative_path(query.path.as_deref()) {
        Ok(Some(path)) => path,
        Ok(None) => ".".to_string(),
        Err(err) => return err,
    };
    match list_local_connector_directory(device_id.as_str(), workspace_id.as_str(), path.as_str())
        .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(local_connector_directory_list_payload(path.as_str(), value)),
        ),
        Err(err) => err,
    }
}

async fn create_directory(
    auth: AuthUser,
    Json(req): Json<CreateLocalDirectoryRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = resolve_user_id(req.user_id, &auth) {
        return err;
    }
    let device_id = match required_text(req.device_id, "device_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    let workspace_id = match required_text(req.workspace_id, "workspace_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    if let Err(err) = load_owned_online_workspace(device_id.as_str(), workspace_id.as_str()).await {
        return err;
    }
    let path = match sanitize_required_local_relative_path(req.path.as_deref(), "path") {
        Ok(value) => value,
        Err(err) => return err,
    };
    match create_local_connector_directory(device_id.as_str(), workspace_id.as_str(), path.as_str())
        .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(json!({
                "path": value.path,
                "created": value.created,
            })),
        ),
        Err(err) => err,
    }
}

async fn create_project(
    auth: AuthUser,
    Json(req): Json<CreateLocalConnectorProjectRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match get_effective_user_settings(Some(user_id.clone())).await {
        Ok(settings) if local_project_creation_enabled(&settings) => {}
        Ok(_) => {
            return error(StatusCode::FORBIDDEN, "当前配置未开启本地项目创建");
        }
        Err(err) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("读取本地项目创建配置失败: {err}"),
            );
        }
    }
    let device_id = match required_text(req.device_id, "device_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    let workspace_id = match required_text(req.workspace_id, "workspace_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    let (device, workspace) =
        match load_owned_online_workspace(device_id.as_str(), workspace_id.as_str()).await {
            Ok(value) => value,
            Err(err) => return err,
        };
    let relative_path = match sanitize_optional_local_relative_path(req.relative_path.as_deref()) {
        Ok(value) => value,
        Err(err) => return err,
    };
    if let Some(path) = relative_path.as_deref() {
        if let Err(err) =
            validate_local_connector_directory(device_id.as_str(), workspace_id.as_str(), path)
                .await
        {
            return err;
        }
    }

    let name = normalize_non_empty(req.name)
        .or_else(|| relative_path.as_deref().and_then(local_relative_basename))
        .or_else(|| normalize_non_empty(Some(workspace.display_name.clone())))
        .or_else(|| normalize_non_empty(Some(workspace.local_path_alias.clone())))
        .unwrap_or_else(|| "Local Project".to_string());
    let root_path = local_connector_root_path(
        device_id.as_str(),
        workspace_id.as_str(),
        relative_path.as_deref(),
    );
    let project = Project::new(
        name,
        root_path,
        normalize_non_empty(req.git_url),
        normalize_non_empty(req.description),
        Some(user_id.clone()),
    );
    let saved_id = match ProjectService::create(project.clone()).await {
        Ok(id) => id,
        Err(err) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("创建项目失败: {err}"),
            );
        }
    };
    let saved = ProjectService::get_by_id(saved_id.as_str())
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| Project {
            id: saved_id.clone(),
            ..project
        });
    if let Err(err) = import_local_project_to_harness(
        saved.id.as_str(),
        device_id.as_str(),
        workspace_id.as_str(),
        relative_path.as_deref(),
    )
    .await
    {
        let failure = error(
            StatusCode::BAD_GATEWAY,
            json!({
                "error": "导入本地项目到 Harness 失败",
                "detail": err,
            }),
        );
        return project_create_error_with_rollback(saved.clone(), &[], failure, true).await;
    }

    let mut bindings = Vec::new();
    for mode in [
        LOCAL_CONNECTOR_BINDING_MODE_MCP,
        LOCAL_CONNECTOR_BINDING_MODE_TERMINAL,
    ] {
        match create_project_binding(
            saved.id.as_str(),
            device_id.as_str(),
            workspace_id.as_str(),
            mode,
        )
        .await
        {
            Ok(binding) => bindings.push(binding),
            Err(err) => {
                return project_create_error_with_rollback(
                    saved.clone(),
                    bindings.as_slice(),
                    err,
                    false,
                )
                .await;
            }
        }
    }

    if let Err(err) = sync_active_project(&saved).await {
        warn!(
            project_id = saved.id.as_str(),
            error = err.as_str(),
            "sync memory project failed after local connector project create"
        );
        let failure = error(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({
                "error": "sync memory project failed",
                "detail": err,
            }),
        );
        return project_create_error_with_rollback(
            saved.clone(),
            bindings.as_slice(),
            failure,
            true,
        )
        .await;
    }

    publish_projects_updated(
        auth.user_id.as_str(),
        "project_created",
        Some(saved.id.as_str()),
        Some(saved.clone()),
    );
    (
        StatusCode::CREATED,
        Json(project_value(
            saved,
            Some(json!({
                "device": device,
                "workspace": workspace,
                "bindings": bindings,
            })),
        )),
    )
}

async fn validate_local_connector_directory(
    device_id: &str,
    workspace_id: &str,
    path: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    list_local_connector_directory(device_id, workspace_id, path)
        .await
        .map(|_| ())
}

pub(crate) async fn import_local_project_to_harness(
    project_id: &str,
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> Result<(), String> {
    let sync_secret = project_sync_secret()?;
    let access = project_management_api_client::get_project_harness_git_access(
        sync_secret.as_str(),
        project_id,
        access_token_scope::get_current_access_token().as_deref(),
    )
    .await?;
    if access.project_id.trim() != project_id.trim() {
        return Err("Harness git access project identity mismatch".to_string());
    }
    if access.repo_path.trim().is_empty() || access.space_identifier.trim().is_empty() {
        return Err("Harness git access metadata is incomplete".to_string());
    }
    let push_url = authenticated_harness_git_url(
        access.git_url.as_str(),
        access.access_username.as_str(),
        access.access_token.as_str(),
    )?;
    let default_branch = access.default_branch.trim();
    if default_branch.is_empty() {
        return Err("Harness default branch is empty".to_string());
    }
    let _prefer_https_git_url = access.git_ssh_url.as_deref().is_none_or(str::is_empty);
    let command = local_harness_import_command(push_url.as_str(), default_branch);
    let value = call_local_mcp_tool(
        device_id,
        workspace_id,
        relative_path,
        &[LOCAL_CONNECTOR_BUILTIN_TERMINAL],
        "execute_command",
        json!({
            "path": ".",
            "common": command,
            "background": false,
            "timeout_ms": LOCAL_HARNESS_IMPORT_TIMEOUT_MS,
        }),
    )
    .await
    .map_err(|err| {
        let message = response_summary(&err.1 .0);
        scrub_sensitive(
            message.as_str(),
            &[push_url.as_str(), access.access_token.as_str()],
        )
    })?;
    let success = value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| value.get("exit_code").and_then(Value::as_i64) == Some(0));
    if success {
        return Ok(());
    }
    let message = value
        .get("stderr")
        .or_else(|| value.get("stdout"))
        .or_else(|| value.get("output"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Local Connector Harness import command failed");
    Err(scrub_sensitive(
        message,
        &[push_url.as_str(), access.access_token.as_str()],
    ))
}

fn project_sync_secret() -> Result<String, String> {
    let cfg = Config::try_get()?;
    cfg.project_service_sync_secret
        .as_deref()
        .or(cfg.task_runner_callback_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "project service sync secret is not configured".to_string())
}

fn authenticated_harness_git_url(
    raw_url: &str,
    username: &str,
    token: &str,
) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(raw_url).map_err(|err| format!("invalid harness git url: {err}"))?;
    url.set_username(username.trim())
        .map_err(|_| "failed to set harness git username".to_string())?;
    url.set_password(Some(token.trim()))
        .map_err(|_| "failed to set harness git token".to_string())?;
    Ok(url.to_string())
}

fn local_harness_import_command(push_url: &str, default_branch: &str) -> String {
    format!(
        r#"set -e
tmp="${{TMPDIR:-/tmp}}/chatos-harness-import-$(date +%s)-$$"
rm -rf "$tmp"
mkdir -p "$tmp"
trap 'rm -rf "$tmp"' EXIT
if command -v rsync >/dev/null 2>&1; then
  rsync -a --delete --exclude='.git/' --exclude='.chatos/' ./ "$tmp/"
else
  tar --exclude='./.git' --exclude='./.git/*' --exclude='*/.git' --exclude='*/.git/*' --exclude='./.chatos' --exclude='./.chatos/*' --exclude='*/.chatos' --exclude='*/.chatos/*' -cf - . | (cd "$tmp" && tar -xf -)
fi
cd "$tmp"
git init -b {branch} >/dev/null 2>&1 || {{ git init >/dev/null && git symbolic-ref HEAD refs/heads/{branch}; }}
git add -A -- .
git -c user.name=ChatOS -c user.email=chatos@example.invalid commit --allow-empty --no-verify -m 'Import local project into ChatOS Harness' >/dev/null
git remote add origin {push_url}
GIT_TERMINAL_PROMPT=0 git push --force origin HEAD:refs/heads/{branch}
"#,
        branch = shell_quote(default_branch),
        push_url = shell_quote(push_url)
    )
}

pub(crate) async fn call_local_mcp_tool(
    device_id: &str,
    workspace_id: &str,
    cwd: Option<&str>,
    enabled_builtin_kinds: &[&str],
    name: &str,
    arguments: Value,
) -> Result<Value, (StatusCode, Json<Value>)> {
    if enabled_builtin_kinds.is_empty() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Local Connector MCP 调用未声明 builtin capability",
        ));
    }
    let path = local_connector_mcp_relay_path(device_id, workspace_id, cwd);
    let enabled_builtin_kinds = enabled_builtin_kinds.join(",");
    let response = connector_post_json_with_headers::<Value, _>(
        path.as_str(),
        &McpToolCallRequest {
            jsonrpc: "2.0",
            id: "chatos-local-fs",
            method: "tools/call",
            params: McpToolCallParams { name, arguments },
        },
        &[(
            LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
            enabled_builtin_kinds,
        )],
    )
    .await?;
    extract_mcp_tool_result(response)
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn scrub_sensitive(value: &str, secrets: &[&str]) -> String {
    let mut output = value.to_string();
    for secret in secrets {
        let secret = secret.trim();
        if !secret.is_empty() {
            output = output.replace(secret, "***");
        }
    }
    output
}

fn extract_mcp_tool_result(response: Value) -> Result<Value, (StatusCode, Json<Value>)> {
    if let Some(mcp_error) = response.get("error") {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            json!({
                "error": "Local Connector MCP 调用失败",
                "detail": mcp_error,
            }),
        ));
    }
    if let Some(structured) = response
        .get("result")
        .and_then(|result| result.get("_structured_result"))
    {
        return Ok(structured.clone());
    }
    let text = response
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_GATEWAY,
                json!({
                    "error": "Local Connector MCP 响应格式错误",
                }),
            )
        })?;
    serde_json::from_str::<Value>(text).map_err(|err| {
        error(
            StatusCode::BAD_GATEWAY,
            json!({
                "error": "Local Connector MCP 响应解析失败",
                "detail": err.to_string(),
            }),
        )
    })
}

pub(crate) async fn create_local_connector_directory(
    device_id: &str,
    workspace_id: &str,
    path: &str,
) -> Result<LocalConnectorDirectoryCreateResponse, (StatusCode, Json<Value>)> {
    let relay_path = format!(
        "/api/local-connectors/relay/{}/workspaces/{}/directories",
        urlencoding::encode(device_id),
        urlencoding::encode(workspace_id)
    );
    connector_post_json::<LocalConnectorDirectoryCreateResponse, _>(
        relay_path.as_str(),
        &RelayWorkspaceDirectoryCreateRequest { path },
    )
    .await
}

pub(crate) async fn call_local_workspace_filesystem(
    device_id: &str,
    workspace_id: &str,
    operation: Value,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let relay_path = format!(
        "/api/local-connectors/relay/{}/workspaces/{}/filesystem",
        urlencoding::encode(device_id),
        urlencoding::encode(workspace_id)
    );
    connector_post_json(relay_path.as_str(), &operation).await
}

async fn list_local_connector_directory(
    device_id: &str,
    workspace_id: &str,
    path: &str,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let relay_path = format!(
        "/api/local-connectors/relay/{}/workspaces/{}/directories",
        urlencoding::encode(device_id),
        urlencoding::encode(workspace_id)
    );
    connector_get_json(relay_path.as_str(), &[("path", path.to_string())]).await
}

async fn create_project_binding(
    project_id: &str,
    device_id: &str,
    workspace_id: &str,
    mode: &str,
) -> Result<LocalConnectorProjectBinding, (StatusCode, Json<Value>)> {
    connector_post_json(
        "/api/local-connectors/project-bindings",
        &CreateProjectBindingRequest {
            project_id,
            device_id,
            workspace_id,
            mode,
            enabled: true,
        },
    )
    .await
}

async fn load_owned_device(
    device_id: &str,
) -> Result<LocalConnectorDevice, (StatusCode, Json<Value>)> {
    let devices =
        connector_get_json::<Vec<LocalConnectorDevice>>("/api/local-connectors/devices", &[])
            .await?;
    devices
        .into_iter()
        .find(|device| device.id == device_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Local Connector device 不存在或不属于当前用户" })),
            )
        })
}

async fn load_owned_workspace(
    device_id: &str,
    workspace_id: &str,
) -> Result<LocalConnectorWorkspace, (StatusCode, Json<Value>)> {
    let workspaces = connector_get_json::<Vec<LocalConnectorWorkspace>>(
        "/api/local-connectors/workspaces",
        &[("device_id", device_id.to_string())],
    )
    .await?;
    workspaces
        .into_iter()
        .find(|workspace| workspace.id == workspace_id && workspace.device_id == device_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Local Connector workspace 不存在或不属于当前用户" })),
            )
        })
}

async fn load_owned_online_workspace(
    device_id: &str,
    workspace_id: &str,
) -> Result<(LocalConnectorDevice, LocalConnectorWorkspace), (StatusCode, Json<Value>)> {
    let device = load_owned_device(device_id).await?;
    if device.status != LOCAL_CONNECTOR_DEVICE_ONLINE {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Local Connector device 已离线",
        ));
    }
    let workspace = load_owned_workspace(device_id, workspace_id).await?;
    if workspace.status != LOCAL_CONNECTOR_WORKSPACE_ACTIVE {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Local Connector workspace 已停用",
        ));
    }
    Ok((device, workspace))
}

pub(crate) async fn validate_local_connector_execution_target(
    device_id: &str,
    workspace_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let _device = load_owned_device(device_id).await?;
    let workspace = load_owned_workspace(device_id, workspace_id).await?;
    if workspace.status != LOCAL_CONNECTOR_WORKSPACE_ACTIVE {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Local Connector workspace 已停用",
        ));
    }
    Ok(())
}

async fn project_create_error_with_rollback(
    project: Project,
    bindings: &[LocalConnectorProjectBinding],
    err: (StatusCode, Json<Value>),
    compensate_memory: bool,
) -> (StatusCode, Json<Value>) {
    let rollback_result =
        rollback_local_connector_project(&project, bindings, compensate_memory).await;
    match rollback_result {
        Ok(()) => err,
        Err(rollback_error) => rollback_incomplete_response(err, rollback_error),
    }
}

async fn rollback_local_connector_project(
    project: &Project,
    bindings: &[LocalConnectorProjectBinding],
    compensate_memory: bool,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for binding in bindings {
        let path = format!(
            "/api/local-connectors/project-bindings/{}",
            urlencoding::encode(binding.id.as_str())
        );
        if let Err((status, detail)) = connector_delete_json(path.as_str()).await {
            failures.push(format!(
                "delete binding {} failed with {}: {}",
                binding.id,
                status,
                response_summary(&detail.0)
            ));
        }
    }
    if let Err(err) = ProjectService::delete(project.id.as_str()).await {
        failures.push(format!("delete project {} failed: {err}", project.id));
    }
    if compensate_memory {
        if let Err(err) = sync_archived_project(project).await {
            failures.push(format!("archive memory project failed: {err}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn rollback_incomplete_response(
    err: (StatusCode, Json<Value>),
    rollback_error: String,
) -> (StatusCode, Json<Value>) {
    let (status, Json(detail)) = err;
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": "Local Connector 项目创建失败，且回滚不完整",
            "original_status": status.as_u16(),
            "detail": detail,
            "rollback_error": rollback_error,
        })),
    )
}

fn response_summary(value: &Value) -> String {
    value
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| value.get("detail").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn required_text(value: Option<String>, field: &str) -> Result<String, (StatusCode, Json<Value>)> {
    normalize_non_empty(value).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{field} 不能为空") })),
        )
    })
}

pub(crate) async fn validate_local_connector_workspace_ref(
    root_ref: &LocalConnectorRootRef,
) -> Result<String, (StatusCode, Json<Value>)> {
    let (_, workspace) =
        load_owned_online_workspace(root_ref.device_id.as_str(), root_ref.workspace_id.as_str())
            .await?;
    Ok(workspace.local_path_alias)
}

fn project_value(project: Project, local_connector: Option<Value>) -> Value {
    let internal_root_path = project.root_path.clone();
    let display_root_path = local_connector_display_path(project.root_path.as_str())
        .unwrap_or_else(|| display_path(project.root_path.as_str()));
    let mut value = serde_json::to_value(project).unwrap_or(Value::Null);
    if let Value::Object(ref mut map) = value {
        map.insert(
            "root_path".to_string(),
            Value::String(internal_root_path.clone()),
        );
        map.insert("rootPath".to_string(), Value::String(internal_root_path));
        map.insert(
            "display_root_path".to_string(),
            Value::String(display_root_path),
        );
        if let Some(local_connector) = local_connector {
            map.insert("local_connector".to_string(), local_connector.clone());
            map.insert("localConnector".to_string(), local_connector);
        }
    }
    value
}

fn error(status: StatusCode, payload: impl Into<Value>) -> (StatusCode, Json<Value>) {
    let payload = payload.into();
    match payload {
        Value::String(message) => (status, Json(json!({ "error": message }))),
        other => (status, Json(other)),
    }
}
