use serde_json::Value;
use tokio::sync::mpsc;

use super::chat_event_sender::{ChatEventSender, WsEventSender};
use super::events::Events;

#[tokio::test]
async fn ws_event_sender_emits_json_payload() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = WsEventSender::new(tx);

    sender.send_json(&serde_json::json!({
        "type": "chunk",
        "content": "hello",
    }));

    let raw = rx.recv().await.expect("expected ws payload");
    let payload: Value = serde_json::from_str(raw.as_str()).expect("valid json");
    assert_eq!(payload.get("type").and_then(Value::as_str), Some("chunk"));
    assert_eq!(
        payload.get("content").and_then(Value::as_str),
        Some("hello")
    );
}

#[tokio::test]
async fn ws_event_sender_emits_done_event() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = WsEventSender::new(tx);

    sender.send_done();

    let raw = rx.recv().await.expect("expected done payload");
    let payload: Value = serde_json::from_str(raw.as_str()).expect("valid json");
    assert_eq!(
        payload.get("type").and_then(Value::as_str),
        Some(Events::DONE)
    );
    assert!(payload.get("timestamp").and_then(Value::as_str).is_some());
}
