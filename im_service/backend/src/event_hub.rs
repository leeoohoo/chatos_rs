use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::broadcast;

#[derive(Default)]
pub struct ImEventHub {
    channels: Mutex<HashMap<String, broadcast::Sender<Value>>>,
}

impl ImEventHub {
    pub fn new() -> Self {
        Self::default()
    }

    fn sender(&self, user_id: &str) -> broadcast::Sender<Value> {
        let normalized_user_id = user_id.trim().to_string();
        let mut channels = self.channels.lock().expect("im event hub lock poisoned");
        if let Some(sender) = channels.get(normalized_user_id.as_str()) {
            return sender.clone();
        }
        let (sender, _) = broadcast::channel(1024);
        channels.insert(normalized_user_id, sender.clone());
        sender
    }

    pub fn subscribe(&self, user_id: &str) -> broadcast::Receiver<Value> {
        self.sender(user_id).subscribe()
    }

    pub fn publish_to_user(&self, user_id: &str, payload: Value) {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return;
        }
        let _ = self.sender(normalized_user_id).send(payload);
    }
}

pub type SharedImEventHub = Arc<ImEventHub>;
