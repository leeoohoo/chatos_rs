// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::device_keys::sign_device_message;
use crate::history::CommandHistoryRecorder;
use crate::mcp::configs::refresh_enabled_local_mcp_checks;
use crate::mcp::manifest::mcp_status_message;
use crate::mcp::service::handle_mcp_request;
use crate::model_configs::handle_model_runtime_request;
use crate::relay::{relay_error_response, RelayRequest, MCP_RELAY_MESSAGE_TYPE};
use crate::sandbox::relay::handle_sandbox_request;
use crate::sandbox::types::LocalSandboxRuntime;
use crate::skills::{
    handle_skill_cancel, handle_skill_execute, handle_skill_prepare, skill_inventory_status_message,
};
use crate::terminal::exec::handle_terminal_exec_request;
use crate::terminal::relay::{
    handle_terminal_close, handle_terminal_command, handle_terminal_input, handle_terminal_resize,
    handle_terminal_session_create_request, handle_terminal_snapshot_request,
};
use crate::terminal::session::LocalTerminalManager;
use crate::{config::ClientConfig, tracing_stdout, LocalState};

const HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
const MCP_CHECK_INTERVAL_SECONDS: u64 = 45;

pub(crate) async fn connect_loop(
    config: ClientConfig,
    state: Arc<RwLock<LocalState>>,
    sandbox_runtime: LocalSandboxRuntime,
    device_id: String,
) -> Result<()> {
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("build local adapter HTTP client")?;
    let ws_path = format!("/api/local-connectors/devices/{device_id}/connect");
    let ws_url = websocket_url(&config.cloud_base_url, ws_path.as_str());
    let connect_request = websocket_connect_request(
        ws_url.as_str(),
        ws_path.as_str(),
        &config,
        device_id.as_str(),
    )?;
    let (ws_stream, _) = tokio_tungstenite::connect_async(connect_request)
        .await
        .with_context(|| format!("connect local connector websocket {ws_url}"))?;
    let (mut write, mut read) = ws_stream.split();
    let terminal_manager = LocalTerminalManager::default();
    let history_recorder = CommandHistoryRecorder {
        state_path: config.state_path.clone(),
        state: state.clone(),
    };
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Value>();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS));
    let mut mcp_check = tokio::time::interval(Duration::from_secs(MCP_CHECK_INTERVAL_SECONDS));
    mcp_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tracing_stdout("connected to local_connector_service");

    loop {
        tokio::select! {
            _ = mcp_check.tick() => {
                if let Err(err) = refresh_enabled_local_mcp_checks(
                    state.as_ref(),
                    config.state_path.as_path(),
                ).await {
                    tracing_stdout(format!("refresh local MCP checks failed: {err}").as_str());
                }
            }
            _ = heartbeat.tick() => {
                write
                    .send(Message::Text(json!({"type": "heartbeat"}).to_string().into()))
                    .await
                    .context("send heartbeat")?;
                let mcp_status = {
                    let state = state.read().await;
                    mcp_status_message(&state)
                };
                if let Some(mcp_status) = mcp_status {
                    write
                        .send(Message::Text(mcp_status.to_string().into()))
                        .await
                        .context("send MCP manifest status")?;
                }
                let skill_inventory = skill_inventory_status_message()
                    .context("build Skill inventory status")?;
                write
                    .send(Message::Text(skill_inventory.to_string().into()))
                    .await
                    .context("send Skill inventory status")?;
            }
            outbound = outbound_rx.recv() => {
                let Some(outbound) = outbound else {
                    return Err(anyhow!("local connector outbound channel closed"));
                };
                write
                    .send(Message::Text(outbound.to_string().into()))
                    .await
                    .context("send relay event")?;
            }
            message = read.next() => {
                let Some(message) = message else {
                    return Err(anyhow!("local connector websocket closed"));
                };
                let message = message.context("read websocket message")?;
                match message {
                    Message::Text(text) => {
                        let state_snapshot = state.read().await.clone();
                        if let Some(response) =
                            handle_text_message(
                                text.as_str(),
                                &state_snapshot,
                                &http_client,
                                &sandbox_runtime,
                                &terminal_manager,
                                &history_recorder,
                                outbound_tx.clone(),
                            ).await
                        {
                            write
                                .send(Message::Text(response.to_string().into()))
                                .await
                                .context("send relay response")?;
                        }
                    }
                    Message::Ping(bytes) => {
                        write.send(Message::Pong(bytes)).await.context("send pong")?;
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

async fn handle_text_message(
    text: &str,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    terminal_manager: &LocalTerminalManager,
    history_recorder: &CommandHistoryRecorder,
    outbound_tx: mpsc::UnboundedSender<Value>,
) -> Option<Value> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if is_remote_control_message(message_type) {
        if let Err(err) = validate_remote_control_context(message_type, &value, state) {
            tracing_stdout(format!("rejected local connector relay message: {err}").as_str());
            return remote_control_error_response(message_type, &value, err);
        }
    }
    match message_type {
        "connected" | "pong" | "ack" | "mcp_manifest_status_ack" | "skill_inventory_status_ack" => {
            tracing_stdout(format!("service message: {message_type}").as_str());
            None
        }
        MCP_RELAY_MESSAGE_TYPE => Some(handle_mcp_request(value, state, history_recorder).await),
        "sandbox_request" => Some(
            handle_sandbox_request(value, state, http_client, sandbox_runtime, history_recorder)
                .await,
        ),
        "terminal_exec_request" => {
            Some(handle_terminal_exec_request(value, state, history_recorder).await)
        }
        "model_runtime_request" => Some(handle_model_runtime_request(value, state).await),
        "skill_prepare_request" => Some(handle_skill_prepare(value, state)),
        "skill_execute_request" => Some(handle_skill_execute(value, state)),
        "skill_cancel_request" => Some(handle_skill_cancel(value)),
        "terminal_session_create_request" => Some(
            handle_terminal_session_create_request(value, state, terminal_manager, outbound_tx)
                .await,
        ),
        "terminal_input" => {
            handle_terminal_input(
                value,
                state,
                terminal_manager,
                history_recorder,
                outbound_tx,
            )
            .await;
            None
        }
        "terminal_command" => {
            handle_terminal_command(value, terminal_manager).await;
            None
        }
        "terminal_resize" => {
            handle_terminal_resize(value, terminal_manager, outbound_tx).await;
            None
        }
        "terminal_snapshot_request" => {
            handle_terminal_snapshot_request(value, terminal_manager, outbound_tx).await;
            None
        }
        "terminal_close" => {
            handle_terminal_close(value, terminal_manager).await;
            None
        }
        _ => {
            tracing_stdout(format!("ignored service message: {message_type}").as_str());
            None
        }
    }
}

fn is_remote_control_message(message_type: &str) -> bool {
    matches!(
        message_type,
        MCP_RELAY_MESSAGE_TYPE
            | "sandbox_request"
            | "terminal_exec_request"
            | "model_runtime_request"
            | "skill_prepare_request"
            | "skill_execute_request"
            | "skill_cancel_request"
            | "terminal_session_create_request"
            | "terminal_input"
            | "terminal_command"
            | "terminal_resize"
            | "terminal_snapshot_request"
            | "terminal_close"
    )
}

fn validate_remote_control_context(
    message_type: &str,
    value: &Value,
    state: &LocalState,
) -> Result<(), String> {
    let request = serde_json::from_value::<RelayRequest>(value.clone())
        .map_err(|err| format!("invalid relay request envelope: {err}"))?;
    let owner_user_id = normalized_optional(request.owner_user_id.as_deref())
        .ok_or_else(|| "relay owner_user_id is required".to_string())?;
    let local_owner_user_id = local_owner_user_id(state)
        .ok_or_else(|| "local connector is not paired to an owner user".to_string())?;
    if owner_user_id != local_owner_user_id {
        return Err("relay owner_user_id does not match the paired owner".to_string());
    }

    let device_id = normalized_optional(request.device_id.as_deref())
        .ok_or_else(|| "relay device_id is required".to_string())?;
    let local_device_id = normalized_optional(state.device_id.as_deref())
        .ok_or_else(|| "local connector device is not registered".to_string())?;
    if device_id != local_device_id {
        return Err("relay device_id does not match this device".to_string());
    }

    let workspace_id = request.workspace_id.trim();
    if workspace_id.is_empty() {
        if relay_allows_empty_workspace(message_type, &request) {
            return Ok(());
        }
        return Err("relay workspace_id is required for this operation".to_string());
    }
    if state.workspace_by_id(workspace_id).is_none() {
        return Err("relay workspace_id is not registered on this device".to_string());
    }
    Ok(())
}

fn relay_allows_empty_workspace(message_type: &str, request: &RelayRequest) -> bool {
    if message_type == "model_runtime_request" {
        return true;
    }
    if matches!(
        message_type,
        "skill_prepare_request" | "skill_execute_request" | "skill_cancel_request"
    ) {
        return true;
    }
    message_type == MCP_RELAY_MESSAGE_TYPE
        && request
            .headers
            .keys()
            .any(|key| key.eq_ignore_ascii_case("x-local-connector-mcp-manifest-id"))
}

fn local_owner_user_id(state: &LocalState) -> Option<&str> {
    state
        .paired_user_id
        .as_deref()
        .and_then(normalized_str)
        .or_else(|| {
            state
                .auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref())
                .and_then(|user| normalized_str(user.id.as_str()))
        })
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.and_then(normalized_str)
}

fn normalized_str(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn remote_control_error_response(
    message_type: &str,
    value: &Value,
    message: String,
) -> Option<Value> {
    let request_id = value
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let response_type = match message_type {
        MCP_RELAY_MESSAGE_TYPE => MCP_RELAY_MESSAGE_TYPE,
        "sandbox_request" => "sandbox_response",
        "terminal_exec_request" => "terminal_response",
        "terminal_session_create_request" => "terminal_session_create_response",
        "model_runtime_request" => "model_runtime_response",
        "skill_prepare_request" => "skill_prepare_response",
        "skill_execute_request" => "skill_execute_response",
        "skill_cancel_request" => "skill_cancel_response",
        _ => return None,
    };
    Some(relay_error_response(
        response_type,
        request_id,
        403,
        message,
    ))
}

fn websocket_connect_request(
    ws_url: &str,
    ws_path: &str,
    config: &ClientConfig,
    device_id: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>> {
    let mut request = ws_url
        .into_client_request()
        .context("build local connector websocket request")?;
    let public_key = config
        .public_key
        .as_deref()
        .ok_or_else(|| anyhow!("local connector device key is unavailable"))?;
    let timestamp = Utc::now().timestamp().to_string();
    let nonce = Uuid::new_v4().to_string();
    let payload = device_signature_payload(device_id, timestamp.as_str(), nonce.as_str(), ws_path);
    let signature =
        sign_device_message(config.state_path.as_path(), public_key, payload.as_bytes())?;
    let headers = request.headers_mut();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(format!("Bearer {}", config.access_token).as_str())
            .context("build local connector authorization header")?,
    );
    headers.insert(
        "x-local-connector-device-id",
        HeaderValue::from_str(device_id).context("build local connector device id header")?,
    );
    headers.insert(
        "x-local-connector-device-timestamp",
        HeaderValue::from_str(timestamp.as_str())
            .context("build local connector device timestamp header")?,
    );
    headers.insert(
        "x-local-connector-device-nonce",
        HeaderValue::from_str(nonce.as_str())
            .context("build local connector device nonce header")?,
    );
    headers.insert(
        "x-local-connector-device-signature",
        HeaderValue::from_str(signature.as_str())
            .context("build local connector device signature header")?,
    );
    headers.insert(
        "x-local-connector-device-signature-alg",
        HeaderValue::from_static("ed25519"),
    );
    Ok(request)
}

fn device_signature_payload(device_id: &str, timestamp: &str, nonce: &str, path: &str) -> String {
    format!("v1\n{device_id}\n{timestamp}\n{nonce}\n{path}")
}

fn websocket_url(base: &str, path: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    let scheme = if trimmed.starts_with("https://") {
        "wss://"
    } else {
        "ws://"
    };
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    format!("{scheme}{without_scheme}{path}")
}
