// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::mcp_tools::ToolResult;
use crate::core::tool_call::build_function_call_output_item;

use super::compat::cap_tool_output_for_input;

pub(super) fn build_tool_output_items(tool_results: &[ToolResult]) -> Vec<Value> {
    tool_results
        .iter()
        .map(|result| {
            let output_text = cap_tool_output_for_input(result.content.as_str());
            build_function_call_output_item(result.tool_call_id.as_str(), output_text.as_str())
        })
        .collect()
}
