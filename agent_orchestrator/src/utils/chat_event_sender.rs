use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::warn;

use crate::utils::events::Events;
use crate::utils::sse::SseSender;

pub trait ChatEventSender: Clone + Send + Sync + 'static {
    fn send_json(&self, value: &Value);
    fn send_done(&self);
}

impl ChatEventSender for SseSender {
    fn send_json(&self, value: &Value) {
        SseSender::send_json(self, value);
    }

    fn send_done(&self) {
        SseSender::send_done(self);
    }
}

#[derive(Clone)]
pub struct WsEventSender {
    tx: mpsc::UnboundedSender<String>,
}

impl WsEventSender {
    pub fn new(tx: mpsc::UnboundedSender<String>) -> Self {
        Self { tx }
    }

    pub fn send_text(&self, text: impl Into<String>) {
        if let Err(err) = self.tx.send(text.into()) {
            warn!(error = %err, "ws send_text failed");
        }
    }
}

impl ChatEventSender for WsEventSender {
    fn send_json(&self, value: &Value) {
        self.send_text(value.to_string());
    }

    fn send_done(&self) {
        self.send_json(&json!({
            "type": Events::DONE,
            "timestamp": crate::core::time::now_rfc3339(),
        }));
    }
}
