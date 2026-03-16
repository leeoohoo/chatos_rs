use serde_json::Value;

use crate::repositories::projects;
use crate::services::memory_server_client::MemoryAgentRuntimeContextDto;

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
    base_prompt: Option<String>,
    runtime_context: Option<&MemoryAgentRuntimeContextDto>,
) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();

    if let Some(base) = normalize_optional_string(base_prompt) {
        sections.push(base);
    }

    if let Some(agent) = runtime_context {
        let mut segment = String::new();
        segment.push_str("你正在以联系人智能体身份参与对话。\n");
        segment.push_str(&format!("联系人名称：{}\n", agent.name.trim()));
        segment.push_str("角色定义：\n");
        segment.push_str(agent.role_definition.trim());

        if !agent.skills.is_empty() {
            segment.push_str("\n\n技能上下文：\n");
            for (index, skill) in agent.skills.iter().enumerate() {
                let skill_name = skill.name.trim();
                let skill_title = if skill_name.is_empty() {
                    format!("技能{}", index + 1)
                } else {
                    skill_name.to_string()
                };
                segment.push_str(&format!("### {}\n", skill_title));
                let content = skill.content.trim();
                if content.chars().count() > 3_000 {
                    let trimmed: String = content.chars().take(3_000).collect();
                    segment.push_str(trimmed.as_str());
                    segment.push_str("\n[技能内容已截断]");
                } else {
                    segment.push_str(content);
                }
                segment.push('\n');
            }
        } else if !agent.skill_ids.is_empty() {
            segment.push_str("\n\n技能引用：");
            segment.push_str(agent.skill_ids.join(", ").as_str());
        }

        sections.push(segment.trim().to_string());
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
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
