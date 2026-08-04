// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::ToolResult;

pub(super) fn should_persist_tool_result(_result: &ToolResult) -> bool {
    // An empty successful result is still a completed tool invocation. Dropping it
    // makes the persisted process timeline look permanently pending because the
    // assistant tool call no longer has a matching tool-result record.
    true
}

pub(super) fn normalized_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
