// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use chatos_sandbox_contract::{
    merge_codex_permission_profile_document_layers, parse_managed_requirements_toml,
    CodexPermissionProfileDocument,
};

use crate::controlled_network::{
    allowed_hosts_from_managed_requirements, ControlledNetworkPolicyEnvelope,
    ControlledNetworkPolicyRequest,
};
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
    controlled_network: Option<ControlledNetworkPolicyRequest>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionCreateRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    controlled_network: Option<ControlledNetworkPolicyRequest>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalInputRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    data: Option<String>,
    command: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalCloseRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalWsRelayQuery {
    workspace_id: Option<String>,
    terminal_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Serialize)]
pub(super) struct ControlledNetworkReadinessResponse {
    available: bool,
    state: &'static str,
    permission_profile: Option<String>,
    allowed_host_count: usize,
}

struct ControlledNetworkPolicySource {
    windows_user_sid: String,
    permission_profile: String,
    allowed_hosts: Vec<String>,
}

struct ControlledNetworkPolicyResolution {
    state: &'static str,
    source: Option<ControlledNetworkPolicySource>,
}

pub(super) async fn controlled_network_readiness(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
) -> Result<Json<ControlledNetworkReadinessResponse>, ApiError> {
    let resolution = resolve_controlled_network_policy_source(
        &state,
        &user,
        device_id.as_str(),
        &ControlledNetworkPolicyRequest::default(),
    )
    .await?;
    let response = match resolution.source {
        Some(source) => ControlledNetworkReadinessResponse {
            available: true,
            state: "ready",
            permission_profile: Some(source.permission_profile),
            allowed_host_count: source.allowed_hosts.len(),
        },
        None => ControlledNetworkReadinessResponse {
            available: false,
            state: resolution.state,
            permission_profile: None,
            allowed_host_count: 0,
        },
    };
    Ok(Json(response))
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
    let network_policy = issue_controlled_network_policy(
        &state,
        &user,
        device_id.as_str(),
        workspace_id.as_str(),
        req.controlled_network,
    )
    .await?;

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
            "network_policy": network_policy,
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

pub(super) async fn terminal_session_create_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalSessionCreateRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let network_policy = issue_controlled_network_policy(
        &state,
        &user,
        device_id.as_str(),
        workspace_id.as_str(),
        req.controlled_network,
    )
    .await?;
    if state.relay.new_terminal_sessions_paused().await {
        return Err(ApiError::too_many_requests(
            "Local Connector is temporarily pausing new terminal sessions",
        ));
    }

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
            "network_policy": network_policy,
        }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn issue_controlled_network_policy(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    workspace_id: &str,
    request: Option<ControlledNetworkPolicyRequest>,
) -> Result<Option<ControlledNetworkPolicyEnvelope>, ApiError> {
    let Some(request) = request else {
        return Ok(None);
    };
    let Some(signer) = state.controlled_network_signer.as_ref() else {
        return Ok(None);
    };
    let resolution =
        resolve_controlled_network_policy_source(state, user, device_id, &request).await?;
    let Some(source) = resolution.source else {
        return Ok(None);
    };
    signer
        .issue(
            user.effective_owner_user_id(),
            device_id,
            workspace_id,
            source.windows_user_sid.as_str(),
            source.allowed_hosts,
            vec![80, 443],
            Utc::now(),
        )
        .map(Some)
        .map_err(ApiError::bad_request)
}

async fn resolve_controlled_network_policy_source(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    request: &ControlledNetworkPolicyRequest,
) -> Result<ControlledNetworkPolicyResolution, ApiError> {
    if state.controlled_network_signer.is_none() {
        return Ok(ControlledNetworkPolicyResolution {
            state: "signer_not_configured",
            source: None,
        });
    }
    let device = state
        .store
        .get_device(device_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector device not found"))?;
    if device.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector device does not belong to current user",
        ));
    }
    let Some(windows_user_sid) = device.windows_user_sid.as_deref() else {
        return Ok(ControlledNetworkPolicyResolution {
            state: "windows_sid_not_registered",
            source: None,
        });
    };
    let layers = state
        .store
        .applicable_managed_requirements_layers(user.effective_owner_user_id(), user.role.as_str())
        .await
        .map_err(ApiError::internal)?;
    let mut requirements = layers
        .into_iter()
        .map(|layer| layer.policy.requirements_toml)
        .collect::<Vec<_>>();
    if requirements.is_empty() {
        if let Some(fallback) = state
            .managed_requirements_signer
            .as_ref()
            .and_then(|value| value.fallback_requirements_toml())
        {
            requirements.push(fallback.to_string());
        }
    }
    if requirements.is_empty() {
        return Ok(ControlledNetworkPolicyResolution {
            state: "managed_policy_not_configured",
            source: None,
        });
    }
    let mut document = CodexPermissionProfileDocument::default();
    for requirements_toml in requirements {
        let layer = match parse_managed_requirements_toml(requirements_toml.as_str()) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(device_id, error = %error, "managed Controlled network policy is invalid");
                return Ok(ControlledNetworkPolicyResolution {
                    state: "managed_policy_invalid",
                    source: None,
                });
            }
        };
        document = merge_codex_permission_profile_document_layers(document, layer);
    }
    if let Err(error) = document.configuration.validate() {
        tracing::warn!(device_id, error = %error, "merged Controlled network policy is invalid");
        return Ok(ControlledNetworkPolicyResolution {
            state: "managed_policy_invalid",
            source: None,
        });
    }
    let permission_profile = request
        .permission_profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(document.default_permissions.as_deref())
        .map(str::to_string);
    let allowed_hosts = match allowed_hosts_from_managed_requirements(&document, request) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return Ok(ControlledNetworkPolicyResolution {
                state: "managed_allowlist_not_configured",
                source: None,
            })
        }
        Err(error) if request.permission_profile.is_none() => {
            tracing::warn!(
                device_id,
                error = %error,
                "managed network policy cannot be compiled for Windows Controlled mode"
            );
            return Ok(ControlledNetworkPolicyResolution {
                state: "managed_policy_not_compilable",
                source: None,
            });
        }
        Err(error) => return Err(ApiError::bad_request(error)),
    };
    Ok(ControlledNetworkPolicyResolution {
        state: "ready",
        source: Some(ControlledNetworkPolicySource {
            windows_user_sid: windows_user_sid.to_string(),
            permission_profile: permission_profile.expect("allowed hosts require a profile"),
            allowed_hosts,
        }),
    })
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    send_relay(&state, request).await?;
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn terminal_close_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalCloseRelayRequest>,
) -> Result<Json<Value>, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "terminal_close".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/close".to_string(),
        headers: BTreeMap::new(),
        body: json!({ "terminal_session_id": terminal_session_id }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    send_relay(&state, request).await?;
    Ok(Json(json!({ "success": true, "closed": true })))
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
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
                drop_terminal_subscription(
                    &state,
                    terminal_session_id.as_str(),
                    subscription_id.as_str(),
                )
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
                drop_terminal_subscription(
                    &state,
                    terminal_session_id.as_str(),
                    subscription_id.as_str(),
                )
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
            drop_terminal_subscription(
                &state,
                terminal_session_id.as_str(),
                subscription_id.as_str(),
            )
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
            drop_terminal_subscription(
                &state,
                terminal_session_id.as_str(),
                subscription_id.as_str(),
            )
            .await;
            return;
        }
    }

    let (mut sender, mut receiver) = socket.split();
    let relay = state.relay.clone();
    let subscriber_terminal_session_id = terminal_session_id.clone();
    let subscriber_id = subscription_id.clone();
    let refresh_interval = state.config.terminal_subscriber_refresh_interval;
    let mut event_task = tokio::spawn(async move {
        let mut refresh = tokio::time::interval(refresh_interval);
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                event = events.recv() => match event {
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
                },
                _ = refresh.tick() => {
                    match relay
                        .refresh_terminal_subscription(
                            subscriber_terminal_session_id.as_str(),
                            subscriber_id.as_str(),
                        )
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(error) => {
                            let _ = sender
                                .send(Message::Text(
                                    json!({"type": "error", "error": error})
                                        .to_string()
                                        .into(),
                                ))
                                .await;
                            break;
                        }
                    }
                }
            }
        }
    });

    loop {
        let message = tokio::select! {
            _ = &mut event_task => break,
            message = receiver.next() => message,
        };
        let Some(message) = message else {
            break;
        };
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
    event_task.abort();
    drop_terminal_subscription(
        &state,
        terminal_session_id.as_str(),
        subscription_id.as_str(),
    )
    .await;
}

pub(super) async fn drop_terminal_subscription(
    state: &AppState,
    terminal_session_id: &str,
    subscription_id: &str,
) {
    if let Err(error) = state
        .relay
        .drop_terminal_subscription(terminal_session_id, subscription_id)
        .await
    {
        tracing::warn!(
            terminal_session_id,
            subscription_id,
            error = error.as_str(),
            "drop Local Connector terminal subscriber lease failed"
        );
    }
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    send_relay(state, request).await.is_ok()
}

pub(super) fn terminal_event_to_ws_payload(message_type: &str, body: &Value) -> Option<Value> {
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
