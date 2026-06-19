use axum::response::sse::Event;
use std::convert::Infallible;
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Clone)]
pub struct SseSender {
    tx: mpsc::UnboundedSender<Result<Event, Infallible>>,
}

impl SseSender {
    pub fn send_json(&self, value: &serde_json::Value) {
        let payload = value.to_string();
        let event = Event::default().data(payload);
        if let Err(err) = self.tx.send(Ok(event)) {
            warn!(error = %err, "sse send_json failed");
        }
    }

    pub fn send_done(&self) {
        if let Err(err) = self.tx.send(Ok(Event::default().data("[DONE]"))) {
            warn!(error = %err, "sse send_done failed");
        }
    }
}
