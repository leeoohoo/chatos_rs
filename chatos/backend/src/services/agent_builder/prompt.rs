// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::chatos_agent_types::ChatosAgentDto;

use super::{truncate_text, NormalizedRequest};

pub(super) fn build_plain_system_prompt() -> String {
    [
        "你是 Chatos 内部的 AI 智能体创建器。",
        "下面会直接给你参考 agent。",
        "请输出一个紧凑 JSON 对象，字段遵循 create_memory_agent 的参数结构。",
        "规则：只允许输出当前 Agent 自身的 inline skills；不要输出 plugin_sources、MCP id、内部服务或执行位置，不要引用外部 skill_ids；工具能力由程序根据 Agent Binding 和 Project Context 注入；不要输出 markdown。",
    ]
    .join("\n")
}

pub(super) fn build_plain_user_prompt(
    request: &NormalizedRequest,
    agents: &[ChatosAgentDto],
) -> String {
    let payload = json!({
        "request": {
            "target_user_id": request.scope_user_id,
            "requirement": request.requirement,
            "explicit_name": request.name,
            "explicit_category": request.category,
            "explicit_description": request.description,
            "explicit_role_definition": request.role_definition,
            "requested_inline_skill_ids": request.skill_ids,
            "skill_prompts": request.skill_prompts,
            "enabled": request.enabled,
            "project_policy": {
                "project_id": request.project_id,
                "project_root": request.project_root,
            }
        },
        "reference_agents": build_agent_index(agents),
        "skill_selection_policy": {
            "inline_skills_only": true,
            "legacy_plugin_sources_retired": true,
            "plugins_selected_per_conversation_or_task": true,
            "mcp_selection_owned_by_program": true,
        }
    });

    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
}

fn build_agent_index(agents: &[ChatosAgentDto]) -> Vec<Value> {
    agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "category": agent.category,
                "description": agent.description.as_deref().map(|value| truncate_text(value, 160)),
                "inline_skills": agent.skills.iter().map(|skill| json!({
                    "id": skill.id,
                    "name": skill.name,
                    "content_preview": truncate_text(skill.content.as_str(), 160),
                })).collect::<Vec<_>>(),
                "role_definition_preview": truncate_text(agent.role_definition.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
}
