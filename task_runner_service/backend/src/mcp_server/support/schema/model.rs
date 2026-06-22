use serde_json::{json, Value};

use crate::models::ModelConfigRecord;

use super::set_tool_property_description;

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
