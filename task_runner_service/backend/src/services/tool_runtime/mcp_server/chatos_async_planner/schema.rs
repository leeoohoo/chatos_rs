// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::mcp_server) fn enrich_tool_schemas_for_async_planner(tools: &mut [Value]) {
    for tool in tools {
        match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "required"],
                    &[
                        "title",
                        "objective",
                        "requires_execution",
                        "enabled_builtin_kinds",
                    ],
                );
                remove_tool_schema_property(tool, &["inputSchema"], "anyOf");
            }
            Some("create_tasks_with_prerequisites") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "properties", "tasks", "items", "required"],
                    &[
                        "client_ref",
                        "title",
                        "objective",
                        "requires_execution",
                        "enabled_builtin_kinds",
                    ],
                );
                remove_tool_schema_property(
                    tool,
                    &["inputSchema", "properties", "tasks", "items"],
                    "anyOf",
                );
            }
            Some("update_task") => {
                remove_tool_schema_property(
                    tool,
                    &["inputSchema", "properties", "patch", "properties"],
                    "status",
                );
            }
            _ => {}
        }
    }
}
