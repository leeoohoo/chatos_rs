// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{
    header::{ACCEPT, CONTENT_TYPE},
    HeaderMap, Method, StatusCode, Uri,
};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::{
    normalize_optional_text, CurrentUser, HealthResponse, DEVICE_STATUS_ONLINE,
    WORKSPACE_STATUS_DISABLED,
};
use crate::relay::{RelayError, RelayRequest, RelayResponse};
use crate::state::AppState;

const DEFAULT_TERMINAL_EXEC_TIMEOUT_MS: u64 = 30_000;
const MAX_TERMINAL_EXEC_TIMEOUT_MS: u64 = 10 * 60 * 1000;

mod auth_middleware;
mod devices;
mod project_bindings;
mod router;
mod sandbox_pairings;
mod workspaces;

use self::auth_middleware::require_auth;
pub use self::auth_middleware::ApiError;
use self::devices::{
    connect_device, create_device, disconnect_device, get_device, heartbeat_device, list_devices,
    load_owned_device, revoke_device,
};
use self::project_bindings::{
    create_project_binding, delete_project_binding, list_project_bindings, update_project_binding,
};
pub use self::router::build_router;
use self::sandbox_pairings::{
    create_sandbox_pairing, delete_sandbox_pairing, list_sandbox_pairings,
    load_owned_sandbox_pairing, update_sandbox_pairing,
};
use self::workspaces::{
    create_workspace, delete_workspace, list_workspaces, load_owned_workspace, update_workspace,
};

#[derive(Debug, Deserialize)]
struct McpRelayQuery {
    workspace_id: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalExecRelayRequest {
    workspace_id: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionCreateRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct TerminalInputRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    data: Option<String>,
    command: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalWsRelayQuery {
    workspace_id: Option<String>,
    terminal_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "local_connector_service".to_string(),
    })
}

async fn current_user_handler(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

async fn memory_engine_proxy(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(path): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let suffix = normalize_memory_engine_proxy_suffix(path.as_str())?;
    validate_memory_engine_proxy_request(
        &method,
        suffix.as_str(),
        uri.query(),
        body.as_ref(),
        &user,
    )?;
    let operator_token = state
        .config
        .memory_engine_operator_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::internal("Local Connector Service Memory Engine secret is not configured")
        })?;
    let mut target_url = format!(
        "{}/{}",
        state.config.memory_engine_base_url.trim_end_matches('/'),
        suffix
    );
    if let Some(query) = uri.query().map(str::trim).filter(|value| !value.is_empty()) {
        target_url.push('?');
        target_url.push_str(query);
    }

    let client = reqwest::Client::builder()
        .timeout(state.config.memory_engine_request_timeout)
        .build()
        .map_err(|err| ApiError::internal(format!("build Memory Engine client failed: {err}")))?;
    let mut request = client
        .request(method.clone(), target_url.as_str())
        .header("x-memory-operator-token", operator_token);
    if let Some(content_type) = headers.get(CONTENT_TYPE) {
        request = request.header(CONTENT_TYPE.as_str(), content_type);
    }
    if let Some(accept) = headers.get(ACCEPT) {
        request = request.header(ACCEPT.as_str(), accept);
    }
    if !body.is_empty() {
        request = request.body(body.clone());
    }
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("Memory Engine request failed: {err}")))?;
    let status = StatusCode::from_u16(response.status().as_u16()).map_err(|err| {
        ApiError::bad_gateway(format!("Memory Engine returned invalid status: {err}"))
    })?;
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response.bytes().await.map_err(|err| {
        ApiError::bad_gateway(format!("read Memory Engine response failed: {err}"))
    })?;
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }
    builder.body(Body::from(bytes)).map_err(|err| {
        ApiError::internal(format!("build Memory Engine proxy response failed: {err}"))
    })
}

async fn mcp_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<McpRelayQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(query.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let mut relay_headers = relay_headers(&headers);
    if let Some(cwd) = normalize_optional_text(query.cwd) {
        relay_headers.insert("x-local-connector-cwd".to_string(), cwd);
    }
    let request = RelayRequest {
        message_type: "mcp".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/mcp".to_string(),
        headers: relay_headers,
        body: relay_body(body.as_ref()),
    };
    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn resolve_model_runtime(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(model_config_id): Path<String>,
) -> Result<Response, ApiError> {
    let model_config_id = required_text(Some(model_config_id), "model_config_id")?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    let device = state
        .store
        .list_devices(owner_user_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .find(|device| device.status == DEVICE_STATUS_ONLINE)
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Local Connector client is offline; model request was terminated",
            )
        })?;

    let request = RelayRequest {
        message_type: "model_runtime_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id,
        device_id: device.id,
        workspace_id: String::new(),
        method: "GET".to_string(),
        path: format!("/model-runtime/{model_config_id}"),
        headers: BTreeMap::new(),
        body: json!({ "model_config_id": model_config_id }),
    };
    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn terminal_exec_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalExecRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let command = required_text(req.command, "command")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let timeout_ms = req
        .timeout_ms
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);
    let relay_timeout = state
        .config
        .relay_request_timeout
        .max(Duration::from_millis(timeout_ms.saturating_add(5_000)));

    let request = RelayRequest {
        message_type: "terminal_exec_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/exec".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "command": command,
            "args": req.args.unwrap_or_default(),
            "cwd": normalize_optional_text(req.cwd),
            "timeout_ms": timeout_ms,
            "source": normalize_optional_text(req.source),
        }),
    };

    let response = state
        .relay
        .dispatch(request, relay_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn terminal_session_create_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalSessionCreateRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "terminal_session_create_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/sessions".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id,
            "cwd": normalize_optional_text(req.cwd),
            "cols": req.cols.unwrap_or(80).max(1),
            "rows": req.rows.unwrap_or(24).max(1),
        }),
    };

    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn terminal_input_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalInputRelayRequest>,
) -> Result<Json<Value>, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    let data = req.data.unwrap_or_default();
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "terminal_input".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/terminal_input".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id,
            "data": data,
            "command": normalize_optional_text(req.command),
        }),
    };

    state
        .relay
        .send(request)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(Json(json!({ "success": true })))
}

async fn terminal_ws_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<TerminalWsRelayQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(query.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let terminal_session_id =
        normalize_optional_text(query.terminal_id).unwrap_or_else(|| Uuid::new_v4().to_string());
    let cwd = normalize_optional_text(query.cwd);
    let cols = query.cols.unwrap_or(80).max(1);
    let rows = query.rows.unwrap_or(24).max(1);
    let owner_user_id = user.effective_owner_user_id().to_string();
    Ok(ws
        .on_upgrade(move |socket| {
            handle_terminal_relay_socket(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                terminal_session_id,
                cwd,
                cols,
                rows,
                socket,
            )
        })
        .into_response())
}

async fn handle_terminal_relay_socket(
    state: AppState,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    terminal_session_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    mut socket: WebSocket,
) {
    let mut events = state
        .relay
        .subscribe_terminal_session(terminal_session_id.as_str())
        .await;
    let create_request = RelayRequest {
        message_type: "terminal_session_create_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.clone(),
        device_id: device_id.clone(),
        workspace_id: workspace_id.clone(),
        method: "POST".to_string(),
        path: "/terminal/sessions".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id.as_str(),
            "cwd": cwd,
            "cols": cols,
            "rows": rows,
        }),
    };
    let create_response = state
        .relay
        .dispatch(create_request, state.config.relay_request_timeout)
        .await;
    match create_response {
        Ok(response) if (200..300).contains(&response.status) => {
            let snapshot = response
                .body
                .get("snapshot")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !snapshot.is_empty()
                && socket
                    .send(Message::Text(
                        json!({"type": "snapshot", "data": snapshot})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .is_err()
            {
                state
                    .relay
                    .drop_terminal_session(terminal_session_id.as_str())
                    .await;
                return;
            }
            let busy = response
                .body
                .get("busy")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if socket
                .send(Message::Text(
                    json!({"type": "state", "busy": busy, "snapshot_paging": true})
                        .to_string()
                        .into(),
                ))
                .await
                .is_err()
            {
                state
                    .relay
                    .drop_terminal_session(terminal_session_id.as_str())
                    .await;
                return;
            }
        }
        Ok(response) => {
            let message = response
                .body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Local Connector terminal startup failed");
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": message})
                        .to_string()
                        .into(),
                ))
                .await;
            state
                .relay
                .drop_terminal_session(terminal_session_id.as_str())
                .await;
            return;
        }
        Err(err) => {
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": err.message()})
                        .to_string()
                        .into(),
                ))
                .await;
            state
                .relay
                .drop_terminal_session(terminal_session_id.as_str())
                .await;
            return;
        }
    }

    let (mut sender, mut receiver) = socket.split();
    let event_task = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let payload =
                        terminal_event_to_ws_payload(event.message_type.as_str(), &event.body);
                    let Some(payload) = payload else {
                        continue;
                    };
                    if sender
                        .send(Message::Text(payload.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if event.message_type == "terminal_exit" {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if !handle_terminal_ws_input(
                    &state,
                    owner_user_id.as_str(),
                    device_id.as_str(),
                    workspace_id.as_str(),
                    terminal_session_id.as_str(),
                    text.as_str(),
                )
                .await
                {
                    break;
                }
            }
            Ok(Message::Binary(bytes)) => {
                let data = String::from_utf8_lossy(&bytes).to_string();
                if !data.is_empty()
                    && !send_terminal_control(
                        &state,
                        owner_user_id.as_str(),
                        device_id.as_str(),
                        workspace_id.as_str(),
                        "terminal_input",
                        terminal_session_id.as_str(),
                        json!({ "data": data }),
                    )
                    .await
                {
                    break;
                }
            }
            Ok(Message::Ping(_)) => {}
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    let _ = send_terminal_control(
        &state,
        owner_user_id.as_str(),
        device_id.as_str(),
        workspace_id.as_str(),
        "terminal_close",
        terminal_session_id.as_str(),
        json!({}),
    )
    .await;
    state
        .relay
        .drop_terminal_session(terminal_session_id.as_str())
        .await;
    event_task.abort();
}

async fn handle_terminal_ws_input(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    text: &str,
) -> bool {
    let parsed = serde_json::from_str::<Value>(text);
    let Ok(value) = parsed else {
        return send_terminal_control(
            state,
            owner_user_id,
            device_id,
            workspace_id,
            "terminal_input",
            terminal_session_id,
            json!({ "data": text }),
        )
        .await;
    };
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match message_type {
        "input" => {
            let data = value
                .get("data")
                .and_then(Value::as_str)
                .unwrap_or_default();
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_input",
                terminal_session_id,
                json!({ "data": data }),
            )
            .await
        }
        "resize" => {
            let cols = value.get("cols").and_then(Value::as_u64).unwrap_or(80);
            let rows = value.get("rows").and_then(Value::as_u64).unwrap_or(24);
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_resize",
                terminal_session_id,
                json!({ "cols": cols, "rows": rows }),
            )
            .await
        }
        "snapshot" => {
            let lines = value.get("lines").and_then(Value::as_u64).unwrap_or(500);
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_snapshot_request",
                terminal_session_id,
                json!({ "lines": lines }),
            )
            .await
        }
        "command" => {
            let command = value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_command",
                terminal_session_id,
                json!({ "command": command }),
            )
            .await
        }
        "ping" => true,
        _ => true,
    }
}

async fn send_terminal_control(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    message_type: &str,
    terminal_session_id: &str,
    mut body: Value,
) -> bool {
    if let Value::Object(ref mut map) = body {
        map.insert(
            "terminal_session_id".to_string(),
            Value::String(terminal_session_id.to_string()),
        );
    }
    let request = RelayRequest {
        message_type: message_type.to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.to_string(),
        device_id: device_id.to_string(),
        workspace_id: workspace_id.to_string(),
        method: "POST".to_string(),
        path: format!("/terminal/{message_type}"),
        headers: BTreeMap::new(),
        body,
    };
    state.relay.send(request).await.is_ok()
}

fn terminal_event_to_ws_payload(message_type: &str, body: &Value) -> Option<Value> {
    match message_type {
        "terminal_output" => Some(json!({
            "type": "output",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
        })),
        "terminal_snapshot" => Some(json!({
            "type": "snapshot",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
        })),
        "terminal_exit" => Some(json!({
            "type": "exit",
            "code": body.get("code").and_then(Value::as_i64).unwrap_or(0),
        })),
        "terminal_state" => Some(json!({
            "type": "state",
            "busy": body.get("busy").and_then(Value::as_bool).unwrap_or(false),
            "snapshot_paging": true,
        })),
        "terminal_error" => Some(json!({
            "type": "error",
            "error": body.get("error").and_then(Value::as_str).unwrap_or("Local Connector terminal error"),
        })),
        _ => None,
    }
}

async fn sandbox_facade_root(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(pairing_id): Path<String>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    sandbox_facade_impl(
        state,
        user,
        pairing_id,
        String::new(),
        method,
        headers,
        body,
    )
    .await
}

async fn sandbox_facade_path(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((pairing_id, path)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    sandbox_facade_impl(state, user, pairing_id, path, method, headers, body).await
}

async fn sandbox_facade_impl(
    state: AppState,
    user: CurrentUser,
    pairing_id: String,
    path: String,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let pairing = load_owned_sandbox_pairing(&state, &user, pairing_id.as_str()).await?;
    if !pairing.enabled {
        return Err(ApiError::bad_request(
            "Local Connector sandbox pairing is disabled",
        ));
    }
    validate_device_workspace(
        &state,
        &user,
        pairing.device_id.as_str(),
        pairing.workspace_id.as_str(),
    )
    .await?;

    let request = RelayRequest {
        message_type: "sandbox_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id: pairing.device_id.clone(),
        workspace_id: pairing.workspace_id.clone(),
        method: method.as_str().to_string(),
        path: normalize_relay_path(path.as_str()),
        headers: relay_headers(&headers),
        body: relay_body(body.as_ref()),
    };

    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn validate_device_workspace(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    workspace_id: &str,
) -> Result<(), ApiError> {
    let device = load_owned_device(state, user, device_id, true).await?;
    if device.status != DEVICE_STATUS_ONLINE {
        return Err(ApiError::service_unavailable(
            "Local Connector device is offline",
        ));
    }
    let workspace = load_owned_workspace(state, user, workspace_id).await?;
    if workspace.device_id != device.id {
        return Err(ApiError::bad_request(
            "Local Connector workspace is not attached to the selected device",
        ));
    }
    if workspace.status == WORKSPACE_STATUS_DISABLED {
        return Err(ApiError::bad_request(
            "Local Connector workspace is disabled",
        ));
    }
    Ok(())
}

fn normalize_memory_engine_proxy_suffix(path: &str) -> Result<String, ApiError> {
    let path = path.trim().trim_start_matches('/');
    let suffix = path
        .strip_prefix("api/memory-engine/v1/")
        .or_else(|| path.strip_prefix("api/memory-engine/v1"))
        .unwrap_or(path)
        .trim_start_matches('/');
    if suffix.is_empty() {
        return Err(ApiError::bad_request(
            "Memory Engine proxy path is required",
        ));
    }
    Ok(suffix.to_string())
}

fn validate_memory_engine_proxy_request(
    method: &Method,
    suffix: &str,
    query: Option<&str>,
    body: &[u8],
    user: &CurrentUser,
) -> Result<(), ApiError> {
    if !memory_engine_proxy_path_allowed(method, suffix) {
        return Err(ApiError::forbidden(
            "Memory Engine proxy path is not allowed for Local Connector approval memory",
        ));
    }
    if suffix == "admin/sources/local_connector_approval" {
        return Ok(());
    }

    let parsed_body =
        if body.is_empty() {
            None
        } else {
            Some(serde_json::from_slice::<Value>(body).map_err(|_| {
                ApiError::bad_request("Memory Engine proxy body must be valid JSON")
            })?)
        };
    let tenant_id = query_param(query, "tenant_id")
        .or_else(|| {
            parsed_body
                .as_ref()
                .and_then(|value| json_string_field(value, "tenant_id"))
        })
        .ok_or_else(|| ApiError::bad_request("Memory Engine proxy tenant_id is required"))?;
    if tenant_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Memory Engine proxy tenant_id does not match current user",
        ));
    }

    if let Some(source_id) = query_param(query, "source_id").or_else(|| {
        parsed_body
            .as_ref()
            .and_then(|value| json_string_field(value, "source_id"))
    }) {
        if source_id != "local_connector_approval" {
            return Err(ApiError::forbidden(
                "Memory Engine proxy source_id is not allowed",
            ));
        }
    }
    Ok(())
}

fn memory_engine_proxy_path_allowed(method: &Method, suffix: &str) -> bool {
    if method == Method::PUT && suffix == "admin/sources/local_connector_approval" {
        return true;
    }
    if method == Method::POST && suffix == "context/compose" {
        return true;
    }
    let parts = suffix.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0] != "threads" || !is_approval_memory_thread_id(parts[1]) {
        return false;
    }
    match (method, parts.as_slice()) {
        (&Method::PUT, ["threads", _thread_id]) => true,
        (&Method::GET, ["threads", _thread_id]) => true,
        (&Method::PUT, ["threads", _thread_id, "records", "batch-sync"]) => true,
        (&Method::GET, ["threads", _thread_id, "records"]) => true,
        (&Method::GET, ["threads", _thread_id, "records", "count"]) => true,
        (&Method::GET, ["threads", _thread_id, "compact-turns"]) => true,
        (&Method::GET, ["threads", _thread_id, "turns", _turn_id, "process-records"]) => true,
        (&Method::POST, ["threads", _thread_id, "active-summary", "run"]) => true,
        (&Method::GET, ["threads", _thread_id, "active-summary", "status"]) => true,
        (&Method::POST, ["threads", _thread_id, "summaries", "run"]) => true,
        (&Method::GET, ["threads", _thread_id, "summaries"]) => true,
        _ => false,
    }
}

fn is_approval_memory_thread_id(thread_id: &str) -> bool {
    thread_id.starts_with("local_connector_command_approval:")
        || thread_id.starts_with("local_connector_command_approval%3A")
        || thread_id.starts_with("local_connector_command_approval%3a")
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let item_key = parts.next()?.trim();
        let item_value = parts.next().unwrap_or_default().trim();
        (item_key == key && !item_value.is_empty()).then(|| item_value.to_string())
    })
}

fn json_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_relay_path(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn relay_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(key, value)| {
            let key = key.as_str().to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "authorization"
                    | "cookie"
                    | "set-cookie"
                    | "x-local-connector-internal-secret"
                    | "x-local-connector-owner-user-id"
                    | "x-chatos-owner-user-id"
            ) {
                return None;
            }
            value.to_str().ok().map(|value| (key, value.to_string()))
        })
        .collect()
}

fn relay_body(body: &[u8]) -> Value {
    if body.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice::<Value>(body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
}

fn relay_error_to_api_error(error: RelayError) -> ApiError {
    match error {
        RelayError::Offline => ApiError::service_unavailable(error.message()),
        RelayError::Timeout => ApiError::gateway_timeout(error.message()),
        RelayError::RequestEncode(_) | RelayError::ResponseChannelClosed => {
            ApiError::bad_gateway(error.message())
        }
    }
}

fn relay_response_to_http(response: RelayResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, Json(response.body)).into_response()
}

fn required_text(value: Option<String>, field: &str) -> Result<String, ApiError> {
    normalize_optional_text(value)
        .ok_or_else(|| ApiError::bad_request(format!("{field} is required and cannot be empty")))
}
