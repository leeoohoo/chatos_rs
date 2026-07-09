// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::mcp_server) fn enrich_tool_schemas_for_async_planner(
    tools: &mut [Value],
    _model_configs: &[ModelConfigRecord],
) {
    let builtin_description = planner_builtin_mcp_kind_schema_description();
    let external_mcp_description = planner_external_mcp_config_schema_description();
    let skill_ids_description = planner_skill_ids_schema_description();
    for tool in tools {
        match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "required"],
                    &["title", "objective"],
                );
                remove_tool_schema_property(tool, &["inputSchema"], "anyOf");
                set_tool_property_description(
                    tool,
                    &["inputSchema", "properties", "enabled_builtin_kinds"],
                    builtin_description.clone(),
                );
                remove_planner_required_builtins_from_enum(
                    tool,
                    &["inputSchema", "properties", "enabled_builtin_kinds"],
                );
                set_tool_property_description(
                    tool,
                    &["inputSchema", "properties", "external_mcp_config_ids"],
                    external_mcp_description.clone(),
                );
                set_tool_property_description(
                    tool,
                    &["inputSchema", "properties", "skill_ids"],
                    skill_ids_description.clone(),
                );
            }
            Some("create_tasks_with_prerequisites") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "properties", "tasks", "items", "required"],
                    &["client_ref", "title", "objective"],
                );
                remove_tool_schema_property(
                    tool,
                    &["inputSchema", "properties", "tasks", "items"],
                    "anyOf",
                );
                let builtin_path = &[
                    "inputSchema",
                    "properties",
                    "tasks",
                    "items",
                    "properties",
                    "enabled_builtin_kinds",
                ];
                set_tool_property_description(tool, builtin_path, builtin_description.clone());
                remove_planner_required_builtins_from_enum(tool, builtin_path);
                set_tool_property_description(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "tasks",
                        "items",
                        "properties",
                        "external_mcp_config_ids",
                    ],
                    external_mcp_description.clone(),
                );
                set_tool_property_description(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "tasks",
                        "items",
                        "properties",
                        "skill_ids",
                    ],
                    skill_ids_description.clone(),
                );
            }
            Some("update_task") => {
                remove_tool_schema_property(
                    tool,
                    &["inputSchema", "properties", "patch", "properties"],
                    "status",
                );
                remove_tool_schema_property(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "patch",
                        "properties",
                        "mcp_config",
                        "properties",
                    ],
                    "enabled",
                );
                remove_tool_schema_property(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "patch",
                        "properties",
                        "mcp_config",
                        "properties",
                    ],
                    "init_mode",
                );
                remove_planner_required_builtins_from_enum(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "patch",
                        "properties",
                        "mcp_config",
                        "properties",
                        "enabled_builtin_kinds",
                    ],
                );
            }
            _ => {}
        }
    }
}

fn remove_planner_required_builtins_from_enum(tool: &mut Value, path: &[&str]) {
    let mut current = tool;
    for segment in path {
        let Some(object) = current.as_object_mut() else {
            return;
        };
        let Some(next) = object.get_mut(*segment) else {
            return;
        };
        current = next;
    }
    let Some(values) = current
        .get_mut("items")
        .and_then(|items| items.get_mut("enum"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    values.retain(|value| {
        value
            .as_str()
            .is_none_or(|kind| !is_planner_required_builtin_kind(kind))
    });
}

fn planner_external_mcp_config_schema_description() -> String {
    "联系人异步任务可以自由组合 builtin MCP 和用户配置的外部 MCP。TaskManager 和 AskUser 属于系统默认工具，会由后端自动带上，不需要选择。用户点名外部系统、外部平台或外部 MCP 名称时，先调用 list_external_mcp_configs 查看当前用户可用配置，匹配后把对应 id 写入 external_mcp_config_ids；如果任务还需要代码、终端、浏览器等内部能力，也同时在 enabled_builtin_kinds 里选择对应 builtin。".to_string()
}

fn planner_skill_ids_schema_description() -> String {
    "联系人异步任务可以通过 skill_ids 显式加载 Task Runner 已安装 skills。选择前必须先调用 search_installed_skills 按关键词搜索当前用户可用 skills（包括内置全局 skills 和当前用户安装的 skills），必要时调用 get_skill_detail 阅读完整说明；只能把返回的真实 id 写入 skill_ids，不要根据名称猜测或编造 ID。如果没有相关 skill，就不要传 skill_ids。".to_string()
}

fn planner_builtin_mcp_kind_schema_description() -> String {
    let mut lines = vec![
        "联系人异步任务可以自由组合 builtin MCP 和用户配置的外部 MCP。这里选择执行阶段需要的 builtin 能力，但不要选择 TaskManager 或 AskUser；这两个系统默认工具会由后端自动带上。如果任务还需要外部 MCP，同时填写 external_mcp_config_ids。"
            .to_string(),
        "硬性约束：如果选择 CodeMaintainerWrite，必须同时选择 CodeMaintainerRead；不要创建只有写入工具、没有读取工具的代码任务。"
            .to_string(),
    ];
    for value in mcp_builtin_kind_values() {
        if is_planner_required_builtin_kind(value.as_str()) {
            continue;
        }
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
