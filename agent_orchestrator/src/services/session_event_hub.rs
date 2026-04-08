use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::OnceCell;
use serde_json::Value;
use tokio::sync::broadcast;

pub struct SessionEventHub {
    channels: DashMap<String, broadcast::Sender<Value>>,
}

impl SessionEventHub {
    pub fn new() -> Self {
        Self {
            channels: DashMap::new(),
        }
    }

    fn sender(&self, session_id: &str) -> broadcast::Sender<Value> {
        if let Some(sender) = self.channels.get(session_id) {
            return sender.clone();
        }
        let (sender, _) = broadcast::channel(1024);
        self.channels
            .entry(session_id.to_string())
            .or_insert_with(|| sender.clone())
            .clone()
    }

    pub fn subscribe(&self, session_id: &str) -> broadcast::Receiver<Value> {
        self.sender(session_id).subscribe()
    }

    pub fn publish(&self, session_id: &str, payload: Value) {
        let _ = self.sender(session_id).send(payload);
    }
}

static SESSION_EVENT_HUB: OnceCell<Arc<SessionEventHub>> = OnceCell::new();

pub fn session_event_hub() -> Arc<SessionEventHub> {
    SESSION_EVENT_HUB
        .get_or_init(|| Arc::new(SessionEventHub::new()))
        .clone()
}
