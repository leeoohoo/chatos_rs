// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::{normalize_optional_text, CurrentUser};
use crate::relay::RelayRequest;
use crate::state::AppState;

use super::{
    dispatch_relay, relay_response_to_http, required_text, send_relay, validate_device_workspace,
    ApiError,
};

const DEFAULT_TERMINAL_EXEC_TIMEOUT_MS: u64 = 30_000;
const MAX_TERMINAL_EXEC_TIMEOUT_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Deserialize)]
pub(super) struct TerminalExecRelayRequest {
    workspace_id: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionCreateRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalInputRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    data: Option<String>,
    command: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalWsRelayQuery {
    workspace_id: Option<String>,
    terminal_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

pub(super) async fn terminal_exec_relay(
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

    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn terminal_session_create_relay(
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

    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn terminal_input_relay(
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

    send_relay(&state, request).await?;
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn terminal_ws_relay(
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
    let create_response =
        dispatch_relay(&state, create_request, state.config.relay_request_timeout).await;
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
    send_relay(state, request).await.is_ok()
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
