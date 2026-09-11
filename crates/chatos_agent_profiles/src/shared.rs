// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{ContextStrategy, LocalAgentRun};
use serde_json::Value;

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
