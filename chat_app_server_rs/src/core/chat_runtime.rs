use serde_json::Value;

use crate::repositories::projects;
use crate::services::memory_server_client::MemoryAgentRuntimeContextDto;

pub const CONTACT_SKILL_READER_TOOL_NAME: &str = "memory_skill_reader_get_skill_detail";

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn normalize_id(value: Option<String>) -> Option<String> {
    normalize_optional_string(value)
}

pub fn metadata_string(metadata: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_optional_string(cursor.as_str().map(ToOwned::to_owned))
}

pub fn metadata_bool(metadata: Option<&Value>, path: &[&str]) -> Option<bool> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_bool()
}

pub fn metadata_string_list(metadata: Option<&Value>, path: &[&str]) -> Vec<String> {
    let mut cursor = match metadata {
        Some(value) => value,
        None => return Vec::new(),
    };
    for key in path {
        let Some(next) = cursor.get(*key) else {
            return Vec::new();
        };
        cursor = next;
    }
    let Some(items) = cursor.as_array() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let Some(raw) = item.as_str() else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub fn contact_agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]))
}

pub fn project_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata_string(metadata, &["chat_runtime", "project_id"])
        .or_else(|| metadata_string(metadata, &["chat_runtime", "projectId"]))
}

pub fn project_root_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata_string(metadata, &["chat_runtime", "project_root"])
        .or_else(|| metadata_string(metadata, &["chat_runtime", "projectRoot"]))
}

pub fn mcp_enabled_from_metadata(metadata: Option<&Value>) -> Option<bool> {
    metadata_bool(metadata, &["chat_runtime", "mcp_enabled"])
        .or_else(|| metadata_bool(metadata, &["chat_runtime", "mcpEnabled"]))
}

pub fn enabled_mcp_ids_from_metadata(metadata: Option<&Value>) -> Vec<String> {
    let from_new = metadata_string_list(metadata, &["chat_runtime", "enabled_mcp_ids"]);
    if !from_new.is_empty() {
        return from_new;
    }
    metadata_string_list(metadata, &["chat_runtime", "enabledMcpIds"])
}

pub fn compose_contact_system_prompt(
    runtime_context: Option<&MemoryAgentRuntimeContextDto>,
) -> Option<String> {
    let agent = runtime_context?;
    let agent_name = agent.name.trim();
    if agent_name.is_empty() {
        return None;
    }

    let mut lines = vec![
        "你正在以联系人智能体身份参与对话。".to_string(),
        format!("联系人名称：{}", agent_name),
    ];

    if let Some(description) = agent.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!("联系人简介：{}", description));
        }
    }
    if let Some(category) = agent.category.as_deref().map(str::trim) {
        if !category.is_empty() {
            lines.push(format!("联系人分类：{}", category));
        }
    }

    lines.push(String::new());
    lines.push("角色定义：".to_string());
    lines.push(agent.role_definition.trim().to_string());
    lines.push(String::new());
    lines.push("关联插件：".to_string());

    if !agent.runtime_plugins.is_empty() {
        for (index, plugin) in agent.runtime_plugins.iter().enumerate() {
            let mut parts = vec![
                format!("plugin_source={}", plugin.source.trim()),
                format!("名称={}", plugin.name.trim()),
            ];
            if let Some(category) = plugin.category.as_deref().map(str::trim) {
                if !category.is_empty() {
                    parts.push(format!("分类={}", category));
                }
            }
            if let Some(description) = plugin.description.as_deref().map(str::trim) {
                if !description.is_empty() {
                    parts.push(format!("简介={}", description));
                }
            }
            lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
        }
    } else if !agent.plugin_sources.is_empty() {
        for (index, source) in agent.plugin_sources.iter().enumerate() {
            lines.push(format!("{}. plugin_source={}", index + 1, source.trim()));
        }
    } else {
        lines.push("无".to_string());
    }

    lines.push(String::new());
    lines.push("关联技能：".to_string());
    if !agent.runtime_skills.is_empty() {
        for (index, skill) in agent.runtime_skills.iter().enumerate() {
            let mut parts = vec![
                format!("skill_id={}", skill.id.trim()),
                format!("名称={}", skill.name.trim()),
            ];
            if let Some(plugin_source) = skill.plugin_source.as_deref().map(str::trim) {
                if !plugin_source.is_empty() {
                    parts.push(format!("plugin_source={}", plugin_source));
                }
            }
            if let Some(description) = skill.description.as_deref().map(str::trim) {
                if !description.is_empty() {
                    parts.push(format!("简介={}", description));
                }
            }
            parts.push(format!("来源类型={}", skill.source_type.trim()));
            lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
        }
    } else if !agent.skill_ids.is_empty() {
        for (index, skill_id) in agent.skill_ids.iter().enumerate() {
            lines.push(format!("{}. skill_id={}", index + 1, skill_id.trim()));
        }
    } else {
        lines.push("无".to_string());
    }

    if !agent.runtime_skills.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "如果需要查看某个 skill 的完整内容，请调用内置工具 `{}`。",
            CONTACT_SKILL_READER_TOOL_NAME
        ));
    }

    Some(lines.join("\n").trim().to_string())
}

fn normalize_path_text(raw: &str) -> String {
    let mut out = raw.trim().replace('\\', "/");
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

pub async fn resolve_project_runtime(
    user_id: Option<&str>,
    project_id: Option<String>,
    project_root: Option<String>,
) -> (Option<String>, Option<String>) {
    let mut resolved_project_id = normalize_optional_string(project_id);
    let mut resolved_project_root = normalize_optional_string(project_root);

    let Some(project_id) = resolved_project_id.clone() else {
        return (resolved_project_id, resolved_project_root);
    };

    let project = match projects::get_project_by_id(project_id.as_str()).await {
        Ok(Some(project)) => project,
        _ => {
            resolved_project_id = None;
            return (resolved_project_id, resolved_project_root);
        }
    };

    if let (Some(uid), Some(project_owner)) = (user_id, project.user_id.as_deref()) {
        if project_owner != uid {
            resolved_project_id = None;
            return (resolved_project_id, resolved_project_root);
        }
    }

    let expected_root = normalize_path_text(project.root_path.as_str());
    match resolved_project_root.clone() {
        Some(current_root) => {
            if normalize_path_text(current_root.as_str()) != expected_root {
                resolved_project_root = Some(project.root_path);
            }
        }
        None => {
            resolved_project_root = Some(project.root_path);
        }
    }

    (resolved_project_id, resolved_project_root)
}

#[cfg(test)]
mod tests {
    use super::{compose_contact_system_prompt, CONTACT_SKILL_READER_TOOL_NAME};
    use crate::services::memory_server_client::{
        MemoryAgentRuntimeContextDto, MemoryAgentRuntimePluginSummaryDto,
        MemoryAgentRuntimeSkillSummaryDto,
    };

    #[test]
    fn builds_contact_prompt_with_plugin_and_skill_summaries() {
        let prompt = compose_contact_system_prompt(Some(&MemoryAgentRuntimeContextDto {
            agent_id: "agent_1".to_string(),
            name: "小林".to_string(),
            description: Some("负责前端排障".to_string()),
            category: Some("frontend".to_string()),
            role_definition: "专注组件与状态问题".to_string(),
            plugin_sources: vec!["frontend_toolkit".to_string()],
            runtime_plugins: vec![MemoryAgentRuntimePluginSummaryDto {
                source: "frontend_toolkit".to_string(),
                name: "前端工具箱".to_string(),
                category: Some("frontend".to_string()),
                description: Some("用于组件设计和渲染排查".to_string()),
                updated_at: Some("2026-03-24T00:00:00Z".to_string()),
            }],
            skills: Vec::new(),
            skill_ids: vec!["skill_a".to_string()],
            runtime_skills: vec![MemoryAgentRuntimeSkillSummaryDto {
                id: "skill_a".to_string(),
                name: "组件排障".to_string(),
                description: Some("定位 UI 异常".to_string()),
                plugin_source: Some("frontend_toolkit".to_string()),
                source_type: "skill_center".to_string(),
                source_path: Some("skills/ui/SKILL.md".to_string()),
                updated_at: Some("2026-03-24T00:00:00Z".to_string()),
            }],
            mcp_policy: None,
            project_policy: None,
            updated_at: "2026-03-24T00:00:00Z".to_string(),
        }))
        .expect("prompt");

        assert!(prompt.contains("联系人名称：小林"));
        assert!(prompt.contains("plugin_source=frontend_toolkit"));
        assert!(prompt.contains("skill_id=skill_a"));
        assert!(prompt.contains(CONTACT_SKILL_READER_TOOL_NAME));
    }
}
