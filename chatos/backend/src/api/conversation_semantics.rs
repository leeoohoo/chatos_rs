// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub fn rewrite_session_keys_to_conversation(value: Value) -> Value {
    fn walk(value: Value) -> Value {
        match value {
            Value::Array(items) => Value::Array(items.into_iter().map(walk).collect()),
            Value::Object(mut map) => {
                if let Some(session_id) = map.remove("session_id") {
                    map.insert("conversation_id".to_string(), session_id);
                }
                if let Some(session_id) = map.remove("sessionId") {
                    map.insert("conversationId".to_string(), session_id);
                }
                if let Some(session_title) = map.remove("session_title") {
                    map.insert("conversation_title".to_string(), session_title);
                }
                if let Some(session_title) = map.remove("sessionTitle") {
                    map.insert("conversationTitle".to_string(), session_title);
                }
                let keys: Vec<String> = map.keys().cloned().collect();
                for key in keys {
                    if let Some(child) = map.remove(&key) {
                        map.insert(key, walk(child));
                    }
                }
                Value::Object(map)
            }
            other => other,
        }
    }

    walk(value)
}
