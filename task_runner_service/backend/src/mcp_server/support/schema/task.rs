use serde_json::{json, Value};

use crate::models::{mcp_builtin_kind_guide, mcp_builtin_kind_values};
use chatos_mcp_runtime::builtin_kind_by_any;

use super::generic_task_model_config_description;

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

pub(crate) fn task_status_values() -> Vec<&'static str> {
    vec![
        "draft",
        "ready",
        "queued",
        "running",
        "succeeded",
        "failed",
        "blocked",
        "cancelled",
        "archived",
    ]
}

pub(crate) fn prompt_status_values() -> Vec<&'static str> {
    vec!["pending", "submitted", "cancelled", "timed_out", "failed"]
}
