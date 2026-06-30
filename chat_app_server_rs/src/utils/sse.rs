use axum::response::sse::Event;
use std::convert::Infallible;
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Clone)]
pub struct SseSender {
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

impl SseSender {
    pub fn send_json(&self, value: &serde_json::Value) {
        let payload = value.to_string();
        let event = Event::default().data(payload);
        self.send_event(event, "send_json");
    }

    pub fn send_done(&self) {
        self.send_event(Event::default().data("[DONE]"), "send_done");
    }

    fn send_event(&self, event: Event, operation: &'static str) {
        match self.tx.try_send(Ok(event)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!(operation = %operation, "sse outbound queue full; dropping event");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                warn!(operation = %operation, "sse outbound channel closed");
            }
        }
    }
}
