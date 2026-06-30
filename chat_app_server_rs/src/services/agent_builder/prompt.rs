use serde_json::{json, Value};

use crate::models::chatos_agent_types::{ChatosAgentDto, ChatosSkillDto, ChatosSkillPluginDto};

use super::{truncate_text, NormalizedRequest};

pub(super) fn build_plain_system_prompt() -> String {
    [
        "你是 Chatos 内部的 AI 智能体创建器。",
        "下面会直接给你可用技能、插件摘要和参考 agent。",
        "请输出一个紧凑 JSON 对象，字段遵循 create_memory_agent 的参数结构。",
        "规则：优先输出 plugin_sources + 已安装 skill_ids；只有当技能中心为空，或者用户显式提供了 skill_prompts 时，才允许输出 inline skills；不要输出 markdown。",
    ]
    .join("\n")
}

pub(super) fn build_plain_user_prompt(
    request: &NormalizedRequest,
    skills: &[ChatosSkillDto],
    agents: &[ChatosAgentDto],
    plugins: &[ChatosSkillPluginDto],
) -> String {
    let payload = json!({
        "request": {
            "target_user_id": request.scope_user_id,
            "requirement": request.requirement,
            "explicit_name": request.name,
            "explicit_category": request.category,
            "explicit_description": request.description,
            "explicit_role_definition": request.role_definition,
            "preferred_skill_ids": request.skill_ids,
            "skill_prompts": request.skill_prompts,
            "enabled": request.enabled,
            "mcp_policy": {
                "enabled": request.mcp_enabled,
                "enabled_mcp_ids": request.enabled_mcp_ids,
            },
            "project_policy": {
                "project_id": request.project_id,
                "project_root": request.project_root,
            }
        },
        "visible_skill_plugins": build_plugin_index(plugins),
        "visible_skills": build_skill_index(skills),
        "reference_agents": build_agent_index(agents),
        "skill_selection_policy": {
            "visible_skill_count": skills.len(),
            "allow_inline_skills_only_when_skill_center_empty_or_explicit_prompts": true,
            "prefer_installed_skill_ids": true,
        }
    });

    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
}

fn build_skill_index(skills: &[ChatosSkillDto]) -> Vec<Value> {
    skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "description": skill.description.as_deref().map(|value| truncate_text(value, 180)),
                "plugin_source": skill.plugin_source,
                "source_path": skill.source_path,
                "content_preview": truncate_text(skill.content.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
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
                "plugin_sources": agent.plugin_sources,
                "skill_ids": agent.skill_ids,
                "default_skill_ids": agent.default_skill_ids,
                "role_definition_preview": truncate_text(agent.role_definition.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
}

fn build_plugin_index(plugins: &[ChatosSkillPluginDto]) -> Vec<Value> {
    plugins
        .iter()
        .map(|plugin| {
            json!({
                "id": plugin.id,
                "source": plugin.source,
                "name": plugin.name,
                "category": plugin.category,
                "description": plugin.description.as_deref().map(|value| truncate_text(value, 160)),
                "installed": plugin.installed,
                "discoverable_skills": plugin.discoverable_skills,
                "installed_skill_count": plugin.installed_skill_count,
            })
        })
        .collect::<Vec<_>>()
}
