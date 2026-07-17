// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::local_runtime::storage::LocalAskUserPromptRecord;

pub(super) fn prompt_response(record: &LocalAskUserPromptRecord) -> Value {
    json!({
        "id": record.id,
        "conversation_id": record.session_id,
        "conversation_turn_id": record.turn_id,
        "tool_call_id": record.tool_call_id,
        "kind": record.kind,
        "status": record.status,
        "prompt": parse_json(record.prompt_json.as_str()),
        "response": record.response_json.as_deref().map(parse_json),
        "expires_at": record.expires_at,
        "source": "local_connector",
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

fn parse_json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}
