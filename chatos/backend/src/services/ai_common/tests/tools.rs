// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn build_tool_result_metadata_keeps_tool_flags() {
    let result = ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "mcp.query".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: Some("turn_abc".to_string()),
        content: "ok".to_string(),
        result: Some(serde_json::json!({"answer": 42})),
    };

    let metadata = build_tool_result_metadata(&result);

    assert_eq!(
        metadata.get("toolName").and_then(|value| value.as_str()),
        Some("mcp.query")
    );
    assert_eq!(
        metadata.get("success").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        metadata.get("isError").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        metadata
            .get("conversation_turn_id")
            .and_then(|value| value.as_str()),
        Some("turn_abc")
    );
    assert_eq!(
        metadata
            .get("structured_result")
            .and_then(|value| value.get("answer"))
            .and_then(|value| value.as_i64()),
        Some(42)
    );
}
