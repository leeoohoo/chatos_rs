// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::time::Duration;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::api::projects::memory_sync::sync_active_project;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::core::validation::normalize_non_empty;
use crate::models::project::{Project, ProjectService};
use crate::services::access_token_scope;
use crate::services::realtime::publish_projects_updated;

const LOCAL_CONNECTOR_BINDING_MODE_MCP: &str = "local_mcp";
const LOCAL_CONNECTOR_BINDING_MODE_TERMINAL: &str = "local_terminal";
const LOCAL_CONNECTOR_DEVICE_ONLINE: &str = "online";
const LOCAL_CONNECTOR_WORKSPACE_ACTIVE: &str = "active";
const LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER: &str =
    "x-local-connector-enabled-builtin-kinds";
pub(crate) const LOCAL_CONNECTOR_BUILTIN_CODE_READ: &str = "CodeMaintainerRead";
pub(crate) const LOCAL_CONNECTOR_BUILTIN_CODE_WRITE: &str = "CodeMaintainerWrite";
pub(crate) const LOCAL_CONNECTOR_BUILTIN_TERMINAL: &str = "TerminalController";
pub(crate) const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalConnectorRootRef {
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) relative_path: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/local-connectors/devices", get(list_devices))
        .route("/api/local-connectors/workspaces", get(list_workspaces))
        .route("/api/local-connectors/fs/list", get(list_directory))
        .route("/api/local-connectors/fs/mkdir", post(create_directory))
        .route("/api/local-connectors/projects", post(create_project))
        .route(
            "/api/local-connectors/terminal/exec",
            post(exec_terminal_command),
        )
}

#[derive(Debug, Deserialize)]
struct DeviceQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceQuery {
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalFsQuery {
    device_id: Option<String>,
    workspace_id: Option<String>,
    path: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateLocalDirectoryRequest {
    device_id: Option<String>,
    workspace_id: Option<String>,
    path: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateLocalConnectorProjectRequest {
    name: Option<String>,
    device_id: Option<String>,
    workspace_id: Option<String>,
    relative_path: Option<String>,
    git_url: Option<String>,
    description: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalTerminalExecRequest {
    device_id: Option<String>,
    workspace_id: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LocalConnectorDevice {
    id: String,
    owner_user_id: String,
    display_name: String,
    public_key: String,
    client_version: Option<String>,
    os: Option<String>,
    status: String,
    last_seen_at: Option<String>,
    revoked_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LocalConnectorWorkspace {
    id: String,
    owner_user_id: String,
    device_id: String,
    display_name: String,
    local_path_alias: String,
    local_path_fingerprint: String,
    capabilities: Vec<String>,
    status: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LocalConnectorProjectBinding {
    id: String,
    owner_user_id: String,
    project_id: String,
    device_id: String,
    workspace_id: String,
    mode: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct CreateProjectBindingRequest<'a> {
    project_id: &'a str,
    device_id: &'a str,
    workspace_id: &'a str,
    mode: &'a str,
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct RelayTerminalExecRequest<'a> {
    workspace_id: &'a str,
    command: &'a str,
    args: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct RelayTerminalSessionCreateRequest<'a> {
    workspace_id: &'a str,
    terminal_session_id: &'a str,
    cwd: Option<&'a str>,
    cols: u16,
    rows: u16,
}

#[derive(Debug, Serialize)]
struct RelayTerminalInputRequest<'a> {
    workspace_id: &'a str,
    terminal_session_id: &'a str,
    data: &'a str,
}

#[derive(Debug, Serialize)]
struct McpToolCallRequest<'a> {
    jsonrpc: &'static str,
    id: &'static str,
    method: &'static str,
    params: McpToolCallParams<'a>,
}

#[derive(Debug, Serialize)]
struct McpToolCallParams<'a> {
    name: &'a str,
    arguments: Value,
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
    if let Err(err) = load_owned_online_workspace(device_id.as_str(), workspace_id.as_str()).await {
        return err;
    }
    let path = match sanitize_optional_local_relative_path(query.path.as_deref()) {
        Ok(Some(path)) => path,
        Ok(None) => ".".to_string(),
        Err(err) => return err,
    };
    match call_local_mcp_tool(
        device_id.as_str(),
        workspace_id.as_str(),
        None,
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "list_dir",
        json!({ "path": path, "max_entries": 1000 }),
    )
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
    match call_local_mcp_tool(
        device_id.as_str(),
        workspace_id.as_str(),
        None,
        &[LOCAL_CONNECTOR_BUILTIN_TERMINAL],
        "execute_command",
        json!({
            "path": ".",
            "common": format!("mkdir -p -- {}", shell_quote(path.as_str())),
            "background": false,
        }),
    )
    .await
    {
        Ok(value) if value.get("exit_code").and_then(Value::as_i64).unwrap_or(0) == 0 => (
            StatusCode::OK,
            Json(json!({
                "path": path,
                "created": true,
            })),
        ),
        Ok(value) => error(
            StatusCode::BAD_GATEWAY,
            json!({
                "error": "Local Connector 创建目录失败",
                "detail": value,
            }),
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
                rollback_local_connector_project(saved.id.as_str(), &bindings).await;
                return err;
            }
        }
    }

    if let Err(err) = sync_active_project(&saved).await {
        warn!(
            project_id = saved.id.as_str(),
            error = err.as_str(),
            "sync memory project failed after local connector project create"
        );
        rollback_local_connector_project(saved.id.as_str(), &bindings).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({
                "error": "sync memory project failed",
                "detail": err,
            }),
        );
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

async fn exec_terminal_command(
    auth: AuthUser,
    Json(req): Json<LocalTerminalExecRequest>,
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
    let command = match required_text(req.command, "command") {
        Ok(value) => value,
        Err(err) => return err,
    };
    if let Err(err) = load_owned_online_workspace(device_id.as_str(), workspace_id.as_str()).await {
        return err;
    }
    let path = format!(
        "/api/local-connectors/relay/{}/terminal/exec",
        urlencoding::encode(device_id.as_str())
    );
    match connector_post_json::<Value, _>(
        path.as_str(),
        &RelayTerminalExecRequest {
            workspace_id: workspace_id.as_str(),
            command: command.as_str(),
            args: req.args.unwrap_or_default(),
            cwd: normalize_non_empty(req.cwd),
            timeout_ms: req.timeout_ms,
            source: normalize_non_empty(req.source),
        },
    )
    .await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => err,
    }
}

async fn validate_local_connector_directory(
    device_id: &str,
    workspace_id: &str,
    path: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    call_local_mcp_tool(
        device_id,
        workspace_id,
        None,
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "list_dir",
        json!({ "path": path, "max_entries": 1 }),
    )
    .await
    .map(|_| ())
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

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) async fn create_local_terminal_session(
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    cwd: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/terminal/sessions",
        urlencoding::encode(device_id)
    );
    connector_post_json::<Value, _>(
        path.as_str(),
        &RelayTerminalSessionCreateRequest {
            workspace_id,
            terminal_session_id,
            cwd,
            cols: cols.max(1),
            rows: rows.max(1),
        },
    )
    .await
}

pub(crate) async fn send_local_terminal_input(
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    data: &str,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/terminal/input",
        urlencoding::encode(device_id)
    );
    connector_post_json::<Value, _>(
        path.as_str(),
        &RelayTerminalInputRequest {
            workspace_id,
            terminal_session_id,
            data,
        },
    )
    .await
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

async fn rollback_local_connector_project(
    project_id: &str,
    bindings: &[LocalConnectorProjectBinding],
) {
    for binding in bindings {
        let path = format!(
            "/api/local-connectors/project-bindings/{}",
            urlencoding::encode(binding.id.as_str())
        );
        let _ = connector_delete_json(path.as_str()).await;
    }
    let _ = ProjectService::delete(project_id).await;
}

async fn connector_get_json<T: DeserializeOwned>(
    path: &str,
    query: &[(&str, String)],
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = reqwest::Client::new()
        .get(connector_url(cfg, path))
        .bearer_auth(token)
        .query(query)
        .timeout(connector_timeout(cfg));
    send_connector_json(request).await
}

async fn connector_post_json<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
) -> Result<T, (StatusCode, Json<Value>)> {
    connector_post_json_with_headers(path, body, &[]).await
}

async fn connector_post_json_with_headers<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
    headers: &[(&str, String)],
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let mut request = reqwest::Client::new()
        .post(connector_url(cfg, path))
        .bearer_auth(token)
        .json(body)
        .timeout(connector_timeout(cfg));
    for (key, value) in headers {
        request = request.header(*key, value.as_str());
    }
    send_connector_json(request).await
}

async fn connector_delete_json(path: &str) -> Result<Value, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = reqwest::Client::new()
        .delete(connector_url(cfg, path))
        .bearer_auth(token)
        .timeout(connector_timeout(cfg));
    send_connector_json(request).await
}

async fn send_connector_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, (StatusCode, Json<Value>)> {
    let response = request
        .send()
        .await
        .map_err(|err| connector_unavailable(err.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| connector_unavailable(err.to_string()))?;
    let value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str::<Value>(text.as_str()).unwrap_or_else(|_| {
            json!({
                "error": text,
            })
        })
    };
    if !status.is_success() {
        return Err((status, Json(value)));
    }
    serde_json::from_value(value).map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "Local Connector service 响应格式错误",
                "detail": err.to_string(),
            })),
        )
    })
}

fn current_access_token() -> Result<String, (StatusCode, Json<Value>)> {
    access_token_scope::get_current_access_token().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "当前请求缺少可转发的 access token" })),
        )
    })
}

fn connector_url(cfg: &Config, path: &str) -> String {
    format!(
        "{}{}",
        cfg.local_connector_service_base_url
            .trim()
            .trim_end_matches('/'),
        path
    )
}

fn local_connector_mcp_relay_path(
    device_id: &str,
    workspace_id: &str,
    cwd: Option<&str>,
) -> String {
    let mut path = format!(
        "/api/local-connectors/relay/{}/mcp?workspace_id={}",
        urlencoding::encode(device_id),
        urlencoding::encode(workspace_id)
    );
    if let Some(cwd) = cwd.and_then(|value| normalize_local_relative_path(Some(value))) {
        path.push_str("&cwd=");
        path.push_str(urlencoding::encode(cwd.as_str()).as_ref());
    }
    path
}

fn connector_timeout(cfg: &Config) -> Duration {
    Duration::from_millis(cfg.local_connector_service_request_timeout_ms.max(300) as u64)
}

fn connector_unavailable(detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": "Local Connector service 不可用",
            "detail": detail,
        })),
    )
}

fn local_connector_directory_list_payload(path: &str, value: Value) -> Value {
    let mut entries = value
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let is_dir = entry
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| entry.get("type").and_then(Value::as_str) == Some("dir"));
            json!({
                "name": entry.get("name").cloned().unwrap_or(Value::Null),
                "path": entry.get("path").cloned().unwrap_or(Value::Null),
                "is_dir": is_dir,
                "len": entry
                    .get("len")
                    .or_else(|| entry.get("size"))
                    .cloned()
                    .unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
        let right_dir = right
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if left_dir != right_dir {
            return right_dir.cmp(&left_dir);
        }
        let left_name = left
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let right_name = right
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        left_name.cmp(&right_name)
    });
    json!({
        "path": if path.trim().is_empty() { "." } else { path },
        "parent": Value::Null,
        "entries": entries,
    })
}

fn required_text(value: Option<String>, field: &str) -> Result<String, (StatusCode, Json<Value>)> {
    normalize_non_empty(value).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{field} 不能为空") })),
        )
    })
}

fn sanitize_optional_local_relative_path(
    value: Option<&str>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(normalized) = normalize_local_relative_path(Some(value)) else {
        return Ok(None);
    };
    if local_relative_path_is_safe(normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            "本地目录路径不能包含 .. 或绝对路径",
        ))
    }
}

fn sanitize_required_local_relative_path(
    value: Option<&str>,
    field: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    match sanitize_optional_local_relative_path(value)? {
        Some(value) => Ok(value),
        None => Err(error(StatusCode::BAD_REQUEST, format!("{field} 不能为空"))),
    }
}

fn normalize_local_relative_path(value: Option<&str>) -> Option<String> {
    let value = value?.trim().replace('\\', "/");
    let value = value.trim_matches('/');
    if value.is_empty() || value == "." {
        return None;
    }
    let parts = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn local_relative_path_is_safe(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path.split('/').all(|part| {
            let part = part.trim();
            !part.is_empty() && part != "." && part != ".."
        })
}

fn local_relative_basename(path: &str) -> Option<String> {
    normalize_local_relative_path(Some(path)).and_then(|path| {
        path.rsplit('/')
            .find(|part| !part.trim().is_empty())
            .map(ToOwned::to_owned)
    })
}

fn encode_local_connector_relative_path(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.trim().is_empty())
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_local_connector_relative_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/').filter(|part| !part.trim().is_empty()) {
        let decoded = urlencoding::decode(part).ok()?.into_owned();
        parts.push(decoded);
    }
    let joined = parts.join("/");
    normalize_local_relative_path(Some(joined.as_str()))
        .filter(|path| local_relative_path_is_safe(path))
}

pub(crate) fn parse_local_connector_root_path(root_path: &str) -> Option<LocalConnectorRootRef> {
    let rest = root_path.trim().strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.splitn(3, '/');
    let device_id = parts.next()?.trim();
    let workspace_id = parts.next()?.trim();
    if device_id.is_empty() || workspace_id.is_empty() {
        return None;
    }
    let relative_path = match parts.next() {
        Some(path) => Some(decode_local_connector_relative_path(path)?),
        None => None,
    };
    Some(LocalConnectorRootRef {
        device_id: device_id.to_string(),
        workspace_id: workspace_id.to_string(),
        relative_path,
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

pub(crate) fn local_connector_root_path(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> String {
    let base = format!("{LOCAL_CONNECTOR_ROOT_PREFIX}{device_id}/{workspace_id}");
    match relative_path.and_then(|value| normalize_local_relative_path(Some(value))) {
        Some(relative_path) => format!(
            "{base}/{}",
            encode_local_connector_relative_path(relative_path.as_str())
        ),
        None => base,
    }
}

fn project_value(project: Project, local_connector: Option<Value>) -> Value {
    let display_root_path = display_path(project.root_path.as_str());
    let mut value = serde_json::to_value(project).unwrap_or(Value::Null);
    if let Value::Object(ref mut map) = value {
        map.insert(
            "root_path".to_string(),
            Value::String(display_root_path.clone()),
        );
        map.insert(
            "rootPath".to_string(),
            Value::String(display_root_path.clone()),
        );
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
