// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_mcp_service::{
    BUILTIN_KIND_CODE_MAINTAINER_READ, BUILTIN_KIND_CODE_MAINTAINER_WRITE,
    BUILTIN_KIND_TERMINAL_CONTROLLER, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use tracing::warn;

use crate::api::projects::memory_sync::sync_active_project;
use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::core::validation::normalize_non_empty;
use crate::models::project::{Project, ProjectService};
use crate::services::realtime::publish_projects_updated;

mod connector_client;
mod directory_payload;
mod root_path;
mod terminal_relay;
mod types;

use connector_client::{
    connector_delete_json, connector_get_json, connector_post_json,
    connector_post_json_with_headers, local_connector_mcp_relay_path,
};
use directory_payload::local_connector_directory_list_payload;
pub(crate) use root_path::{
    local_connector_root_path, parse_local_connector_root_path, LocalConnectorRootRef,
};
use root_path::{
    local_relative_basename, sanitize_optional_local_relative_path,
    sanitize_required_local_relative_path,
};
pub(crate) use terminal_relay::{create_local_terminal_session, send_local_terminal_input};
use types::{
    CreateLocalConnectorProjectRequest, CreateLocalDirectoryRequest, CreateProjectBindingRequest,
    DeviceQuery, LocalConnectorDevice, LocalConnectorProjectBinding, LocalConnectorWorkspace,
    LocalFsQuery, McpToolCallParams, McpToolCallRequest, WorkspaceQuery,
};
const LOCAL_CONNECTOR_BINDING_MODE_MCP: &str = "local_mcp";
const LOCAL_CONNECTOR_BINDING_MODE_TERMINAL: &str = "local_terminal";
const LOCAL_CONNECTOR_DEVICE_ONLINE: &str = "online";
const LOCAL_CONNECTOR_WORKSPACE_ACTIVE: &str = "active";
pub(crate) const LOCAL_CONNECTOR_BUILTIN_CODE_READ: &str = BUILTIN_KIND_CODE_MAINTAINER_READ;
pub(crate) const LOCAL_CONNECTOR_BUILTIN_CODE_WRITE: &str = BUILTIN_KIND_CODE_MAINTAINER_WRITE;
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
