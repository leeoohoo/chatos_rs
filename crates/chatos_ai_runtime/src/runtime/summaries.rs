use serde_json::Value;

use chatos_mcp_runtime::ToolResult;

use crate::tool_call::extract_tool_call_name;

pub(super) fn summarize_tool_call_names(tool_calls: &Value, limit: usize) -> Vec<String> {
    tool_calls
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool_call| extract_tool_call_name(tool_call).map(ToOwned::to_owned))
        .take(limit)
        .collect()
}

pub(super) fn summarize_tool_result_names(
    tool_results: &[ToolResult],
    limit: usize,
) -> Vec<String> {
    tool_results
        .iter()
        .map(|result| result.name.clone())
        .take(limit)
        .collect()
}
