use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::core::auth::AuthUser;
use crate::services::realtime::{
    subscribe_user_events, RealtimeAckMessage, RealtimeClientControlMessage,
    RealtimeErrorMessage, RealtimeSubscriptionSet,
};

pub fn router() -> axum::Router {
    axum::Router::new().route("/api/realtime/ws", axum::routing::get(realtime_ws))
}

async fn realtime_ws(auth: AuthUser, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_realtime_socket(auth.user_id, socket))
}

async fn handle_realtime_socket(user_id: String, socket: WebSocket) {
    let mut receiver = subscribe_user_events();
    let (mut sender, mut receiver_ws) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
    let subscriptions = Arc::new(Mutex::new(RealtimeSubscriptionSet::default()));

    let send_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let events_task = tokio::spawn({
        let outbound_tx = outbound_tx.clone();
        let user_id = user_id.clone();
        let subscriptions = subscriptions.clone();
        async move {
            loop {
                match receiver.recv().await {
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
                        if outbound_tx.send(Message::Text(payload)).is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });

    while let Some(msg) = receiver_ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if is_ping_message(text.as_str()) {
                    let pong = serde_json::json!({
                        "type": "pong",
                        "ts": crate::core::time::now_rfc3339()
                    });
                    let _ = outbound_tx.send(Message::Text(pong.to_string()));
                    continue;
                }
                match serde_json::from_str::<RealtimeClientControlMessage>(text.as_str()) {
                    Ok(control) if control.message_type == "subscribe" => {
                        let result = {
                            let mut subscriptions = subscriptions.lock().await;
                            subscriptions.subscribe(control.topics)
                        };
                        send_control_response(
                            &outbound_tx,
                            result.map(|topics| serde_json::to_string(&RealtimeAckMessage {
                                message_type: "ack",
                                acked: "subscribe",
                                topics,
                            })),
                        );
                    }
                    Ok(control) if control.message_type == "unsubscribe" => {
                        let result = {
                            let mut subscriptions = subscriptions.lock().await;
                            subscriptions.unsubscribe(control.topics)
                        };
                        send_control_response(
                            &outbound_tx,
                            result.map(|topics| serde_json::to_string(&RealtimeAckMessage {
                                message_type: "ack",
                                acked: "unsubscribe",
                                topics,
                            })),
                        );
                    }
                    Ok(_) => {}
                    Err(_) => {}
                }
            }
            Ok(Message::Ping(bytes)) => {
                let _ = outbound_tx.send(Message::Pong(bytes));
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

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
        .and_then(|value| value.get("type").and_then(|item| item.as_str()).map(str::to_string))
        .map(|value| value == "ping")
        .unwrap_or(false)
}

fn send_control_response(
    outbound_tx: &mpsc::UnboundedSender<Message>,
    payload: Result<Result<String, serde_json::Error>, String>,
) {
    match payload {
        Ok(Ok(text)) => {
            let _ = outbound_tx.send(Message::Text(text));
        }
        Ok(Err(err)) => {
            let error = RealtimeErrorMessage {
                message_type: "error",
                code: "encode_failed",
                message: err.to_string(),
            };
            if let Ok(text) = serde_json::to_string(&error) {
                let _ = outbound_tx.send(Message::Text(text));
            }
        }
        Err(message) => {
            let error = RealtimeErrorMessage {
                message_type: "error",
                code: "invalid_topic",
                message,
            };
            if let Ok(text) = serde_json::to_string(&error) {
                let _ = outbound_tx.send(Message::Text(text));
            }
        }
    }
}
