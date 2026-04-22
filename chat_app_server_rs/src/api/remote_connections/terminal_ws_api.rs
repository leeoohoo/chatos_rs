use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, Query, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

use super::{
    get_remote_terminal_manager, resolve_jump_connection_snapshot, ws_error_output,
    RemoteTerminalEvent, RemoteTerminalWsQuery, WsInput, WsOutput,
};

pub(super) async fn remote_terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<RemoteTerminalWsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err).into_response(),
    };

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => {
            return axum::Json(serde_json::json!({
                "error": err,
                "code": crate::core::remote_connection_error_codes::remote_connection_codes::AUTH_FAILED
            }))
            .into_response()
        }
    };

    let verification_code = query.verification_code;
    ws.on_upgrade(move |socket| {
        handle_remote_terminal_socket(resolved_connection, verification_code.clone(), socket)
    })
}

async fn handle_remote_terminal_socket(
    connection: RemoteConnection,
    verification_code: Option<String>,
    socket: WebSocket,
) {
    let has_initial_verification_code = verification_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    let (mut sender, mut receiver_ws) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();

    let forward_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let (verification_tx, verification_rx) = std_mpsc::channel::<String>();
    let (challenge_tx, challenge_rx) = std_mpsc::channel::<String>();
    let challenge_outbound_tx = outbound_tx.clone();
    let challenge_task = tokio::task::spawn_blocking(move || {
        while let Ok(prompt) = challenge_rx.recv() {
            let payload = serde_json::to_string(&WsOutput::Error {
                error: "需要二次验证".to_string(),
                code: crate::core::remote_connection_error_codes::remote_connection_codes::SECOND_FACTOR_REQUIRED
                    .to_string(),
                challenge_prompt: Some(prompt),
            })
            .unwrap_or_else(|_| "{}".to_string());
            if challenge_outbound_tx.send(Message::Text(payload)).is_err() {
                break;
            }
        }
    });

    let connection_for_startup = connection.clone();
    let startup = tokio::spawn(async move {
        let manager = get_remote_terminal_manager();
        manager
            .ensure_running(
                &connection_for_startup,
                verification_code.as_deref(),
                Some(verification_rx),
                Some(challenge_tx),
            )
            .await
    });
    let mut startup = Some(startup);

    loop {
        tokio::select! {
            startup_result = async {
                match startup.as_mut() {
                    Some(handle) => handle.await,
                    None => std::future::pending().await,
                }
            } => {
                let session = match startup_result {
                    Ok(Ok(session)) => session,
                    Ok(Err(err)) => {
                        warn!(
                            connection_id = connection.id.as_str(),
                            host = connection.host.as_str(),
                            port = connection.port,
                            username = connection.username.as_str(),
                            auth_type = connection.auth_type.as_str(),
                            jump_enabled = connection.jump_enabled,
                            has_verification_code = has_initial_verification_code,
                            error = err.as_str(),
                            "Remote terminal startup failed"
                        );
                        let _ = outbound_tx.send(Message::Text(
                            serde_json::to_string(&ws_error_output(err)).unwrap_or_default(),
                        ));
                        challenge_task.abort();
                        forward_task.abort();
                        let _ = challenge_task.await;
                        let _ = forward_task.await;
                        return;
                    }
                    Err(err) => {
                        let _ = outbound_tx.send(Message::Text(
                            serde_json::to_string(&ws_error_output(format!(
                                "remote terminal startup task failed: {err}"
                            )))
                            .unwrap_or_default(),
                        ));
                        challenge_task.abort();
                        forward_task.abort();
                        let _ = challenge_task.await;
                        let _ = forward_task.await;
                        return;
                    }
                };

                run_connected_remote_terminal_socket(
                    connection,
                    session,
                    receiver_ws,
                    outbound_tx,
                    forward_task,
                    challenge_task,
                )
                .await;
                return;
            }
            maybe_msg = receiver_ws.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsInput>(&text) {
                            Ok(WsInput::Verification { code }) => {
                                let _ = verification_tx.send(code);
                            }
                            Ok(WsInput::Ping) => {
                                let timestamp = crate::core::time::now_rfc3339();
                                let payload = serde_json::to_string(&WsOutput::Pong { timestamp })
                                    .unwrap_or_else(|_| "{}".to_string());
                                let _ = outbound_tx.send(Message::Text(payload));
                            }
                            Ok(_) => {
                                // Defer normal terminal input until the SSH shell is ready.
                            }
                            Err(err) => {
                                let payload = serde_json::to_string(&ws_error_output(format!(
                                    "invalid ws message: {err}"
                                )))
                                .unwrap_or_else(|_| "{}".to_string());
                                let _ = outbound_tx.send(Message::Text(payload));
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        challenge_task.abort();
                        forward_task.abort();
                        let _ = challenge_task.await;
                        let _ = forward_task.await;
                        return;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {
                        challenge_task.abort();
                        forward_task.abort();
                        let _ = challenge_task.await;
                        let _ = forward_task.await;
                        return;
                    }
                }
            }
        }
    }
}

async fn run_connected_remote_terminal_socket(
    connection: RemoteConnection,
    session: std::sync::Arc<super::remote_terminal::RemoteTerminalSession>,
    mut receiver_ws: futures::stream::SplitStream<WebSocket>,
    tx: mpsc::UnboundedSender<Message>,
    forward_task: tokio::task::JoinHandle<()>,
    challenge_task: tokio::task::JoinHandle<()>,
) {
    session.touch_activity();
    let _ = RemoteConnectionService::touch(&connection.id).await;

    let mut receiver = session.subscribe();

    let snapshot = session.output_snapshot();
    if !snapshot.is_empty() {
        let payload = serde_json::to_string(&WsOutput::Snapshot { data: snapshot })
            .unwrap_or_else(|_| "{}".to_string());
        if tx.send(Message::Text(payload)).is_err() {
            return;
        }
    }
    let payload = serde_json::to_string(&WsOutput::State {
        busy: session.is_busy(),
    })
    .unwrap_or_else(|_| "{}".to_string());
    if tx.send(Message::Text(payload)).is_err() {
        return;
    }

    let tx_events = tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(RemoteTerminalEvent::Output(data)) => {
                    let text = serde_json::to_string(&WsOutput::Output { data })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Ok(RemoteTerminalEvent::Exit(code)) => {
                    let text = serde_json::to_string(&WsOutput::Exit { code })
                        .unwrap_or_else(|_| "{}".to_string());
                    let _ = tx_events.send(Message::Text(text));
                    break;
                }
                Ok(RemoteTerminalEvent::State(busy)) => {
                    let text = serde_json::to_string(&WsOutput::State { busy })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(Ok(msg)) = receiver_ws.next().await {
        match msg {
            Message::Text(text) => {
                let parsed = serde_json::from_str::<WsInput>(&text);
                match parsed {
                    Ok(WsInput::Input { data }) => {
                        if let Err(err) = session.write_input(data.as_str()) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Command { command }) => {
                        let mut cmd = command;
                        if !cmd.ends_with('\n') {
                            cmd.push('\n');
                        }
                        if let Err(err) = session.write_input(cmd.as_str()) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Resize { cols, rows }) => {
                        if let Err(err) = session.resize(cols, rows) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        }
                    }
                    Ok(WsInput::Verification { .. }) => {}
                    Ok(WsInput::Ping) => {
                        session.touch_activity();
                        let timestamp = crate::core::time::now_rfc3339();
                        let payload = serde_json::to_string(&WsOutput::Pong { timestamp })
                            .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                    Err(err) => {
                        let payload = serde_json::to_string(&ws_error_output(format!(
                            "invalid ws message: {err}"
                        )))
                        .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                }
            }
            Message::Binary(data) => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = session.write_input(text.as_str());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    info!(
        connection_id = connection.id.as_str(),
        host = connection.host.as_str(),
        port = connection.port,
        "Remote terminal websocket closed"
    );

    drop(tx);
    event_task.abort();
    challenge_task.abort();
    forward_task.abort();
    let _ = event_task.await;
    let _ = challenge_task.await;
    let _ = forward_task.await;
}
