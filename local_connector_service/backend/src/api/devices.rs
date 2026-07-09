// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::models::{
    normalize_optional_text, CurrentUser, LocalConnectorDevice, LocalConnectorSession,
    DEVICE_STATUS_REVOKED,
};
use crate::state::AppState;

use super::{required_text, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct DeviceQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateDeviceRequest {
    display_name: Option<String>,
    public_key: Option<String>,
    client_version: Option<String>,
    os: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DeviceHeartbeatRequest {
    session_id: Option<String>,
}

pub(super) async fn list_devices(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<Vec<LocalConnectorDevice>>, ApiError> {
    let owner_user_id = resolve_owner_user_id(query.user_id, &user)?;
    state
        .store
        .list_devices(owner_user_id.as_str())
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateDeviceRequest>,
) -> Result<(StatusCode, Json<LocalConnectorDevice>), ApiError> {
    let device = LocalConnectorDevice::new(
        user.effective_owner_user_id().to_string(),
        required_text(req.display_name, "display_name")?,
        required_text(req.public_key, "public_key")?,
        normalize_optional_text(req.client_version),
        normalize_optional_text(req.os),
    );
    state
        .store
        .create_device(&device)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(device)))
}

pub(super) async fn get_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    load_owned_device(&state, &user, id.as_str(), false)
        .await
        .map(Json)
}

pub(super) async fn heartbeat_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<DeviceHeartbeatRequest>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    if let Some(session_id) = normalize_optional_text(req.session_id) {
        state
            .store
            .heartbeat_session(session_id.as_str(), device.id.as_str())
            .await
            .map_err(ApiError::internal)?;
    } else {
        state
            .store
            .mark_device_online(device.id.as_str())
            .await
            .map_err(ApiError::internal)?;
    }
    load_owned_device(&state, &user, id.as_str(), true)
        .await
        .map(Json)
}

pub(super) async fn revoke_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_device(&state, &user, id.as_str(), false).await?;
    state
        .store
        .revoke_device(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn disconnect_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    state
        .store
        .mark_device_offline(device.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    load_owned_device(&state, &user, id.as_str(), true)
        .await
        .map(Json)
}

pub(super) async fn connect_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    Ok(ws
        .on_upgrade(move |socket| handle_connector_socket(state, owner_user_id, device.id, socket))
        .into_response())
}

pub(super) async fn load_owned_device(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    reject_revoked: bool,
) -> Result<LocalConnectorDevice, ApiError> {
    let device = state
        .store
        .get_device(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector device not found"))?;
    if device.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector device does not belong to current user",
        ));
    }
    if reject_revoked && device.status == DEVICE_STATUS_REVOKED {
        return Err(ApiError::bad_request(
            "Local Connector device has been revoked",
        ));
    }
    Ok(device)
}

async fn handle_connector_socket(
    state: AppState,
    owner_user_id: String,
    device_id: String,
    socket: WebSocket,
) {
    let session = LocalConnectorSession::new(owner_user_id, device_id.clone());
    if let Err(err) = state.store.open_session(&session).await {
        send_startup_error(socket, err).await;
        return;
    }
    let _ = state.store.mark_device_online(device_id.as_str()).await;

    let (mut sender, mut receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(256);
    state
        .relay
        .register_session(
            device_id.clone(),
            session.owner_user_id.clone(),
            session.id.clone(),
            outbound_tx.clone(),
        )
        .await;

    let send_task = tokio::spawn(async move {
        while let Some(text) = outbound_rx.recv().await {
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let _ = send_outbound_json(
        &outbound_tx,
        json!({
            "type": "connected",
            "device_id": device_id.as_str(),
            "session_id": session.id.as_str(),
            "connection_id": session.connection_id.as_str(),
            "timestamp": crate::models::now_rfc3339(),
        }),
    )
    .await;

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if is_heartbeat_message(text.as_str()) {
                    let _ = state
                        .store
                        .heartbeat_session(session.id.as_str(), device_id.as_str())
                        .await;
                    if !send_outbound_json(
                        &outbound_tx,
                        json!({
                            "type": "pong",
                            "session_id": session.id.as_str(),
                            "timestamp": crate::models::now_rfc3339(),
                        }),
                    )
                    .await
                    {
                        break;
                    }
                } else if let Ok(consumed) = state.relay.handle_inbound_text(text.as_str()).await {
                    if !consumed {
                        let _ = send_outbound_json(
                            &outbound_tx,
                            json!({
                                "type": "ack",
                                "message": "control channel message accepted",
                                "timestamp": crate::models::now_rfc3339(),
                            }),
                        )
                        .await;
                    }
                } else {
                    let _ = send_outbound_json(
                        &outbound_tx,
                        json!({
                            "type": "error",
                            "code": "invalid_relay_response",
                            "message": "invalid relay response message",
                            "timestamp": crate::models::now_rfc3339(),
                        }),
                    )
                    .await;
                }
            }
            Ok(Message::Ping(bytes)) => {
                let payload_len = bytes.len();
                let _ = state
                    .store
                    .heartbeat_session(session.id.as_str(), device_id.as_str())
                    .await;
                let _ = send_outbound_json(
                    &outbound_tx,
                    json!({
                        "type": "pong",
                        "payload_bytes": payload_len,
                        "session_id": session.id.as_str(),
                        "timestamp": crate::models::now_rfc3339(),
                    }),
                )
                .await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    let _ = state
        .store
        .close_session(session.id.as_str(), device_id.as_str())
        .await;
    state
        .relay
        .unregister_session(device_id.as_str(), session.id.as_str())
        .await;
    send_task.abort();
}

async fn send_socket_json(socket: &mut WebSocket, value: Value) -> bool {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .is_ok()
}

async fn send_startup_error(mut socket: WebSocket, message: String) {
    let _ = send_socket_json(
        &mut socket,
        json!({
            "type": "error",
            "code": "session_open_failed",
            "message": message,
        }),
    )
    .await;
}

async fn send_outbound_json(sender: &mpsc::Sender<String>, value: Value) -> bool {
    sender.send(value.to_string()).await.is_ok()
}

fn resolve_owner_user_id(
    requested_user_id: Option<String>,
    user: &CurrentUser,
) -> Result<String, ApiError> {
    let owner_user_id = user.effective_owner_user_id();
    if let Some(requested) = normalize_optional_text(requested_user_id) {
        if requested != owner_user_id {
            return Err(ApiError::forbidden("user_id 与登录用户不一致"));
        }
    }
    Ok(owner_user_id.to_string())
}

fn is_heartbeat_message(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("ping") || trimmed.eq_ignore_ascii_case("heartbeat") {
        return true;
    }
    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|value| {
            value.get("type").and_then(Value::as_str).map(|item| {
                item.eq_ignore_ascii_case("ping") || item.eq_ignore_ascii_case("heartbeat")
            })
        })
        .unwrap_or(false)
}
