// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_ai_runtime::{
    build_tool_call_execution_plan as build_shared_tool_call_execution_plan,
    build_tool_call_items as build_shared_tool_call_items,
    expand_tool_results_with_aliases as expand_shared_tool_results_with_aliases,
    ToolCallExecutionPlan,
};
use serde_json::Value;

use crate::services::agent_runtime::mcp_tool_execute::ToolResult;
use crate::services::shared_mcp_runtime::{chatos_tool_result, shared_tool_result};

pub(super) fn build_tool_call_execution_plan(tool_calls_arr: &[Value]) -> ToolCallExecutionPlan {
    build_shared_tool_call_execution_plan(tool_calls_arr)
}

pub(super) fn expand_tool_results_with_aliases(
    tool_results: &[ToolResult],
    alias_map: &HashMap<String, Vec<String>>,
) -> Vec<ToolResult> {
    let shared_results = tool_results
        .iter()
        .cloned()
        .map(shared_tool_result)
        .collect::<Vec<_>>();
    expand_shared_tool_results_with_aliases(shared_results.as_slice(), alias_map)
        .into_iter()
        .map(chatos_tool_result)
        .collect()
}

pub(super) fn build_tool_call_items(tool_calls_arr: &[Value]) -> Vec<Value> {
    build_shared_tool_call_items(tool_calls_arr)
}
