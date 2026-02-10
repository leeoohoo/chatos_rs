use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::warn;

#[derive(Clone)]
pub struct SseSender {
    tx: mpsc::UnboundedSender<Result<Event, Infallible>>,
}

impl SseSender {
    pub fn send_event(&self, event: Event) {
        if let Err(err) = self.tx.send(Ok(event)) {
            warn!(error = %err, "sse send_event failed");
        }
    }

    pub fn send_json(&self, value: &serde_json::Value) {
        let payload = value.to_string();
        let event = Event::default().data(payload);
        if let Err(err) = self.tx.send(Ok(event)) {
            warn!(error = %err, "sse send_json failed");
        }
    }

    pub fn send_raw(&self, data: &str) {
        let event = Event::default().data(data.to_string());
        if let Err(err) = self.tx.send(Ok(event)) {
            warn!(error = %err, "sse send_raw failed");
        }
    }

    pub fn send_done(&self) {
        if let Err(err) = self.tx.send(Ok(Event::default().data("[DONE]"))) {
            warn!(error = %err, "sse send_done failed");
        }
    }
}

pub fn sse_channel() -> (
    Sse<impl Stream<Item = Result<Event, Infallible>>>,
    SseSender,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let stream = UnboundedReceiverStream::new(rx);
    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    );
    (sse, SseSender { tx })
}
