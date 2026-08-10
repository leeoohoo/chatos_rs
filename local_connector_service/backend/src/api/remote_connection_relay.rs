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
    dispatch_relay, drop_terminal_subscription, relay_response_to_http, required_text, send_relay,
    terminal_event_to_ws_payload, validate_device_workspace, ApiError,
};

#[derive(Debug, Deserialize)]
pub(super) struct RemoteConnectionTestRelayRequest {
    workspace_id: Option<String>,
    connection: Option<Value>,
    verification_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteConnectionCommandRelayRequest {
    workspace_id: Option<String>,
    connection: Option<Value>,
    command: Option<String>,
    timeout_ms: Option<u64>,
    verification_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteTerminalCloseRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteTerminalWsRelayQuery {
    workspace_id: Option<String>,
    terminal_id: Option<String>,
}

pub(super) async fn remote_connection_test_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<RemoteConnectionTestRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let connection = req
        .connection
        .filter(Value::is_object)
        .ok_or_else(|| ApiError::bad_request("connection is required"))?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "remote_connection_test_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/remote-connections/test".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "connection": connection,
            "verification_code": req.verification_code,
        }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let relay_timeout = state
        .config
        .relay_request_timeout
        .max(Duration::from_secs(20));
    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn remote_connection_command_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<RemoteConnectionCommandRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let connection = req
        .connection
        .filter(Value::is_object)
        .ok_or_else(|| ApiError::bad_request("connection is required"))?;
    let command = required_text(req.command, "command")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let timeout_ms = req.timeout_ms.unwrap_or(30_000).clamp(1_000, 600_000);
    let relay_timeout = state
        .config
        .relay_request_timeout
        .max(Duration::from_millis(timeout_ms.saturating_add(5_000)));
    let request = RelayRequest {
        message_type: "remote_connection_command_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/remote-connections/command".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "connection": connection,
            "command": command,
            "timeout_ms": timeout_ms,
            "verification_code": req.verification_code,
        }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn remote_terminal_close_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<RemoteTerminalCloseRelayRequest>,
) -> Result<Json<Value>, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    send_relay(
        &state,
        RelayRequest {
            message_type: "remote_terminal_close".to_string(),
            request_id: Uuid::new_v4().to_string(),
            owner_user_id: user.effective_owner_user_id().to_string(),
            device_id,
            workspace_id,
            method: "POST".to_string(),
            path: "/remote-connections/terminal/close".to_string(),
            headers: BTreeMap::new(),
            body: json!({ "terminal_session_id": terminal_session_id }),
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        },
    )
    .await?;
    Ok(Json(json!({ "success": true, "disconnected": true })))
}

pub(super) async fn remote_sftp_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(mut body): Json<Value>,
) -> Result<Response, ApiError> {
    let workspace_id = body
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::bad_request("workspace_id is required"))?;
    if let Value::Object(ref mut map) = body {
        map.remove("workspace_id");
    }
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let request = RelayRequest {
        message_type: "remote_sftp_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/remote-connections/sftp".to_string(),
        headers: BTreeMap::new(),
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(
        &state,
        request,
        state
            .config
            .relay_request_timeout
            .max(Duration::from_secs(610)),
    )
    .await?;
    Ok(relay_response_to_http(response))
}

pub(super) async fn remote_terminal_ws_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<RemoteTerminalWsRelayQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(query.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let terminal_session_id =
        normalize_optional_text(query.terminal_id).unwrap_or_else(|| Uuid::new_v4().to_string());
    let owner_user_id = user.effective_owner_user_id().to_string();
    Ok(ws
        .on_upgrade(move |socket| {
            handle_remote_terminal_socket(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                terminal_session_id,
                socket,
            )
        })
        .into_response())
}

async fn handle_remote_terminal_socket(
    state: AppState,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    terminal_session_id: String,
    mut socket: WebSocket,
) {
    let setup = match tokio::time::timeout(Duration::from_secs(5), socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<Value>(text.as_str()).ok(),
        _ => None,
    };
    let Some(connection) = setup
        .as_ref()
        .filter(|value| value.get("type").and_then(Value::as_str) == Some("setup"))
        .and_then(|value| value.get("connection"))
        .filter(|value| value.is_object())
        .cloned()
    else {
        let _ = socket
            .send(Message::Text(
                json!({"type": "error", "error": "remote terminal setup is required"})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    };

    let mut verification_code = None;
    let mut cols = 80u16;
    let mut rows = 24u16;
    let mut queued_message = None;
    if let Ok(Some(message)) = tokio::time::timeout(Duration::from_millis(250), socket.recv()).await
    {
        match message {
            Ok(Message::Text(text)) => {
                let value = serde_json::from_str::<Value>(text.as_str()).ok();
                match value
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str)
                {
                    Some("verification") => {
                        verification_code = value
                            .as_ref()
                            .and_then(|value| value.get("code"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned);
                    }
                    Some("resize") => {
                        cols = value
                            .as_ref()
                            .and_then(|value| value.get("cols"))
                            .and_then(Value::as_u64)
                            .unwrap_or(80)
                            .clamp(1, u16::MAX as u64) as u16;
                        rows = value
                            .as_ref()
                            .and_then(|value| value.get("rows"))
                            .and_then(Value::as_u64)
                            .unwrap_or(24)
                            .clamp(1, u16::MAX as u64) as u16;
                    }
                    _ => queued_message = Some(Message::Text(text)),
                }
            }
            Ok(message) => queued_message = Some(message),
            Err(_) => return,
        }
    }

    let subscription = match state
        .relay
        .subscribe_terminal_session(terminal_session_id.as_str())
        .await
    {
        Ok(subscription) => subscription,
        Err(error) => {
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": error}).to_string().into(),
                ))
                .await;
            return;
        }
    };
    let subscription_id = subscription.id;
    let mut events = subscription.events;
    let startup_response = loop {
        let create_request = RelayRequest {
            message_type: "remote_terminal_session_create_request".to_string(),
            request_id: Uuid::new_v4().to_string(),
            owner_user_id: owner_user_id.clone(),
            device_id: device_id.clone(),
            workspace_id: workspace_id.clone(),
            method: "POST".to_string(),
            path: "/remote-connections/terminal/sessions".to_string(),
            headers: BTreeMap::new(),
            body: json!({
                "terminal_session_id": terminal_session_id,
                "connection": connection,
                "verification_code": verification_code,
                "cols": cols,
                "rows": rows,
            }),
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        };
        match dispatch_relay(
            &state,
            create_request,
            state
                .config
                .relay_request_timeout
                .max(Duration::from_secs(190)),
        )
        .await
        {
            Ok(response) if (200..300).contains(&response.status) => break response,
            Ok(response) => {
                let needs_verification = response.body.get("code").and_then(Value::as_str)
                    == Some("second_factor_required");
                let mut body = response.body;
                if let Value::Object(ref mut map) = body {
                    map.insert("type".to_string(), Value::String("error".to_string()));
                }
                if socket
                    .send(Message::Text(body.to_string().into()))
                    .await
                    .is_err()
                {
                    drop_terminal_subscription(
                        &state,
                        terminal_session_id.as_str(),
                        subscription_id.as_str(),
                    )
                    .await;
                    return;
                }
                if !needs_verification {
                    drop_terminal_subscription(
                        &state,
                        terminal_session_id.as_str(),
                        subscription_id.as_str(),
                    )
                    .await;
                    return;
                }
                let Some(code) =
                    wait_for_remote_terminal_verification(&mut socket, &mut cols, &mut rows).await
                else {
                    drop_terminal_subscription(
                        &state,
                        terminal_session_id.as_str(),
                        subscription_id.as_str(),
                    )
                    .await;
                    return;
                };
                verification_code = Some(code);
            }
            Err(error) => {
                let _ = socket
                    .send(Message::Text(
                        json!({"type": "error", "error": error.message()})
                            .to_string()
                            .into(),
                    ))
                    .await;
                drop_terminal_subscription(
                    &state,
                    terminal_session_id.as_str(),
                    subscription_id.as_str(),
                )
                .await;
                return;
            }
        }
    };

    let snapshot = startup_response
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
        drop_terminal_subscription(
            &state,
            terminal_session_id.as_str(),
            subscription_id.as_str(),
        )
        .await;
        return;
    }
    let busy = startup_response
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
        drop_terminal_subscription(
            &state,
            terminal_session_id.as_str(),
            subscription_id.as_str(),
        )
        .await;
        return;
    }

    let (mut sender, mut receiver) = socket.split();
    let relay = state.relay.clone();
    let refresh_interval = state.config.terminal_subscriber_refresh_interval;
    let subscriber_terminal_id = terminal_session_id.clone();
    let subscriber_id = subscription_id.clone();
    let mut event_task = tokio::spawn(async move {
        let mut refresh = tokio::time::interval(refresh_interval);
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(event) => {
                        let Some(payload) = terminal_event_to_ws_payload(
                            event.message_type.as_str(),
                            &event.body,
                        ) else {
                            continue;
                        };
                        if sender.send(Message::Text(payload.to_string().into())).await.is_err()
                            || event.message_type == "terminal_exit"
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = refresh.tick() => {
                    if !matches!(
                        relay.refresh_terminal_subscription(
                            subscriber_terminal_id.as_str(),
                            subscriber_id.as_str(),
                        ).await,
                        Ok(true)
                    ) {
                        break;
                    }
                }
            }
        }
    });

    if let Some(message) = queued_message {
        let _ = handle_remote_terminal_ws_message(
            &state,
            owner_user_id.as_str(),
            device_id.as_str(),
            workspace_id.as_str(),
            terminal_session_id.as_str(),
            message,
        )
        .await;
    }
    loop {
        let message = tokio::select! {
            _ = &mut event_task => break,
            message = receiver.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        match message {
            Ok(message) => {
                if !handle_remote_terminal_ws_message(
                    &state,
                    owner_user_id.as_str(),
                    device_id.as_str(),
                    workspace_id.as_str(),
                    terminal_session_id.as_str(),
                    message,
                )
                .await
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    event_task.abort();
    drop_terminal_subscription(
        &state,
        terminal_session_id.as_str(),
        subscription_id.as_str(),
    )
    .await;
}

async fn wait_for_remote_terminal_verification(
    socket: &mut WebSocket,
    cols: &mut u16,
    rows: &mut u16,
) -> Option<String> {
    let deadline = tokio::time::sleep(Duration::from_secs(300));
    tokio::pin!(deadline);
    loop {
        let message = tokio::select! {
            _ = &mut deadline => return None,
            message = socket.recv() => message,
        };
        match message {
            Some(Ok(Message::Text(text))) => {
                let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                match value.get("type").and_then(Value::as_str) {
                    Some("verification") => {
                        if let Some(code) = value
                            .get("code")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            return Some(code.to_string());
                        }
                    }
                    Some("resize") => {
                        *cols = value
                            .get("cols")
                            .and_then(Value::as_u64)
                            .unwrap_or(*cols as u64)
                            .clamp(1, u16::MAX as u64) as u16;
                        *rows = value
                            .get("rows")
                            .and_then(Value::as_u64)
                            .unwrap_or(*rows as u64)
                            .clamp(1, u16::MAX as u64) as u16;
                    }
                    Some("ping") => {
                        match socket
                            .send(Message::Text(json!({"type": "pong"}).to_string().into()))
                            .await
                        {
                            Ok(()) => {}
                            Err(_) => return None,
                        }
                    }
                    _ => {}
                }
            }
            Some(Ok(Message::Ping(bytes))) => {
                if socket.send(Message::Pong(bytes)).await.is_err() {
                    return None;
                }
            }
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
            Some(Ok(_)) => {}
        }
    }
}

async fn handle_remote_terminal_ws_message(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    message: Message,
) -> bool {
    match message {
        Message::Text(text) => {
            let value = serde_json::from_str::<Value>(text.as_str())
                .unwrap_or_else(|_| json!({ "type": "input", "data": text.as_str() }));
            match value.get("type").and_then(Value::as_str) {
                Some("input") => send_remote_terminal_control(
                    state,
                    owner_user_id,
                    device_id,
                    workspace_id,
                    terminal_session_id,
                    "remote_terminal_input",
                    json!({ "data": value.get("data").and_then(Value::as_str).unwrap_or_default() }),
                )
                .await,
                Some("resize") => send_remote_terminal_control(
                    state,
                    owner_user_id,
                    device_id,
                    workspace_id,
                    terminal_session_id,
                    "remote_terminal_resize",
                    json!({
                        "cols": value.get("cols").and_then(Value::as_u64).unwrap_or(80),
                        "rows": value.get("rows").and_then(Value::as_u64).unwrap_or(24),
                    }),
                )
                .await,
                Some("command") => {
                    let mut command = value
                        .get("command")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if !command.ends_with('\n') {
                        command.push('\n');
                    }
                    send_remote_terminal_control(
                        state,
                        owner_user_id,
                        device_id,
                        workspace_id,
                        terminal_session_id,
                        "remote_terminal_input",
                        json!({ "data": command }),
                    )
                    .await
                }
                Some("verification") | Some("ping") => true,
                _ => true,
            }
        }
        Message::Binary(bytes) => {
            let data = String::from_utf8_lossy(&bytes).into_owned();
            send_remote_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                terminal_session_id,
                "remote_terminal_input",
                json!({ "data": data }),
            )
            .await
        }
        Message::Ping(_) | Message::Pong(_) => true,
        Message::Close(_) => false,
    }
}

async fn send_remote_terminal_control(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    message_type: &str,
    mut body: Value,
) -> bool {
    if let Value::Object(ref mut map) = body {
        map.insert(
            "terminal_session_id".to_string(),
            Value::String(terminal_session_id.to_string()),
        );
    }
    send_relay(
        state,
        RelayRequest {
            message_type: message_type.to_string(),
            request_id: Uuid::new_v4().to_string(),
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            workspace_id: workspace_id.to_string(),
            method: "POST".to_string(),
            path: format!("/remote-connections/terminal/{message_type}"),
            headers: BTreeMap::new(),
            body,
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        },
    )
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_object_connection_payloads() {
        let request = RemoteConnectionTestRelayRequest {
            workspace_id: Some("workspace-1".to_string()),
            connection: Some(json!("not-an-object")),
            verification_code: None,
        };

        assert!(!request.connection.is_some_and(|value| value.is_object()));
    }
}
