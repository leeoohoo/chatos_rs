// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::models::{
    normalize_optional_text, CurrentUser, LocalConnectorDevice, LocalConnectorSession,
    DEVICE_STATUS_REVOKED,
};
use crate::state::AppState;
use crate::store::SessionAcquireError;

use super::plugin_management_mcps::{
    is_mcp_manifest_status_message, mark_device_mcps_offline, sync_socket_mcp_statuses,
};
use super::plugin_management_skills::{
    is_skill_inventory_status_message, mark_device_skills_offline, sync_socket_skill_inventory,
};
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
    let public_key = required_text(req.public_key, "public_key")?;
    device_public_key_bytes(public_key.as_str())?;
    let device = LocalConnectorDevice::new(
        user.effective_owner_user_id().to_string(),
        required_text(req.display_name, "display_name")?,
        public_key,
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
        let renewed = state
            .store
            .heartbeat_session(
                user.effective_owner_user_id(),
                session_id.as_str(),
                device.id.as_str(),
                state.config.active_session_lease_ttl,
            )
            .await
            .map_err(ApiError::internal)?;
        if !renewed {
            return Err(ApiError::conflict(
                "connector_session_lease_lost",
                "Local Connector active session lease is missing or expired",
            ));
        }
    } else {
        return Err(ApiError::bad_request(
            "session_id is required to renew the Local Connector active session lease",
        ));
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
    release_device_session(&state, user.effective_owner_user_id(), id.as_str()).await?;
    state
        .store
        .revoke_device(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    if let Err(err) =
        mark_device_mcps_offline(&state, user.effective_owner_user_id(), id.as_str()).await
    {
        tracing::warn!(
            device_id = id,
            error = err,
            "mark revoked device MCPs offline failed"
        );
    }
    if let Err(err) =
        mark_device_skills_offline(&state, user.effective_owner_user_id(), id.as_str()).await
    {
        tracing::warn!(
            device_id = id,
            error = err,
            "mark revoked device Skills offline failed"
        );
    }
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn disconnect_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    release_device_session(&state, user.effective_owner_user_id(), device.id.as_str()).await?;
    state
        .store
        .mark_device_offline(device.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    if let Err(err) =
        mark_device_mcps_offline(&state, user.effective_owner_user_id(), device.id.as_str()).await
    {
        tracing::warn!(
            device_id = device.id,
            error = err,
            "mark disconnected device MCPs offline failed"
        );
    }
    if let Err(err) =
        mark_device_skills_offline(&state, user.effective_owner_user_id(), device.id.as_str()).await
    {
        tracing::warn!(
            device_id = device.id,
            error = err,
            "mark disconnected device Skills offline failed"
        );
    }
    load_owned_device(&state, &user, id.as_str(), true)
        .await
        .map(Json)
}

pub(super) async fn connect_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    verify_device_connect_signature(&state, &headers, &device).await?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    let owner_devices = state
        .store
        .list_devices(owner_user_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let session = LocalConnectorSession::new(
        owner_user_id.clone(),
        device.id.clone(),
        state.config.active_session_lease_ttl,
    );
    state
        .store
        .open_session(&session)
        .await
        .map_err(|err| match err {
            SessionAcquireError::AlreadyActive => ApiError::conflict(
                "connector_already_active",
                "another Local Connector client is already active for this user",
            ),
            SessionAcquireError::Store(err) => ApiError::internal(err),
        })?;
    state
        .store
        .mark_device_online(device.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    for previous_device in owner_devices
        .into_iter()
        .filter(|item| item.id != device.id)
    {
        if let Err(err) =
            mark_device_mcps_offline(&state, owner_user_id.as_str(), previous_device.id.as_str())
                .await
        {
            tracing::warn!(
                device_id = previous_device.id,
                error = err,
                "mark previous device MCPs offline after lease acquisition failed"
            );
        }
        if let Err(err) =
            mark_device_skills_offline(&state, owner_user_id.as_str(), previous_device.id.as_str())
                .await
        {
            tracing::warn!(
                device_id = previous_device.id,
                error = err,
                "mark previous device Skills offline after lease acquisition failed"
            );
        }
    }
    Ok(ws
        .on_upgrade(move |socket| handle_connector_socket(state, session, socket))
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
    session: LocalConnectorSession,
    socket: WebSocket,
) {
    let device_id = session.device_id.clone();

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
                    let renewed = state
                        .store
                        .heartbeat_session(
                            session.owner_user_id.as_str(),
                            session.id.as_str(),
                            device_id.as_str(),
                            state.config.active_session_lease_ttl,
                        )
                        .await
                        .unwrap_or(false);
                    if !renewed {
                        let _ = send_outbound_json(
                            &outbound_tx,
                            json!({
                                "type": "error",
                                "code": "connector_session_lease_lost",
                                "message": "Local Connector active session lease is missing or expired",
                                "timestamp": crate::models::now_rfc3339(),
                            }),
                        )
                        .await;
                        break;
                    }
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
                } else if is_mcp_manifest_status_message(text.as_str()) {
                    match sync_socket_mcp_statuses(
                        &state,
                        session.owner_user_id.as_str(),
                        device_id.as_str(),
                        text.as_str(),
                    )
                    .await
                    {
                        Ok(count) => {
                            if !send_outbound_json(
                                &outbound_tx,
                                json!({
                                    "type": "mcp_manifest_status_ack",
                                    "count": count,
                                    "timestamp": crate::models::now_rfc3339(),
                                }),
                            )
                            .await
                            {
                                break;
                            }
                        }
                        Err(err) => {
                            let _ = send_outbound_json(
                                &outbound_tx,
                                json!({
                                    "type": "error",
                                    "code": "mcp_manifest_status_rejected",
                                    "message": err,
                                    "timestamp": crate::models::now_rfc3339(),
                                }),
                            )
                            .await;
                        }
                    }
                } else if is_skill_inventory_status_message(text.as_str()) {
                    match sync_socket_skill_inventory(
                        &state,
                        session.owner_user_id.as_str(),
                        device_id.as_str(),
                        text.as_str(),
                    )
                    .await
                    {
                        Ok(count) => {
                            if !send_outbound_json(
                                &outbound_tx,
                                json!({
                                    "type": "skill_inventory_status_ack",
                                    "count": count,
                                    "timestamp": crate::models::now_rfc3339(),
                                }),
                            )
                            .await
                            {
                                break;
                            }
                        }
                        Err(err) => {
                            let _ = send_outbound_json(
                                &outbound_tx,
                                json!({
                                    "type": "error",
                                    "code": "skill_inventory_status_rejected",
                                    "message": err,
                                    "timestamp": crate::models::now_rfc3339(),
                                }),
                            )
                            .await;
                        }
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
                let renewed = state
                    .store
                    .heartbeat_session(
                        session.owner_user_id.as_str(),
                        session.id.as_str(),
                        device_id.as_str(),
                        state.config.active_session_lease_ttl,
                    )
                    .await
                    .unwrap_or(false);
                if !renewed {
                    break;
                }
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
        .close_session(
            session.owner_user_id.as_str(),
            session.id.as_str(),
            device_id.as_str(),
        )
        .await;
    state
        .relay
        .unregister_session(device_id.as_str(), session.id.as_str())
        .await;
    if let Err(err) =
        mark_device_mcps_offline(&state, session.owner_user_id.as_str(), device_id.as_str()).await
    {
        tracing::warn!(
            device_id,
            error = err,
            "mark socket device MCPs offline failed"
        );
    }
    if let Err(err) =
        mark_device_skills_offline(&state, session.owner_user_id.as_str(), device_id.as_str()).await
    {
        tracing::warn!(
            device_id,
            error = err,
            "mark socket device Skills offline failed"
        );
    }
    send_task.abort();
}

async fn send_outbound_json(sender: &mpsc::Sender<String>, value: Value) -> bool {
    sender.send(value.to_string()).await.is_ok()
}

async fn release_device_session(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
) -> Result<(), ApiError> {
    let active_session = state
        .store
        .active_session(owner_user_id)
        .await
        .map_err(ApiError::internal)?;
    if active_session
        .as_ref()
        .is_some_and(|item| item.device_id == device_id)
    {
        let session = active_session.expect("active session checked above");
        state
            .store
            .close_session(owner_user_id, session.id.as_str(), device_id)
            .await
            .map_err(ApiError::internal)?;
        state
            .relay
            .unregister_session(device_id, session.id.as_str())
            .await;
    } else {
        state
            .store
            .close_device_session(owner_user_id, device_id)
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(())
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

async fn verify_device_connect_signature(
    state: &AppState,
    headers: &HeaderMap,
    device: &LocalConnectorDevice,
) -> Result<(), ApiError> {
    if !state.config.require_device_connect_signature {
        return Ok(());
    }
    let public_key = device_public_key_bytes(device.public_key.as_str())?;
    let algorithm = required_header(headers, "x-local-connector-device-signature-alg")?;
    if algorithm != "ed25519" {
        return Err(ApiError::unauthorized(
            "Local Connector device signature algorithm is not supported",
        ));
    }
    let header_device_id = required_header(headers, "x-local-connector-device-id")?;
    if header_device_id != device.id {
        return Err(ApiError::unauthorized(
            "Local Connector device signature device id does not match",
        ));
    }
    let timestamp = required_header(headers, "x-local-connector-device-timestamp")?
        .parse::<i64>()
        .map_err(|_| ApiError::unauthorized("Local Connector device timestamp is invalid"))?;
    let now = Utc::now().timestamp();
    let max_skew = state
        .config
        .device_connect_signature_max_skew
        .as_secs()
        .try_into()
        .unwrap_or(300_i64);
    if now.saturating_sub(timestamp).abs() > max_skew {
        return Err(ApiError::unauthorized(
            "Local Connector device signature timestamp is outside the allowed window",
        ));
    }
    let nonce = required_header(headers, "x-local-connector-device-nonce")?;
    if nonce.len() < 16 || nonce.len() > 128 {
        return Err(ApiError::unauthorized(
            "Local Connector device signature nonce is invalid",
        ));
    }
    if !state
        .consume_device_connect_nonce(device.id.as_str(), nonce.as_str(), now)
        .await
    {
        return Err(ApiError::unauthorized(
            "Local Connector device signature nonce was already used",
        ));
    }
    let signature = required_header(headers, "x-local-connector-device-signature")?;
    let signature = URL_SAFE_NO_PAD.decode(signature.as_bytes()).map_err(|_| {
        ApiError::unauthorized("Local Connector device signature encoding is invalid")
    })?;
    let path = format!("/api/local-connectors/devices/{}/connect", device.id);
    let payload =
        device_signature_payload(device.id.as_str(), timestamp, nonce.as_str(), path.as_str());
    UnparsedPublicKey::new(&ED25519, public_key.as_slice())
        .verify(payload.as_bytes(), signature.as_slice())
        .map_err(|_| ApiError::unauthorized("Local Connector device signature is invalid"))
}

fn device_public_key_bytes(value: &str) -> Result<Vec<u8>, ApiError> {
    let encoded = value.trim().strip_prefix("ed25519:").ok_or_else(|| {
        ApiError::unauthorized(
            "Local Connector device key is not an ed25519 public key; re-register the device",
        )
    })?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).map_err(|_| {
        ApiError::unauthorized("Local Connector device public key encoding is invalid")
    })?;
    if bytes.len() != 32 {
        return Err(ApiError::unauthorized(
            "Local Connector device public key length is invalid",
        ));
    }
    Ok(bytes)
}

fn required_header(headers: &HeaderMap, name: &'static str) -> Result<String, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::unauthorized(format!("{name} is required")))
}

fn device_signature_payload(device_id: &str, timestamp: i64, nonce: &str, path: &str) -> String {
    format!("v1\n{device_id}\n{timestamp}\n{nonce}\n{path}")
}
