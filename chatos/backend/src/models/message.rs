// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const MESSAGE_REVISION_METADATA_KEY: &str = "_chatos_revision";

fn default_pending() -> String {
    "pending".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default = "default_pending")]
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

impl Message {
    pub fn new(session_id: String, role: String, content: String) -> Message {
        Message {
            id: Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            message_mode: None,
            message_source: None,
            summary: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            summary_status: default_pending(),
            summary_id: None,
            summarized_at: None,
            created_at: crate::core::time::now_rfc3339(),
        }
    }

    pub fn revision(&self) -> i64 {
        self.metadata
            .as_ref()
            .and_then(|value| value.get(MESSAGE_REVISION_METADATA_KEY))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(1)
    }

    pub fn set_revision(&mut self, revision: i64) {
        let revision = revision.max(1);
        let metadata = self
            .metadata
            .get_or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !metadata.is_object() {
            *metadata = Value::Object(serde_json::Map::new());
        }
        if let Some(object) = metadata.as_object_mut() {
            object.insert(
                MESSAGE_REVISION_METADATA_KEY.to_string(),
                Value::Number(revision.into()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Message;

    #[test]
    fn message_revision_defaults_and_increments_explicitly() {
        let mut message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "hello".to_string(),
        );
        assert_eq!(message.revision(), 1);

        message.set_revision(7);
        assert_eq!(message.revision(), 7);
    }
}
