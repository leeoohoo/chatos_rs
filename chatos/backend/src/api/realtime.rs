// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::Extension;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::api::metrics::{ActiveWebSocketConnection, WebSocketKind};
use crate::api::{RequestAccessToken, RequestClientScopes};
use crate::core::auth::{resolve_auth_user_and_scopes_via_user_service, AuthUser};
use crate::services::realtime::{
    subscribe_user_events, RealtimeAckMessage, RealtimeClientControlMessage, RealtimeErrorMessage,
    RealtimeSubscriptionSet, RealtimeTopic, RealtimeTopicScope,
};
use crate::utils::ws_outbound;

const REALTIME_WS_OUTBOUND_QUEUE_CAPACITY: usize = 256;
const REALTIME_WS_CHANNEL: &str = "realtime";
const COMPANION_SESSION_REVALIDATE_SECONDS: u64 = 30;

pub fn router() -> axum::Router {
    axum::Router::new().route("/api/realtime/ws", axum::routing::get(realtime_ws))
}

async fn realtime_ws(
    auth: AuthUser,
    scopes: Option<Extension<RequestClientScopes>>,
    access_token: Option<Extension<RequestAccessToken>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let companion = scopes
        .as_ref()
        .is_some_and(|Extension(scopes)| scopes.is_wechat_companion());
    let access_token = access_token.map(|Extension(token)| token.as_str().to_string());
    ws.on_upgrade(move |socket| {
        handle_realtime_socket(auth.user_id, socket, companion, access_token)
    })
}

async fn handle_realtime_socket(
    user_id: String,
    socket: WebSocket,
    companion: bool,
    access_token: Option<String>,
) {
    let _active_connection = ActiveWebSocketConnection::start(WebSocketKind::Realtime);
    let mut receiver = subscribe_user_events();
    let (mut sender, mut receiver_ws) = socket.split();
    let (outbound_tx, mut outbound_rx) = ws_outbound::channel(REALTIME_WS_OUTBOUND_QUEUE_CAPACITY);
    let shutdown = CancellationToken::new();
    let subscriptions = Arc::new(Mutex::new(RealtimeSubscriptionSet::default()));
    let mut session_revalidation = tokio::time::interval(std::time::Duration::from_secs(
        COMPANION_SESSION_REVALIDATE_SECONDS,
    ));
    session_revalidation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    session_revalidation.tick().await;

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
                        let payload = match serialize_event(envelope.as_ref(), companion) {
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
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            user_id = %user_id,
                            skipped,
                            "realtime websocket subscriber lagged; closing connection for reconciliation"
                        );
                        shutdown.cancel();
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });

    loop {
        let msg = tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = session_revalidation.tick(), if companion => {
                if !companion_session_still_valid(user_id.as_str(), access_token.as_deref()).await {
                    break;
                }
                continue;
            }
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
                            if companion && !companion_topics_allowed(control.topics.as_slice()) {
                                Err(
                                    "WeChat Companion may only subscribe to a conversation topic"
                                        .to_string(),
                                )
                            } else {
                                subscriptions.subscribe(control.topics)
                            }
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

async fn companion_session_still_valid(user_id: &str, access_token: Option<&str>) -> bool {
    let Some(access_token) = access_token else {
        return false;
    };
    matches!(
        resolve_auth_user_and_scopes_via_user_service(access_token).await,
        Ok((auth, scopes))
            if auth.user_id == user_id
                && scopes.iter().any(|scope| scope == "wechat_companion")
    )
}

fn companion_topics_allowed(topics: &[RealtimeTopic]) -> bool {
    !topics.is_empty()
        && topics
            .iter()
            .all(|topic| topic.scope == RealtimeTopicScope::Conversation && topic.id.is_some())
}

fn serialize_event(
    envelope: &crate::services::realtime::SequencedRealtimeEventEnvelope,
    companion: bool,
) -> Result<String, serde_json::Error> {
    if !companion {
        return serde_json::to_string(envelope);
    }
    serde_json::to_string(&serde_json::json!({
        "type": envelope.message_type,
        "event": envelope.event,
        "event_id": envelope.event_id,
        "event_sequence": envelope.event_sequence,
        "conversation_id": envelope.conversation_id,
        "ts": envelope.ts,
    }))
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

#[cfg(test)]
mod tests {
    use super::{companion_topics_allowed, serialize_event};
    use crate::services::realtime::{
        ChatStreamRealtimePayload, RealtimeEventEnvelope, RealtimeEventPayload, RealtimeTopic,
        RealtimeTopicScope, SequencedRealtimeEventEnvelope,
    };
    use serde_json::json;

    #[test]
    fn companion_realtime_only_accepts_conversation_topics() {
        assert!(companion_topics_allowed(&[RealtimeTopic {
            scope: RealtimeTopicScope::Conversation,
            id: Some("conversation-1".to_string()),
        }]));
        assert!(!companion_topics_allowed(&[RealtimeTopic {
            scope: RealtimeTopicScope::Sessions,
            id: None,
        }]));
    }

    #[test]
    fn companion_realtime_event_is_an_invalidation_without_runtime_payload() {
        let envelope = SequencedRealtimeEventEnvelope {
            event_id: "event-1".to_string(),
            event_sequence: 1,
            envelope: RealtimeEventEnvelope {
                message_type: "event",
                event: "chat.delta",
                user_id: "user-1".to_string(),
                conversation_id: Some("conversation-1".to_string()),
                project_id: Some("project-secret".to_string()),
                payload: RealtimeEventPayload::ChatStream(ChatStreamRealtimePayload {
                    conversation_id: "conversation-1".to_string(),
                    conversation_turn_id: Some("turn-1".to_string()),
                    project_id: Some("project-secret".to_string()),
                    user_message_id: None,
                    stream_type: "delta".to_string(),
                    raw: json!({ "workspace_root": "/private/work", "delta": "secret" }),
                }),
                ts: "2026-09-14T00:00:00Z".to_string(),
            },
        };
        let value: serde_json::Value = serde_json::from_str(
            serialize_event(&envelope, true)
                .expect("serialize companion event")
                .as_str(),
        )
        .expect("parse companion event");
        assert_eq!(value["conversation_id"], "conversation-1");
        assert!(value.get("payload").is_none());
        assert!(value.get("project_id").is_none());
        assert!(value.get("user_id").is_none());
    }
}
