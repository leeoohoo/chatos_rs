// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{MemoryRecordWriter, SaveRecordInput};
use serde_json::{Map, Value};

use crate::local_runtime::storage::{AppendLocalMessageInput, LocalDatabase};

#[derive(Clone)]
pub(crate) struct LocalChatRecordWriter {
    database: LocalDatabase,
    owner_user_id: String,
    session_id: String,
    turn_id: String,
}

impl LocalChatRecordWriter {
    pub(crate) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            database,
            owner_user_id: owner_user_id.into(),
            session_id: session_id.into(),
            turn_id: turn_id.into(),
        }
    }

    fn should_persist(input: &SaveRecordInput) -> bool {
        input.role == "tool"
            || (input.role == "assistant"
                && input.tool_calls.as_ref().is_some_and(json_value_has_items))
    }
}

#[async_trait]
impl MemoryRecordWriter for LocalChatRecordWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        if !Self::should_persist(&input) {
            return Ok(());
        }
        if input.conversation_id != self.session_id {
            return Err("local runtime record conversation does not match session".to_string());
        }
        if input.conversation_turn_id.as_deref() != Some(self.turn_id.as_str()) {
            return Err("local runtime record turn does not match active turn".to_string());
        }

        let metadata = local_record_metadata(&input);
        self.database
            .append_turn_message(AppendLocalMessageInput {
                session_id: self.session_id.clone(),
                owner_user_id: self.owner_user_id.clone(),
                turn_id: self.turn_id.clone(),
                message_id: input.message_id,
                role: input.role,
                content: input.content,
                reasoning: input.reasoning,
                tool_calls_json: input.tool_calls.map(|value| value.to_string()),
                tool_call_id: input.tool_call_id,
                metadata_json: metadata.map(|value| value.to_string()),
                created_at: input.created_at,
            })
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn local_record_metadata(input: &SaveRecordInput) -> Option<Value> {
    let mut metadata = match input.packed_metadata() {
        Some(Value::Object(values)) => values,
        Some(value) => Map::from_iter([("metadata".to_string(), value)]),
        None => Map::new(),
    };
    if input.role == "tool" {
        if let Some(structured) = input.structured_payload.clone() {
            metadata
                .entry("structured_result".to_string())
                .or_insert(structured);
        }
    }
    metadata.insert(
        "runtime_origin".to_string(),
        Value::String("local_device".to_string()),
    );
    (!metadata.is_empty()).then_some(Value::Object(metadata))
}

fn json_value_has_items(value: &Value) -> bool {
    match value {
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        _ => true,
    }
}
