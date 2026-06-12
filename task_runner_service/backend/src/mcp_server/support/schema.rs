use serde_json::{json, Value};

use crate::models::{mcp_builtin_kind_guide, mcp_builtin_kind_values, ModelConfigRecord};
use chatos_mcp_runtime::builtin_kind_by_any;

pub(crate) fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

pub(crate) fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub(crate) fn required_object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

pub(crate) fn generic_task_model_config_description(allow_clear: bool) -> String {
    let mut lines = vec![
        "指定任务默认使用的模型配置 ID。先调用 list_model_configs 查看可用模型，并优先根据 usage_scenario 选择最合适的模型。".to_string(),
        "如果当前任务不需要固定默认模型，可以省略该字段。".to_string(),
    ];
    if allow_clear {
        lines.push("如需清空已有默认模型绑定，可传空字符串。".to_string());
    }
    lines.join("\n")
}

pub(crate) fn generic_run_model_config_description() -> String {
    "指定本次运行临时覆盖使用的模型配置 ID。先调用 list_model_configs 查看可用模型，并优先根据 usage_scenario 选择；省略时沿用任务自己的默认模型配置。".to_string()
}

pub(crate) fn enrich_tool_schemas_with_model_configs(
    tools: &mut [Value],
    model_configs: &[ModelConfigRecord],
) {
    let enabled_models = model_configs
        .iter()
        .filter(|model| model.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let task_model_description = task_model_config_description_with_options(&enabled_models, false);
    let task_model_update_description =
        task_model_config_description_with_options(&enabled_models, true);
    let run_model_description = run_model_config_description_with_options(&enabled_models);

    for tool in tools {
        match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => set_tool_property_description(
                tool,
                &["inputSchema", "properties", "default_model_config_id"],
                task_model_description.clone(),
            ),
            Some("update_task") => set_tool_property_description(
                tool,
                &[
                    "inputSchema",
                    "properties",
                    "patch",
                    "properties",
                    "default_model_config_id",
                ],
                task_model_update_description.clone(),
            ),
            Some("create_tasks_with_prerequisites") => set_tool_property_description(
                tool,
                &[
                    "inputSchema",
                    "properties",
                    "tasks",
                    "items",
                    "properties",
                    "default_model_config_id",
                ],
                task_model_description.clone(),
            ),
            Some("start_task_run") => set_tool_property_description(
                tool,
                &["inputSchema", "properties", "model_config_id"],
                run_model_description.clone(),
            ),
            Some("batch_start_task_runs") => set_tool_property_description(
                tool,
                &["inputSchema", "properties", "model_config_id"],
                run_model_description.clone(),
            ),
            _ => {}
        }
    }
}

pub(crate) fn task_model_config_description_with_options(
    model_configs: &[ModelConfigRecord],
    allow_clear: bool,
) -> String {
    let mut lines = vec![generic_task_model_config_description(allow_clear)];
    if model_configs.is_empty() {
        lines.push("当前还没有启用中的模型配置。".to_string());
        return lines.join("\n");
    }
    lines.push("当前启用模型：".to_string());
    for model in model_configs {
        lines.push(format!(
            "- {}: {} ({})。使用场景：{}",
            model.id,
            model.name,
            model.model,
            model
                .usage_scenario
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("未填写")
        ));
    }
    lines.join("\n")
}

pub(crate) fn run_model_config_description_with_options(
    model_configs: &[ModelConfigRecord],
) -> String {
    let mut lines = vec![generic_run_model_config_description()];
    if model_configs.is_empty() {
        lines.push("当前还没有启用中的模型配置。".to_string());
        return lines.join("\n");
    }
    lines.push("当前启用模型：".to_string());
    for model in model_configs {
        lines.push(format!(
            "- {}: {} ({})。使用场景：{}",
            model.id,
            model.name,
            model.model,
            model
                .usage_scenario
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("未填写")
        ));
    }
    lines.join("\n")
}

pub(crate) fn set_tool_property_description(
    tool: &mut Value,
    path: &[&str],
    description: String,
) {
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
    let Some(object) = current.as_object_mut() else {
        return;
    };
    object.insert("description".to_string(), Value::String(description));
}

pub(crate) fn remove_tool_schema_property(tool: &mut Value, path: &[&str], property_name: &str) {
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
    if let Some(object) = current.as_object_mut() {
        object.remove(property_name);
    }
}

pub(crate) fn set_schema_required_fields(tool: &mut Value, path: &[&str], required: &[&str]) {
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
    *current = Value::Array(
        required
            .iter()
            .map(|value| Value::String((*value).to_string()))
            .collect(),
    );
}

pub(crate) fn create_task_schema() -> Value {
    let enabled_builtin_kinds_description = builtin_mcp_kind_schema_description();
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "minLength": 1, "description": "任务标题。" },
            "description": { "type": "string", "description": "任务背景、上下文或补充说明。" },
            "objective": { "type": "string", "minLength": 1, "description": "任务执行目标，说明任务完成时应达成什么结果。" },
            "input_payload": { "description": "任务输入数据。可以放结构化 JSON、引用信息或执行所需材料。" },
            "priority": { "type": "integer", "description": "任务优先级，数字越大优先级越高。" },
            "tags": { "type": "array", "items": { "type": "string" }, "description": "任务标签。" },
            "default_model_config_id": {
                "type": "string",
                "description": generic_task_model_config_description(false)
            },
            "schedule": { "type": "object", "description": "任务调度配置；不需要定时或延迟执行时不要传。" },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": enabled_builtin_kinds_description
            }
        },
        "required": ["title", "objective"],
        "additionalProperties": false
    })
}

pub(crate) fn update_task_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "description": { "type": "string" },
            "objective": { "type": "string" },
            "input_payload": {},
            "status": { "type": "string", "enum": task_status_values() },
            "priority": { "type": "integer" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "default_model_config_id": {
                "type": "string",
                "description": generic_task_model_config_description(true)
            },
            "schedule": { "type": "object" },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "mcp_config": task_mcp_config_schema()
        },
        "additionalProperties": false
    })
}

pub(crate) fn prerequisite_task_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "当前任务执行前必须先成功完成的真实任务 ID 列表。只能填写 list_tasks/get_task/create_task/create_tasks_with_prerequisites 返回过的真实 task_id，不能自己编造 ID；如果要同时创建新的前置任务，请使用 create_tasks_with_prerequisites 的 client_ref/prerequisite_refs。"
    })
}

pub(crate) fn create_tasks_with_prerequisites_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "client_ref": {
                            "type": "string",
                            "minLength": 1,
                            "description": "本次工具调用内的临时任务引用，例如 collect_logs。只在本次请求内有效，后端会返回真实 task_id。"
                        },
                        "title": { "type": "string", "minLength": 1 },
                        "description": { "type": "string" },
                        "objective": { "type": "string", "minLength": 1 },
                        "input_payload": {},
                        "priority": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "default_model_config_id": {
                            "type": "string",
                            "description": generic_task_model_config_description(false)
                        },
                        "schedule": { "type": "object" },
                        "enabled_builtin_kinds": {
                            "type": "array",
                            "items": builtin_mcp_kind_item_schema(),
                            "uniqueItems": true,
                            "description": builtin_mcp_kind_schema_description()
                        },
                        "prerequisite_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "引用同一次 create_tasks_with_prerequisites 请求中其它任务的 client_ref。用于新建任务之间的前置依赖，不能引用自己，不能成环。"
                        },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    },
                    "required": ["client_ref", "title", "objective"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["tasks"],
        "additionalProperties": false
    })
}

pub(crate) fn task_mcp_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "enabled": { "type": "boolean", "description": "是否启用 MCP。通常保持 true。" },
            "init_mode": {
                "type": "string",
                "enum": ["builtin_only", "full", "disabled"],
                "description": "MCP 初始化方式。任务系统通常使用 builtin_only。"
            },
            "builtin_prompt_mode": {
                "type": "string",
                "enum": ["effective", "configured"],
                "description": "MCP prompt 生成方式。通常使用 effective。"
            },
            "builtin_prompt_locale": {
                "type": "string",
                "enum": ["zh-CN", "en-US"],
                "description": "MCP prompt 语言。"
            },
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": builtin_mcp_kind_schema_description()
            }
        },
        "additionalProperties": false
    })
}

pub(crate) fn builtin_mcp_kind_item_schema() -> Value {
    json!({
        "type": "string",
        "enum": mcp_builtin_kind_values()
    })
}

pub(crate) fn builtin_mcp_kind_schema_description() -> String {
    let mut lines = vec![
        "可选的 builtin MCP 多选列表。只在任务执行确实需要对应能力时选择；不确定时可先调用 list_mcp_builtin_catalog 查看当前目录。可选值："
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

pub(crate) fn normalize_mcp_builtin_kind_names(values: Vec<String>) -> Result<Vec<String>, String> {
    let allowed = mcp_builtin_kind_values();
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kind = builtin_kind_by_any(trimmed).ok_or_else(|| {
            format!(
                "未知 builtin MCP kind: {trimmed}. 可选值: {}",
                allowed.join(", ")
            )
        })?;
        let normalized = kind.kind_name().to_string();
        if !out.iter().any(|item| item == &normalized) {
            out.push(normalized);
        }
    }
    Ok(out)
}

pub(crate) fn create_model_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "minLength": 1 },
            "provider": { "type": "string", "minLength": 1 },
            "base_url": { "type": "string", "minLength": 1 },
            "api_key": { "type": "string" },
            "model": { "type": "string", "minLength": 1 },
            "usage_scenario": {
                "type": "string",
                "description": "这个模型适合处理的任务场景，例如长文总结、代码修复、多步推理、快速分类等。"
            },
            "temperature": { "type": "number" },
            "max_output_tokens": { "type": "integer" },
            "thinking_level": { "type": "string" },
            "supports_responses": { "type": "boolean" },
            "instructions": { "type": "string" },
            "request_cwd": { "type": "string" },
            "include_prompt_cache_retention": { "type": "boolean" },
            "request_body_limit_bytes": { "type": "integer", "minimum": 1 },
            "enabled": { "type": "boolean" }
        },
        "required": ["name", "provider", "base_url", "model"],
        "additionalProperties": false
    })
}

pub(crate) fn update_model_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "provider": { "type": "string" },
            "base_url": { "type": "string" },
            "api_key": { "type": "string" },
            "model": { "type": "string" },
            "usage_scenario": {
                "type": "string",
                "description": "更新这个模型适合处理的任务场景说明。"
            },
            "temperature": { "type": "number" },
            "max_output_tokens": { "type": "integer" },
            "thinking_level": { "type": "string" },
            "supports_responses": { "type": "boolean" },
            "instructions": { "type": "string" },
            "request_cwd": { "type": "string" },
            "include_prompt_cache_retention": { "type": "boolean" },
            "request_body_limit_bytes": { "type": "integer", "minimum": 1 },
            "enabled": { "type": "boolean" }
        },
        "additionalProperties": false
    })
}

pub(crate) fn task_status_values() -> Vec<&'static str> {
    vec![
        "draft",
        "ready",
        "running",
        "succeeded",
        "failed",
        "blocked",
        "cancelled",
        "archived",
    ]
}

pub(crate) fn run_status_values() -> Vec<&'static str> {
    vec![
        "queued",
        "running",
        "succeeded",
        "failed",
        "cancelled",
        "blocked",
    ]
}

pub(crate) fn prompt_status_values() -> Vec<&'static str> {
    vec!["pending", "submitted", "cancelled", "timed_out", "failed"]
}
