// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::LocalAgentRunRecord;
use serde_json::Value;

#[derive(Debug, Clone)]
pub(super) struct SourceConversationContext {
    pub(super) conversation_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) remote_connection_id: Option<String>,
}

pub(super) fn source_attachments(parent: &LocalAgentRunRecord) -> Result<Vec<Value>, String> {
    let Some(attachments) = parent.input.get("attachments") else {
        return Ok(Vec::new());
    };
    attachments
        .as_array()
        .cloned()
        .ok_or_else(|| "source conversation attachments must be an array".to_string())
}

pub(super) fn source_conversation_context(
    parent: &LocalAgentRunRecord,
) -> SourceConversationContext {
    SourceConversationContext {
        conversation_id: input_string(&parent.input, "conversation_id")
            .or_else(|| input_string(&parent.input, "source_conversation_id")),
        turn_id: input_string(&parent.input, "turn_id")
            .or_else(|| input_string(&parent.input, "source_turn_id")),
        remote_connection_id: parent
            .input
            .pointer("/runtime_settings/remote_connection_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| input_string(&parent.input, "remote_connection_id")),
    }
}

fn input_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}
