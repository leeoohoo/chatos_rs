// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::RemoteConnection;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_mcp_service::{
    BUILTIN_KIND_CODE_MAINTAINER_READ, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
mod connector_client;
mod directory_payload;
mod root_path;
mod terminal_relay;
mod types;
use connector_client::{
    connector_get_json, connector_post_json, connector_post_json_with_headers,
    connector_post_json_with_headers_and_timeout, connector_post_json_with_timeout,
    local_connector_mcp_relay_path,
};
pub(crate) use connector_client::{local_connector_tls_connector, local_connector_websocket_url};
use directory_payload::local_connector_directory_list_payload;
pub(crate) use root_path::{
    local_connector_root_path, parse_local_connector_root_path, LocalConnectorRootRef,
};
use root_path::{sanitize_optional_local_relative_path, sanitize_required_local_relative_path};
pub(crate) use terminal_relay::{
    close_local_terminal_session, create_local_terminal_session, send_local_terminal_input,
};
use types::{
    CreateLocalDirectoryRequest, DeviceQuery, LocalConnectorDevice,
    LocalConnectorDirectoryCreateResponse, LocalConnectorWorkspace, LocalFsQuery,
    McpToolCallParams, McpToolCallRequest, RelayWorkspaceDirectoryCreateRequest, WorkspaceQuery,
};
const LOCAL_CONNECTOR_DEVICE_ONLINE: &str = "online";
const LOCAL_CONNECTOR_WORKSPACE_ACTIVE: &str = "active";
pub(crate) const LOCAL_CONNECTOR_BUILTIN_CODE_READ: &str = BUILTIN_KIND_CODE_MAINTAINER_READ;
pub fn router() -> Router {
    Router::new()
        .route("/api/local-connectors/devices", get(list_devices))
        .route("/api/local-connectors/workspaces", get(list_workspaces))
        .route("/api/local-connectors/fs/list", get(list_directory))
        .route("/api/local-connectors/fs/mkdir", post(create_directory))
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

pub(crate) async fn call_local_mcp_tool(
    device_id: &str,
    workspace_id: &str,
    cwd: Option<&str>,
    enabled_builtin_kinds: &[&str],
    name: &str,
    arguments: Value,
) -> Result<Value, (StatusCode, Json<Value>)> {
    call_local_mcp_tool_with_optional_timeout(
        device_id,
        workspace_id,
        cwd,
        enabled_builtin_kinds,
        name,
        arguments,
        None,
    )
    .await
}

async fn call_local_mcp_tool_with_optional_timeout(
    device_id: &str,
    workspace_id: &str,
    cwd: Option<&str>,
    enabled_builtin_kinds: &[&str],
    name: &str,
    arguments: Value,
    timeout: Option<Duration>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    if enabled_builtin_kinds.is_empty() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Local Connector MCP 调用未声明 builtin capability",
        ));
    }
    let path = local_connector_mcp_relay_path(device_id, workspace_id, cwd);
    let enabled_builtin_kinds = enabled_builtin_kinds.join(",");
    let request = McpToolCallRequest {
        jsonrpc: "2.0",
        id: "chatos-local-fs",
        method: "tools/call",
        params: McpToolCallParams { name, arguments },
    };
    let headers = [(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
        enabled_builtin_kinds,
    )];
    let response = match timeout {
        Some(timeout) => {
            connector_post_json_with_headers_and_timeout::<Value, _>(
                path.as_str(),
                &request,
                &headers,
                timeout,
            )
            .await?
        }
        None => {
            connector_post_json_with_headers::<Value, _>(path.as_str(), &request, &headers).await?
        }
    };
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

fn error(status: StatusCode, payload: impl Into<Value>) -> (StatusCode, Json<Value>) {
    let payload = payload.into();
    match payload {
        Value::String(message) => (status, Json(json!({ "error": message }))),
        other => (status, Json(other)),
    }
}
