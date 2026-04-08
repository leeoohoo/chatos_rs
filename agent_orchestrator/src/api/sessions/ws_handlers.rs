use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::api::chat_stream_common::{validate_chat_stream_request, ChatStreamRequest};
use crate::api::chat_v2::stream_chat_v2;
use crate::api::chat_v3::stream_chat_v3;
use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::services::memory_server_client;
use crate::services::session_event_hub::session_event_hub;
use crate::utils::abort_registry;
use crate::utils::chat_event_sender::{ChatEventSender, WsEventSender};
use crate::utils::events::Events;

#[derive(Debug, Deserialize)]
struct SessionWsClientMessage {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(default)]
    request: Option<ChatStreamRequest>,
}

pub(super) async fn session_events_ws(
    auth: AuthUser,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err).into_response();
    }
    let access_token = memory_server_client::current_access_token();
    let user_id = auth.user_id;
    ws.on_upgrade(move |socket| handle_session_socket(session_id, user_id, access_token, socket))
}

async fn handle_session_socket(
    session_id: String,
    user_id: String,
    access_token: Option<String>,
    socket: WebSocket,
) {
    let hub = session_event_hub();
    let mut receiver = hub.subscribe(&session_id);
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
    let outbound_sender = WsEventSender::new(outbound_tx.clone());
    let active_chat = Arc::new(AtomicBool::new(false));
    let (mut sender, mut receiver_ws) = socket.split();

    outbound_sender.send_text(
        json!({
            "type": "session_events.connected",
            "session_id": session_id.clone(),
            "timestamp": crate::core::time::now_rfc3339(),
        })
        .to_string(),
    );

    let writer_task = tokio::spawn(async move {
        while let Some(payload) = outbound_rx.recv().await {
            if sender.send(Message::Text(payload)).await.is_err() {
                break;
            }
        }
    });

    let relay_tx = outbound_tx.clone();
    let relay_task = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(payload) => {
                    if relay_tx.send(payload.to_string()).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(msg) = receiver_ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                handle_client_message(
                    &session_id,
                    &user_id,
                    access_token.clone(),
                    outbound_sender.clone(),
                    active_chat.clone(),
                    text.as_str(),
                )
                .await;
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Binary(_)) | Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
            Err(_) => break,
        }
    }

    relay_task.abort();
    writer_task.abort();
    let _ = relay_task.await;
    let _ = writer_task.await;
}

async fn handle_client_message(
    session_id: &str,
    user_id: &str,
    access_token: Option<String>,
    sender: WsEventSender,
    active_chat: Arc<AtomicBool>,
    text: &str,
) {
    let parsed = match serde_json::from_str::<SessionWsClientMessage>(text) {
        Ok(value) => value,
        Err(err) => {
            send_ws_error(
                &sender,
                format!("invalid session ws payload: {}", err),
                Some("invalid_ws_payload"),
            );
            return;
        }
    };

    match parsed.message_type.as_str() {
        "chat.send" => {
            let mut request = match parsed.request {
                Some(value) => value,
                None => {
                    send_ws_error(
                        &sender,
                        "missing chat request",
                        Some("missing_chat_request"),
                    );
                    return;
                }
            };
            request.session_id = Some(session_id.to_string());
            request.user_id = Some(user_id.to_string());

            if let Err((status, axum::Json(payload))) =
                validate_chat_stream_request(&request, false)
            {
                let message = payload
                    .get("error")
                    .and_then(|value| value.as_str())
                    .unwrap_or("invalid chat request");
                sender.send_json(&json!({
                    "type": Events::ERROR,
                    "timestamp": crate::core::time::now_rfc3339(),
                    "status": status.as_u16(),
                    "message": message,
                    "data": payload,
                }));
                return;
            }

            if active_chat
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                send_ws_error(
                    &sender,
                    "当前会话已有进行中的对话，请等待完成或先停止",
                    Some("chat_already_running"),
                );
                return;
            }

            abort_registry::reset(session_id);
            let stream_sender = sender.clone();
            let stream_access_token = access_token.clone();
            let active_flag = active_chat.clone();
            let use_responses = request.ai_model_config.as_ref().and_then(|cfg| {
                cfg.get("supports_responses")
                    .and_then(|value| value.as_bool())
            }) == Some(true);

            tokio::spawn(async move {
                memory_server_client::with_access_token_scope(stream_access_token, async move {
                    if use_responses {
                        stream_chat_v3(stream_sender.clone(), request).await;
                    } else {
                        stream_chat_v2(stream_sender.clone(), request, false, true, false).await;
                    }
                })
                .await;
                active_flag.store(false, Ordering::SeqCst);
            });
        }
        "chat.stop" => {
            let _ = abort_registry::abort(session_id);
        }
        _ => {
            send_ws_error(
                &sender,
                format!(
                    "unsupported session ws message type: {}",
                    parsed.message_type
                ),
                Some("unsupported_ws_message"),
            );
        }
    }
}

fn send_ws_error(sender: &WsEventSender, message: impl Into<String>, code: Option<&str>) {
    let message = message.into();
    sender.send_json(&json!({
        "type": Events::ERROR,
        "timestamp": crate::core::time::now_rfc3339(),
        "message": message.clone(),
        "code": code,
        "data": {
            "error": message.clone(),
            "message": message,
            "code": code,
        }
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicBool, Arc};

    use serde_json::Value;
    use tokio::sync::mpsc;

    use super::handle_client_message;
    use crate::utils::abort_registry;
    use crate::utils::chat_event_sender::WsEventSender;
    use crate::utils::events::Events;

    async fn recv_json(rx: &mut mpsc::UnboundedReceiver<String>) -> Value {
        let raw = rx.recv().await.expect("expected ws payload");
        serde_json::from_str(raw.as_str()).expect("valid json payload")
    }

    #[tokio::test]
    async fn reports_invalid_ws_payload() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = WsEventSender::new(tx);

        handle_client_message(
            "session-1",
            "user-1",
            None,
            sender,
            Arc::new(AtomicBool::new(false)),
            "{",
        )
        .await;

        let payload = recv_json(&mut rx).await;
        assert_eq!(
            payload.get("type").and_then(Value::as_str),
            Some(Events::ERROR)
        );
        assert_eq!(
            payload
                .get("data")
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str),
            Some("invalid_ws_payload")
        );
    }

    #[tokio::test]
    async fn reports_missing_chat_request() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = WsEventSender::new(tx);

        handle_client_message(
            "session-1",
            "user-1",
            None,
            sender,
            Arc::new(AtomicBool::new(false)),
            r#"{"type":"chat.send"}"#,
        )
        .await;

        let payload = recv_json(&mut rx).await;
        assert_eq!(
            payload
                .get("data")
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str),
            Some("missing_chat_request")
        );
    }

    #[tokio::test]
    async fn reports_invalid_chat_request() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = WsEventSender::new(tx);

        handle_client_message(
            "session-1",
            "user-1",
            None,
            sender,
            Arc::new(AtomicBool::new(false)),
            r#"{"type":"chat.send","request":{"content":"   "}}"#,
        )
        .await;

        let payload = recv_json(&mut rx).await;
        assert_eq!(
            payload.get("type").and_then(Value::as_str),
            Some(Events::ERROR)
        );
        assert_eq!(payload.get("status").and_then(Value::as_u64), Some(400));
    }

    #[tokio::test]
    async fn aborts_session_on_chat_stop_message() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let sender = WsEventSender::new(tx);
        abort_registry::reset("session-stop");
        assert!(!abort_registry::is_aborted("session-stop"));

        handle_client_message(
            "session-stop",
            "user-1",
            None,
            sender,
            Arc::new(AtomicBool::new(false)),
            r#"{"type":"chat.stop"}"#,
        )
        .await;

        assert!(abort_registry::is_aborted("session-stop"));
    }

    #[tokio::test]
    async fn rejects_chat_send_when_another_chat_is_running() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = WsEventSender::new(tx);

        handle_client_message(
            "session-1",
            "user-1",
            None,
            sender,
            Arc::new(AtomicBool::new(true)),
            r#"{"type":"chat.send","request":{"content":"hello"}}"#,
        )
        .await;

        let payload = recv_json(&mut rx).await;
        assert_eq!(
            payload
                .get("data")
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str),
            Some("chat_already_running")
        );
    }

    #[tokio::test]
    async fn rejects_unsupported_ws_message_type() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = WsEventSender::new(tx);

        handle_client_message(
            "session-1",
            "user-1",
            None,
            sender,
            Arc::new(AtomicBool::new(false)),
            r#"{"type":"ping"}"#,
        )
        .await;

        let payload = recv_json(&mut rx).await;
        assert_eq!(
            payload
                .get("data")
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str),
            Some("unsupported_ws_message")
        );
    }
}
