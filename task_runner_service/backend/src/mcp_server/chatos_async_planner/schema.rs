use super::*;

pub(in crate::mcp_server) fn enrich_tool_schemas_for_async_planner(
    tools: &mut [Value],
    _model_configs: &[ModelConfigRecord],
) {
    let builtin_description = planner_builtin_mcp_kind_schema_description();
    for tool in tools {
        match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "required"],
                    &["title", "objective", "enabled_builtin_kinds"],
                );
                set_tool_property_description(
                    tool,
                    &["inputSchema", "properties", "enabled_builtin_kinds"],
                    builtin_description.clone(),
                );
            }
            Some("create_tasks_with_prerequisites") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "properties", "tasks", "items", "required"],
                    &["client_ref", "title", "objective", "enabled_builtin_kinds"],
                );
                set_tool_property_description(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "tasks",
                        "items",
                        "properties",
                        "enabled_builtin_kinds",
                    ],
                    builtin_description.clone(),
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

fn planner_builtin_mcp_kind_schema_description() -> String {
    let mut lines = vec![
        "联系人异步任务必须选择至少一个 builtin MCP 能力。只勾选本次执行真正需要的能力；不确定时可先调用 list_mcp_builtin_catalog 查看说明。"
            .to_string(),
        "硬性约束：如果选择 CodeMaintainerWrite，必须同时选择 CodeMaintainerRead；不要创建只有写入工具、没有读取工具的代码任务。"
            .to_string(),
    ];
    for value in mcp_builtin_kind_values() {
        if let Some(kind) = builtin_kind_by_any(value.as_str()) {
            let guide = mcp_builtin_kind_guide(kind);
            lines.push(format!(
                "- {}: {} 使用场景：{}。能力：{}。",
                value,
                guide.description,
                guide.use_cases.join("、"),
                guide.capabilities.join("、")
            ));
        }
    }
    lines.join("\n")
}
