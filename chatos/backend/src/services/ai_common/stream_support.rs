// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::mcp_tools::ToolResult;

pub(crate) fn build_tool_result_metadata(result: &ToolResult) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("toolName".to_string(), Value::String(result.name.clone()));
    map.insert("success".to_string(), Value::Bool(result.success));
    map.insert("isError".to_string(), Value::Bool(result.is_error));
    if let Some(structured_result) = result.result.clone() {
        map.insert("structured_result".to_string(), structured_result);
    }
    if let Some(turn_id) = result
        .conversation_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn_id.to_string()),
        );
    }
    Value::Object(map)
}
