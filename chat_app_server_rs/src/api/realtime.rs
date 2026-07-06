// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::core::auth::AuthUser;
use crate::services::realtime::{
    subscribe_user_events, RealtimeAckMessage, RealtimeClientControlMessage, RealtimeErrorMessage,
    RealtimeSubscriptionSet,
};
use crate::utils::ws_outbound;

const REALTIME_WS_OUTBOUND_QUEUE_CAPACITY: usize = 256;
const REALTIME_WS_CHANNEL: &str = "realtime";

pub fn router() -> axum::Router {
    axum::Router::new().route("/api/realtime/ws", axum::routing::get(realtime_ws))
}

async fn realtime_ws(auth: AuthUser, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_realtime_socket(auth.user_id, socket))
}

async fn handle_realtime_socket(user_id: String, socket: WebSocket) {
    let mut receiver = subscribe_user_events();
    let (mut sender, mut receiver_ws) = socket.split();
    let (outbound_tx, mut outbound_rx) = ws_outbound::channel(REALTIME_WS_OUTBOUND_QUEUE_CAPACITY);
    let shutdown = CancellationToken::new();
    let subscriptions = Arc::new(Mutex::new(RealtimeSubscriptionSet::default()));

    let send_task = tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    maybe_msg = outbound_rx.recv() => {
                        let Some(msg) = maybe_msg else {
                            break;
                        };
                        tokio::select! {
                            _ = shutdown.cancelled() => break,
                            result = sender.send(msg) => {
                                if result.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let events_task = tokio::spawn({
        let outbound_tx = outbound_tx.clone();
        let shutdown = shutdown.clone();
        let user_id = user_id.clone();
        let subscriptions = subscriptions.clone();
        async move {
            loop {
                let received = tokio::select! {
                    _ = shutdown.cancelled() => break,
                    received = receiver.recv() => received,
                };
                match received {
                    Ok(envelope) => {
                        if envelope.user_id != user_id {
                            continue;
                        }
                        let allowed = {
                            let subscriptions = subscriptions.lock().await;
                            subscriptions.allows(envelope.as_ref())
                        };
                        if !allowed {
                            continue;
                        }
                        let payload = match serde_json::to_string(envelope.as_ref()) {
                            Ok(value) => value,
                            Err(_) => continue,
                        };
                        if !ws_outbound::try_send_or_close(
                            &outbound_tx,
                            Message::text(payload),
                            REALTIME_WS_CHANNEL,
                            &shutdown,
                        ) {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });

    loop {
        let msg = tokio::select! {
            _ = shutdown.cancelled() => break,
            msg = receiver_ws.next() => msg,
        };
        match msg {
            None => break,
            Some(Ok(Message::Text(text))) => {
                if is_ping_message(text.as_str()) {
                    let pong = serde_json::json!({
                        "type": "pong",
                        "ts": crate::core::time::now_rfc3339()
                    });
                    if !ws_outbound::try_send_or_close(
                        &outbound_tx,
                        Message::text(pong.to_string()),
                        REALTIME_WS_CHANNEL,
                        &shutdown,
                    ) {
                        break;
                    }
                    continue;
                }
                match serde_json::from_str::<RealtimeClientControlMessage>(text.as_str()) {
                    Ok(control) if control.message_type == "subscribe" => {
                        let result = {
                            let mut subscriptions = subscriptions.lock().await;
                            subscriptions.subscribe(control.topics)
                        };
                        if !send_control_response(
                            &outbound_tx,
                            &shutdown,
                            result.map(|topics| {
                                serde_json::to_string(&RealtimeAckMessage {
                                    message_type: "ack",
                                    acked: "subscribe",
                                    topics,
                                })
                            }),
                        ) {
                            break;
                        }
                    }
                    Ok(control) if control.message_type == "unsubscribe" => {
                        let result = {
                            let mut subscriptions = subscriptions.lock().await;
                            subscriptions.unsubscribe(control.topics)
                        };
                        if !send_control_response(
                            &outbound_tx,
                            &shutdown,
                            result.map(|topics| {
                                serde_json::to_string(&RealtimeAckMessage {
                                    message_type: "ack",
                                    acked: "unsubscribe",
                                    topics,
                                })
                            }),
                        ) {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {}
                }
            }
            Some(Ok(Message::Ping(bytes))) => {
                if !ws_outbound::try_send_or_close(
                    &outbound_tx,
                    Message::Pong(bytes),
                    REALTIME_WS_CHANNEL,
                    &shutdown,
                ) {
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
            Some(Ok(_)) => {}
        }
    }

    shutdown.cancel();
    events_task.abort();
    send_task.abort();
}

fn is_ping_message(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("ping") {
        return true;
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(|item| item.as_str())
                .map(str::to_string)
        })
        .map(|value| value == "ping")
        .unwrap_or(false)
}

fn send_control_response(
    outbound_tx: &ws_outbound::WsOutboundSender,
    shutdown: &CancellationToken,
    payload: Result<Result<String, serde_json::Error>, String>,
) -> bool {
    match payload {
        Ok(Ok(text)) => ws_outbound::try_send_or_close(
            outbound_tx,
            Message::text(text),
            REALTIME_WS_CHANNEL,
            shutdown,
        ),
        Ok(Err(err)) => {
            let error = RealtimeErrorMessage {
                message_type: "error",
                code: "encode_failed",
                message: err.to_string(),
            };
            if let Ok(text) = serde_json::to_string(&error) {
                return ws_outbound::try_send_or_close(
                    outbound_tx,
                    Message::text(text),
                    REALTIME_WS_CHANNEL,
                    shutdown,
                );
            }
            true
        }
        Err(message) => {
            let error = RealtimeErrorMessage {
                message_type: "error",
                code: "invalid_topic",
                message,
            };
            if let Ok(text) = serde_json::to_string(&error) {
                return ws_outbound::try_send_or_close(
                    outbound_tx,
                    Message::text(text),
                    REALTIME_WS_CHANNEL,
                    shutdown,
                );
            }
            true
        }
    }
}
