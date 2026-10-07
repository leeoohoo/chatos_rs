// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use chatos_mcp_runtime::ToolResult;

const MEMORY_TOOL_CONTENT_MAX_CHARS: usize = 262_144;
const OMITTED_BINARY_PREFIX: &str = "[binary omitted from memory";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeRecordOptions {
    pub persist_assistant_records: bool,
    pub persist_tool_records: bool,
    pub assistant_message_id: Option<String>,
    pub tool_message_id_prefix: Option<String>,
    pub assistant_message_mode: Option<String>,
    pub assistant_message_source: Option<String>,
    pub assistant_metadata: Option<Value>,
    pub tool_message_mode: Option<String>,
    pub tool_message_source: Option<String>,
    pub tool_metadata: Option<Value>,
}

impl RuntimeRecordOptions {
    pub fn persist_all() -> Self {
        Self {
            persist_assistant_records: true,
            persist_tool_records: true,
            ..Self::default()
        }
    }

    pub fn with_persist_assistant_records(mut self, persist: bool) -> Self {
        self.persist_assistant_records = persist;
        self
    }

    pub fn with_persist_tool_records(mut self, persist: bool) -> Self {
        self.persist_tool_records = persist;
        self
    }

    pub fn with_assistant_message_id(mut self, message_id: impl Into<String>) -> Self {
        self.assistant_message_id = Some(message_id.into());
        self
    }

    pub fn with_tool_message_id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.tool_message_id_prefix = Some(prefix.into());
        self
    }

    pub fn with_assistant_message_mode(mut self, mode: impl Into<String>) -> Self {
        self.assistant_message_mode = Some(mode.into());
        self
    }

    pub fn with_assistant_message_source(mut self, source: impl Into<String>) -> Self {
        self.assistant_message_source = Some(source.into());
        self
    }

    pub fn with_assistant_metadata(mut self, metadata: Value) -> Self {
        self.assistant_metadata = Some(metadata);
        self
    }

    pub fn with_tool_message_mode(mut self, mode: impl Into<String>) -> Self {
        self.tool_message_mode = Some(mode.into());
        self
    }

    pub fn with_tool_message_source(mut self, source: impl Into<String>) -> Self {
        self.tool_message_source = Some(source.into());
        self
    }

    pub fn with_tool_metadata(mut self, metadata: Value) -> Self {
        self.tool_metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveRecordInput {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub response_id: Option<String>,
    pub response_status: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: Option<String>,
}

impl SaveRecordInput {
    pub fn message(
        conversation_id: impl Into<String>,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            role: role.into(),
            content: content.into(),
            ..Self::default()
        }
    }

    pub fn user_message(conversation_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::message(conversation_id, "user", content)
    }

    pub fn assistant_message(
        conversation_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::message(conversation_id, "assistant", content)
    }

    pub fn with_conversation_turn_id(mut self, conversation_turn_id: impl Into<String>) -> Self {
        self.conversation_turn_id = Some(conversation_turn_id.into());
        self
    }

    pub fn with_message_id(mut self, message_id: impl Into<String>) -> Self {
        self.message_id = Some(message_id.into());
        self
    }

    pub fn with_structured_payload(mut self, structured_payload: Value) -> Self {
        self.structured_payload = Some(structured_payload);
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_message_mode(mut self, message_mode: impl Into<String>) -> Self {
        self.message_mode = Some(message_mode.into());
        self
    }

    pub fn with_message_source(mut self, message_source: impl Into<String>) -> Self {
        self.message_source = Some(message_source.into());
        self
    }

    pub fn with_created_at(mut self, created_at: impl Into<String>) -> Self {
        self.created_at = Some(created_at.into());
        self
    }

    pub fn packed_metadata(&self) -> Option<Value> {
        let mut map = metadata_object(self.metadata.clone());
        insert_non_empty(&mut map, "conversation_turn_id", &self.conversation_turn_id);
        insert_non_empty(&mut map, "message_mode", &self.message_mode);
        insert_non_empty(&mut map, "message_source", &self.message_source);
        insert_value(&mut map, "tool_calls", self.tool_calls.clone());
        insert_non_empty(&mut map, "tool_call_id", &self.tool_call_id);
        insert_non_empty(&mut map, "reasoning", &self.reasoning);
        insert_non_empty(&mut map, "response_id", &self.response_id);
        insert_non_empty(&mut map, "response_status", &self.response_status);
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map))
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveAssistantRecordInput {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub message_id: Option<String>,
    pub content: String,
    pub reasoning: Option<String>,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub tool_calls: Option<Value>,
    pub response_id: Option<String>,
    pub response_status: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: Option<String>,
}

impl From<SaveAssistantRecordInput> for SaveRecordInput {
    fn from(input: SaveAssistantRecordInput) -> Self {
        Self {
            conversation_id: input.conversation_id,
            conversation_turn_id: input.conversation_turn_id,
            message_id: input.message_id,
            role: "assistant".to_string(),
            content: input.content,
            structured_payload: input.structured_payload,
            metadata: input.metadata,
            message_mode: input.message_mode,
            message_source: input.message_source,
            tool_calls: input.tool_calls,
            tool_call_id: None,
            reasoning: input.reasoning,
            response_id: input.response_id,
            response_status: input.response_status,
            summary_status: input.summary_status,
            summary_id: input.summary_id,
            summarized_at: input.summarized_at,
            created_at: input.created_at,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveToolRecordInput {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String,
    pub success: bool,
    pub is_error: bool,
    pub is_stream: bool,
    pub structured_result: Option<Value>,
    pub metadata: Option<Value>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: Option<String>,
}

impl SaveToolRecordInput {
    pub fn from_tool_result(
        conversation_id: String,
        default_turn_id: Option<String>,
        result: &ToolResult,
    ) -> Self {
        Self {
            conversation_id,
            conversation_turn_id: result.conversation_turn_id.clone().or(default_turn_id),
            message_id: None,
            tool_call_id: result.tool_call_id.clone(),
            tool_name: result.name.clone(),
            content: result.content.clone(),
            success: result.success,
            is_error: result.is_error,
            is_stream: result.is_stream,
            structured_result: result.result.clone(),
            metadata: None,
            message_mode: None,
            message_source: None,
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: None,
        }
    }
}

impl From<SaveToolRecordInput> for SaveRecordInput {
    fn from(input: SaveToolRecordInput) -> Self {
        let content = memory_safe_tool_content(input.content.as_str());
        let structured_result = input.structured_result.map(memory_safe_tool_value);
        let mut metadata = metadata_object(input.metadata);
        insert_non_empty(&mut metadata, "toolName", &Some(input.tool_name));
        metadata.insert("success".to_string(), Value::Bool(input.success));
        metadata.insert("isError".to_string(), Value::Bool(input.is_error));
        metadata.insert("isStream".to_string(), Value::Bool(input.is_stream));

        Self {
            conversation_id: input.conversation_id,
            conversation_turn_id: input.conversation_turn_id,
            message_id: input.message_id,
            role: "tool".to_string(),
            content,
            structured_payload: structured_result,
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(Value::Object(metadata))
            },
            message_mode: input.message_mode,
            message_source: input.message_source,
            tool_calls: None,
            tool_call_id: Some(input.tool_call_id),
            reasoning: None,
            response_id: None,
            response_status: None,
            summary_status: input.summary_status,
            summary_id: input.summary_id,
            summarized_at: input.summarized_at,
            created_at: input.created_at,
        }
    }
}

fn memory_safe_tool_content(content: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(content) {
        let value = memory_safe_tool_value(value);
        if let Ok(encoded) = serde_json::to_string(&value) {
            return bound_memory_tool_content(encoded.as_str());
        }
    }
    bound_memory_tool_content(content)
}

fn bound_memory_tool_content(content: &str) -> String {
    if content.chars().count() <= MEMORY_TOOL_CONTENT_MAX_CHARS {
        return content.to_string();
    }
    let mut bounded = content
        .chars()
        .take(MEMORY_TOOL_CONTENT_MAX_CHARS)
        .collect::<String>();
    bounded.push_str("\n[tool output truncated for memory persistence]");
    bounded
}

fn memory_safe_tool_value(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(memory_safe_tool_value).collect())
        }
        Value::Object(mut map) => {
            let binary_kind = map
                .get("type")
                .and_then(Value::as_str)
                .filter(|kind| matches!(*kind, "image" | "audio"));
            let mime_type = map
                .get("mimeType")
                .or_else(|| map.get("mime_type"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(data) = map.get("data").and_then(Value::as_str) {
                let is_binary = binary_kind.is_some()
                    || mime_type.as_deref().is_some_and(|value| {
                        value.starts_with("image/") || value.starts_with("audio/")
                    });
                if is_binary {
                    let kind = binary_kind.unwrap_or("binary");
                    let mime = mime_type.as_deref().unwrap_or("application/octet-stream");
                    map.insert(
                        "data".to_string(),
                        Value::String(format!(
                            "{OMITTED_BINARY_PREFIX}: type={kind}, mime={mime}, encoded_chars={}]",
                            data.len()
                        )),
                    );
                }
            }
            for value in map.values_mut() {
                let original = std::mem::take(value);
                *value = memory_safe_tool_value(original);
            }
            Value::Object(map)
        }
        Value::String(value)
            if value.len() > 4_096 && value.starts_with("data:") && value.contains(";base64,") =>
        {
            let mime = value
                .strip_prefix("data:")
                .and_then(|value| value.split_once(';'))
                .map(|(mime, _)| mime)
                .unwrap_or("application/octet-stream");
            Value::String(format!(
                "{OMITTED_BINARY_PREFIX}: mime={mime}, encoded_chars={}]",
                value.len()
            ))
        }
        other => other,
    }
}

#[async_trait]
pub trait MemoryRecordWriter: Send + Sync {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String>;

    async fn save_assistant_record(&self, input: SaveAssistantRecordInput) -> Result<(), String> {
        self.save_record(input.into()).await
    }

    async fn save_tool_record(&self, input: SaveToolRecordInput) -> Result<(), String> {
        self.save_record(input.into()).await
    }

    async fn save_tool_records(&self, inputs: Vec<SaveToolRecordInput>) -> Result<(), String> {
        for input in inputs {
            self.save_tool_record(input).await?;
        }
        Ok(())
    }
}

fn metadata_object(metadata: Option<Value>) -> Map<String, Value> {
    match metadata {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = Map::new();
            map.insert("metadata".to_string(), other);
            map
        }
        None => Map::new(),
    }
}

fn insert_non_empty(map: &mut Map<String, Value>, key: &str, value: &Option<String>) {
    if let Some(value) = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}
