// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::{mcp_builtin_kind_guide, mcp_builtin_kind_values};
use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies};

use super::common::required_object_schema;

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
            "schedule": { "type": "object", "description": "任务调度配置；不需要定时或延迟执行时不要传。" },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": enabled_builtin_kinds_description
            },
            "external_mcp_config_ids": external_mcp_config_ids_schema(),
            "skill_ids": skill_ids_schema()
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
                        "schedule": { "type": "object" },
                        "enabled_builtin_kinds": {
                            "type": "array",
                            "items": builtin_mcp_kind_item_schema(),
                            "uniqueItems": true,
                            "description": builtin_mcp_kind_schema_description()
                        },
                        "external_mcp_config_ids": external_mcp_config_ids_schema(),
                        "skill_ids": skill_ids_schema(),
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
            },
            "external_mcp_config_ids": external_mcp_config_ids_schema(),
            "skill_ids": skill_ids_schema()
        },
        "additionalProperties": false
    })
}

pub(crate) fn external_mcp_config_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "需要在任务执行时加载的外部 MCP 配置 ID 列表。只能填写 list_external_mcp_configs 返回的 id；如果用户点名某个外部 MCP、外部平台或外部系统，必须先调用 list_external_mcp_configs 匹配名称并填写这里，不能用 enabled_builtin_kinds 的 builtin 能力代替；如果任务不需要外部 MCP，不要传。"
    })
}

pub(crate) fn skill_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "需要在任务执行时加载的 Task Runner Skill ID 列表。只能填写 search_installed_skills 或 get_skill_detail 返回的真实 id；不要凭名称编造 ID。选择 skill 前先用 search_installed_skills 按关键词搜索当前用户可用的已安装 skills（包含内置全局 skills 和当前用户安装的 skills），必要时再用 get_skill_detail 查看完整说明。用户上传或点名 PDF、DOCX、表格、PPT、图片等复杂文件，或当前对话里的文件正文抽取失败/乱码/不完整时，必须优先搜索并绑定对应文件类型 skill，再创建读取/分析任务。如果任务不需要额外 skill，不要传。"
    })
}

pub(crate) fn search_installed_skills_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "keyword": {
                "type": "string",
                "description": "按任务领域、文件类型、工具名或能力关键词搜索，例如 docx、pdf、spreadsheet、browser、image、review。为空时返回当前用户可用的常用已安装 skills。"
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "description": "最多返回多少条，默认 20。"
            }
        },
        "additionalProperties": false
    })
}

pub(crate) fn get_skill_detail_schema() -> Value {
    required_object_schema(
        json!({
            "skill_id": {
                "type": "string",
                "minLength": 1,
                "description": "search_installed_skills 或任务记录里返回的真实 Skill ID。"
            }
        }),
        &["skill_id"],
    )
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
        "约束：如果选择 CodeMaintainerWrite，必须同时选择 CodeMaintainerRead；后端也会自动补齐这个依赖。"
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
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    Ok(complete_builtin_kind_dependencies(out)
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect())
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
