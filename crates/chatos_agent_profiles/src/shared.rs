// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{ContextStrategy, LocalAgentRun, UserInteractionQuestion};
use serde_json::{json, Value};

pub(crate) fn ask_user_schema(name: &str, description: &str) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string"},
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "option_id": {"type": "string"},
                            "label": {"type": "string"},
                            "description": {"type": ["string", "null"]}
                        },
                        "required": ["option_id", "label", "description"],
                        "additionalProperties": false
                    }
                },
                "image_references": {
                    "type": "array",
                    "items": {"type": "string"}
                },
                "details": {}
            },
            "required": ["prompt", "options", "image_references"],
            "additionalProperties": false
        }
    })
}

pub(crate) fn validate_ask_user_arguments(arguments: Value) -> Result<Value, String> {
    let question: UserInteractionQuestion = serde_json::from_value(arguments).map_err(|error| {
        format!("Ask User arguments do not match the visual question contract: {error}")
    })?;
    question
        .validate()
        .map_err(|error| format!("Ask User arguments are invalid: {error}"))?;
    serde_json::to_value(question)
        .map_err(|error| format!("failed to serialize Ask User arguments: {error}"))
}

pub(crate) fn parse_tool_arguments(call: &Value) -> Result<Value, String> {
    let arguments = call.get("arguments").cloned().unwrap_or(Value::Null);
    match arguments {
        Value::String(text) => serde_json::from_str(&text)
            .map_err(|error| format!("tool arguments are invalid JSON: {error}")),
        Value::Object(_) => Ok(arguments),
        _ => Err("tool arguments must be a JSON object".to_string()),
    }
}

pub(crate) fn validate_context_strategy(
    run: &LocalAgentRun,
    native_compaction_threshold: Option<u64>,
    memory_engine_active_threshold: Option<u64>,
    maximum_summary_attempts: u8,
) -> Result<(), String> {
    match run.context_strategy {
        ContextStrategy::ProviderNative
            if native_compaction_threshold.is_some()
                && memory_engine_active_threshold.is_none()
                && maximum_summary_attempts == 0 =>
        {
            Ok(())
        }
        ContextStrategy::MemoryEngine
            if native_compaction_threshold.is_none()
                && memory_engine_active_threshold.is_some()
                && maximum_summary_attempts > 0 =>
        {
            Ok(())
        }
        _ => Err("profile context settings do not match the frozen strategy".to_string()),
    }
}
