// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::ModelConfigRecord;

use super::super::access::model_has_cloud_runtime_credentials;

pub(crate) fn enrich_tool_schemas_with_model_configs(
    tools: &mut [Value],
    model_configs: &[ModelConfigRecord],
) {
    let selectable_models = model_configs
        .iter()
        .filter(|model| model.enabled && model_has_cloud_runtime_credentials(model))
        .collect::<Vec<_>>();
    let schema = model_config_selection_schema(selectable_models.as_slice());
    for tool in tools {
        match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => {
                set_model_config_schema(tool, "/inputSchema/properties", schema.clone())
            }
            Some("create_tasks_with_prerequisites") | Some("create_project_execution_tasks") => {
                set_model_config_schema(
                    tool,
                    "/inputSchema/properties/tasks/items/properties",
                    schema.clone(),
                )
            }
            _ => {}
        }
    }
}

fn set_model_config_schema(tool: &mut Value, properties_pointer: &str, schema: Value) {
    let Some(properties) = tool
        .pointer_mut(properties_pointer)
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    properties.insert("default_model_config_id".to_string(), schema);
}

fn model_config_selection_schema(model_configs: &[&ModelConfigRecord]) -> Value {
    let choices = model_configs
        .iter()
        .map(|model| {
            let usage = model
                .usage_scenario
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let title = match usage {
                Some(usage) => format!("{} / {} — {}", model.name, model.model, usage),
                None => format!("{} / {}", model.name, model.model),
            };
            json!({
                "const": model.id,
                "title": title
            })
        })
        .collect::<Vec<_>>();
    let ids = model_configs
        .iter()
        .map(|model| Value::String(model.id.clone()))
        .collect::<Vec<_>>();
    let labels = choices
        .iter()
        .filter_map(|choice| choice.get("title").cloned())
        .collect::<Vec<_>>();
    let mut schema = json!({
        "type": "string",
        "minLength": 1,
        "description": "Select exactly one enabled Task Runner execution model for this task. Match the model usage scenario to the task objective. Omit this field only when Task Runner should choose automatically."
    });
    if !ids.is_empty() {
        schema["enum"] = Value::Array(ids);
        schema["oneOf"] = Value::Array(choices);
        schema["x-enum-labels"] = Value::Array(labels);
    }
    schema
}

fn thinking_level_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["none", "auto", "minimal", "low", "medium", "high", "xhigh", "max"],
        "description": "可选的默认思考等级。不要自由输入；只能从枚举中选择。省略该字段表示使用模型/供应商/运行时默认值。"
    })
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
            "thinking_level": thinking_level_schema(),
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
            "thinking_level": thinking_level_schema(),
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
