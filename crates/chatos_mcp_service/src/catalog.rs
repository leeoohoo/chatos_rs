// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::Value;

pub fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("name").and_then(Value::as_str)
}

pub fn sort_tools_by_name(mut tools: Vec<Value>) -> Vec<Value> {
    tools.sort_by(|left, right| {
        let left_name = tool_name(left).unwrap_or("");
        let right_name = tool_name(right).unwrap_or("");
        left_name.cmp(right_name)
    });
    tools
}

pub fn tool_name_set(tools: &[Value]) -> HashSet<String> {
    tools
        .iter()
        .filter_map(tool_name)
        .map(ToOwned::to_owned)
        .collect()
}

pub fn contains_tool_name(tools: &[Value], name: &str) -> bool {
    tools.iter().any(|tool| tool_name(tool) == Some(name))
}
