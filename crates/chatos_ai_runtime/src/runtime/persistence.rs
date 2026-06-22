use serde_json::Value;

use chatos_mcp_runtime::ToolResult;

pub(super) fn should_persist_tool_result(result: &ToolResult) -> bool {
    if !result.success || result.is_error || result.is_stream {
        return true;
    }

    let structured_empty_array = matches!(
        result.result.as_ref(),
        Some(Value::Array(items)) if items.is_empty()
    );
    if !structured_empty_array {
        return true;
    }

    result.content.trim() != "[]"
}

pub(super) fn normalized_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
