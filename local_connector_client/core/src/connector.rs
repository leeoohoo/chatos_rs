// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;

use crate::history::CommandHistoryRecorder;
use crate::mcp::service::handle_mcp_request;
use crate::relay::MCP_RELAY_MESSAGE_TYPE;
use crate::sandbox::relay::handle_sandbox_request;
use crate::sandbox::types::LocalSandboxRuntime;
use crate::terminal::exec::handle_terminal_exec_request;
use crate::terminal::relay::{
    handle_terminal_close, handle_terminal_input, handle_terminal_resize,
    handle_terminal_session_create_request, handle_terminal_snapshot_request,
};
use crate::terminal::session::LocalTerminalManager;
use crate::{config::ClientConfig, tracing_stdout, LocalState};

const HEARTBEAT_INTERVAL_SECONDS: u64 = 15;

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
    let ws_url = websocket_url(
        &config.cloud_base_url,
        format!("/api/local-connectors/devices/{device_id}/connect").as_str(),
        config.access_token.as_str(),
    );
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url.as_str())
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
    tracing_stdout("connected to local_connector_service");

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                write
                    .send(Message::Text(json!({"type": "heartbeat"}).to_string().into()))
                    .await
                    .context("send heartbeat")?;
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
    match message_type {
        "connected" | "pong" | "ack" => {
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

fn websocket_url(base: &str, path: &str, token: &str) -> String {
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
    format!(
        "{scheme}{without_scheme}{path}?token={}",
        urlencoding::encode(token)
    )
}
