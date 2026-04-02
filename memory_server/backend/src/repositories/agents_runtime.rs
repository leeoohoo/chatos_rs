use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::db::Db;
use crate::models::{
    MemoryAgent, MemoryAgentRuntimeCommandSummary, MemoryAgentRuntimePluginSummary,
    MemoryAgentRuntimeSkillSummary, MemorySkill, MemorySkillPlugin,
};
use crate::services::skills::{
    extract_plugin_content_async, resolve_plugin_root_from_cache, resolve_skill_state_root,
};

use super::{
    agents_support::visible_user_ids_for_agent_owner, normalize_optional_text,
    skills as skills_repo,
};

pub(crate) async fn load_runtime_plugin_map(
    db: &Db,
    agent: &MemoryAgent,
) -> Result<HashMap<String, MemorySkillPlugin>, String> {
    let visible_user_ids = visible_user_ids_for_agent_owner(agent.user_id.as_str());
    let plugins = skills_repo::get_plugins_by_sources_for_user_ids(
        db,
        visible_user_ids.as_slice(),
        agent.plugin_sources.as_slice(),
    )
    .await?;
    let mut plugin_map = plugins
        .into_iter()
        .map(|plugin| (plugin.source.clone(), plugin))
        .collect::<HashMap<_, _>>();
    let plugins_root = resolve_skill_state_root(agent.user_id.as_str()).join("plugins");

    for source in &agent.plugin_sources {
        let Some(existing) = plugin_map.get(source).cloned() else {
            continue;
        };
        if !plugin_needs_refresh(&existing) {
            continue;
        }
        let Some(plugin_root) = resolve_plugin_root_from_cache(
            plugins_root.as_path(),
            existing.cache_path.as_deref(),
            existing.source.as_str(),
        ) else {
            continue;
        };
        let Ok(extracted) = extract_plugin_content_async(plugin_root).await else {
            continue;
        };
        let Some(refreshed) =
            merge_extracted_plugin_content(&existing, extracted.content, extracted.commands)
        else {
            continue;
        };
        if let Ok(saved) = skills_repo::upsert_plugin(db, refreshed).await {
            plugin_map.insert(source.clone(), saved);
        }
    }

    Ok(plugin_map)
}

pub(crate) fn build_runtime_plugins(
    agent: &MemoryAgent,
    plugin_map: &HashMap<String, MemorySkillPlugin>,
) -> Vec<MemoryAgentRuntimePluginSummary> {
    agent
        .plugin_sources
        .iter()
        .filter_map(|source| plugin_map.get(source))
        .map(|plugin| MemoryAgentRuntimePluginSummary {
            source: plugin.source.clone(),
            name: plugin.name.clone(),
            category: plugin.category.clone(),
            description: plugin.description.clone(),
            content_summary: normalize_optional_text(plugin.description.as_deref()),
            updated_at: Some(plugin.updated_at.clone()),
        })
        .collect::<Vec<_>>()
}

pub(crate) fn build_runtime_commands(
    agent: &MemoryAgent,
    plugin_map: &HashMap<String, MemorySkillPlugin>,
) -> Vec<MemoryAgentRuntimeCommandSummary> {
    let mut runtime_commands = Vec::new();
    let mut seen_command_keys = HashSet::new();

    for plugin_source in &agent.plugin_sources {
        let Some(plugin) = plugin_map.get(plugin_source.as_str()) else {
            continue;
        };
        let mut commands = plugin.commands.clone();
        commands.sort_by(|left, right| {
            let left_key = left.source_path.trim().to_string();
            let right_key = right.source_path.trim().to_string();
            left_key
                .cmp(&right_key)
                .then_with(|| left.name.trim().cmp(right.name.trim()))
        });

        for command in commands {
            let source_path = command.source_path.trim().to_string();
            if source_path.is_empty() {
                continue;
            }
            let dedup_key = format!("{}::{}", plugin.source, source_path);
            if !seen_command_keys.insert(dedup_key) {
                continue;
            }
            let command_ref = format!("CMD{}", runtime_commands.len() + 1);
            runtime_commands.push(MemoryAgentRuntimeCommandSummary {
                command_ref,
                name: command_display_name(command.name.as_str(), source_path.as_str()),
                description: normalize_optional_text(command.description.as_deref())
                    .or_else(|| infer_description_from_markdown(command.content.as_str())),
                argument_hint: normalize_optional_text(command.argument_hint.as_deref()),
                plugin_source: plugin.source.clone(),
                source_path,
                content: command.content.trim().to_string(),
                updated_at: Some(plugin.updated_at.clone()),
            });
        }
    }

    runtime_commands
}

pub(crate) async fn build_runtime_skills(
    db: &Db,
    agent: &MemoryAgent,
) -> Result<Vec<MemoryAgentRuntimeSkillSummary>, String> {
    let visible_user_ids = visible_user_ids_for_agent_owner(agent.user_id.as_str());
    let skills = skills_repo::list_skills_by_ids(
        db,
        visible_user_ids.as_slice(),
        agent.skill_ids.as_slice(),
    )
    .await?;
    Ok(build_runtime_skills_from_sources(agent, skills))
}

fn build_runtime_skills_from_sources(
    agent: &MemoryAgent,
    skills: Vec<MemorySkill>,
) -> Vec<MemoryAgentRuntimeSkillSummary> {
    let skill_map = skills
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();
    let inline_skill_map = agent
        .skills
        .iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();
    let mut added_inline_skill_ids = HashSet::new();
    let mut runtime_skills = Vec::new();

    for skill_id in &agent.skill_ids {
        if let Some(skill) = skill_map.get(skill_id) {
            runtime_skills.push(MemoryAgentRuntimeSkillSummary {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: normalize_optional_text(skill.description.as_deref())
                    .or_else(|| infer_description_from_markdown(skill.content.as_str())),
                plugin_source: Some(skill.plugin_source.clone()),
                source_type: "skill_center".to_string(),
                source_path: Some(skill.source_path.clone()),
                updated_at: Some(skill.updated_at.clone()),
            });
            continue;
        }

        if let Some(skill) = inline_skill_map.get(skill_id) {
            added_inline_skill_ids.insert(skill.id.clone());
            runtime_skills.push(MemoryAgentRuntimeSkillSummary {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: None,
                plugin_source: None,
                source_type: "inline".to_string(),
                source_path: None,
                updated_at: Some(agent.updated_at.clone()),
            });
        }
    }

    for skill in &agent.skills {
        if added_inline_skill_ids.contains(&skill.id) {
            continue;
        }
        runtime_skills.push(MemoryAgentRuntimeSkillSummary {
            id: skill.id.clone(),
            name: skill.name.clone(),
            description: None,
            plugin_source: None,
            source_type: "inline".to_string(),
            source_path: None,
            updated_at: Some(agent.updated_at.clone()),
        });
    }

    runtime_skills
}

fn plugin_needs_refresh(plugin: &MemorySkillPlugin) -> bool {
    let content_missing = plugin
        .content
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .is_none();
    let commands_missing = plugin.commands.is_empty();
    let command_metadata_missing = plugin.commands.iter().any(|command| {
        let description_missing = command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        let argument_hint_missing = command
            .argument_hint
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        description_missing || argument_hint_missing
    });

    content_missing || commands_missing || command_metadata_missing
}

fn merge_extracted_plugin_content(
    existing: &MemorySkillPlugin,
    extracted_content: Option<String>,
    extracted_commands: Vec<crate::models::MemorySkillPluginCommand>,
) -> Option<MemorySkillPlugin> {
    let mut refreshed = existing.clone();
    let mut changed = false;

    let content_missing = refreshed
        .content
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .is_none();
    if content_missing {
        if let Some(content) = extracted_content
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            refreshed.content = Some(content.to_string());
            changed = true;
        }
    }

    let commands_missing = refreshed.commands.is_empty();
    let command_metadata_missing = refreshed.commands.iter().any(|command| {
        let description_missing = command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        let argument_hint_missing = command
            .argument_hint
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        description_missing || argument_hint_missing
    });
    if (commands_missing || command_metadata_missing) && !extracted_commands.is_empty() {
        refreshed.commands = extracted_commands;
        refreshed.command_count = refreshed.commands.len().min(i64::MAX as usize) as i64;
        changed = true;
    }

    if changed {
        Some(refreshed)
    } else {
        None
    }
}

fn command_display_name(raw_name: &str, source_path: &str) -> String {
    let normalized_name = raw_name.trim();
    if !normalized_name.is_empty() {
        return normalized_name.to_string();
    }

    let normalized_path = source_path.trim();
    if normalized_path.is_empty() {
        return "unnamed-command".to_string();
    }

    Path::new(normalized_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_path.to_string())
}

fn parse_description_from_markdown_frontmatter(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("---\n") && !trimmed.starts_with("---\r\n") {
        return None;
    }
    let mut lines = trimmed.lines();
    if lines.next().map(str::trim) != Some("---") {
        return None;
    }
    for line in lines {
        let normalized = line.trim();
        if normalized == "---" {
            break;
        }
        let Some((key, value)) = normalized.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("description") {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

fn parse_description_from_leading_table(raw: &str) -> Option<String> {
    let lines = raw.lines().collect::<Vec<_>>();
    if lines.len() < 2 {
        return None;
    }
    let mut start = 0usize;
    while start < lines.len() && lines[start].trim().is_empty() {
        start += 1;
    }
    if start + 1 >= lines.len() {
        return None;
    }
    let header = lines[start].trim();
    let separator = lines[start + 1].trim();
    if !header.contains('|') || !separator.contains('|') {
        return None;
    }
    let is_separator = separator
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .all(|cell| !cell.is_empty() && cell.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '));
    if !is_separator {
        return None;
    }

    let mut rows = vec![header];
    for line in lines.iter().skip(start + 2) {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains('|') {
            break;
        }
        rows.push(trimmed);
    }
    for row in rows {
        let cells = row
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if cells.len() < 2 {
            continue;
        }
        if !cells[0].eq_ignore_ascii_case("description") {
            continue;
        }
        if cells[1].is_empty() {
            return None;
        }
        return Some(cells[1].to_string());
    }
    None
}

fn parse_first_body_paragraph(raw: &str) -> Option<String> {
    let mut in_code_block = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#')
            || trimmed.starts_with('|')
            || trimmed.starts_with('-')
            || trimmed.starts_with('*')
            || trimmed
                .chars()
                .all(|ch| ch == '-' || ch == ':' || ch == '|')
        {
            continue;
        }
        return Some(trimmed.to_string());
    }
    None
}

fn infer_description_from_markdown(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    parse_description_from_markdown_frontmatter(trimmed)
        .or_else(|| parse_description_from_leading_table(trimmed))
        .or_else(|| parse_first_body_paragraph(trimmed))
        .and_then(|value| normalize_optional_text(Some(value.as_str())))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::models::{MemoryAgentSkill, MemorySkillPluginCommand};

    use super::*;

    fn sample_agent() -> MemoryAgent {
        MemoryAgent {
            id: "agent-1".to_string(),
            user_id: "user-1".to_string(),
            name: "Agent".to_string(),
            description: None,
            category: None,
            model_config_id: Some("model-1".to_string()),
            role_definition: "role".to_string(),
            plugin_sources: vec!["plugin-a".to_string()],
            skills: vec![MemoryAgentSkill {
                id: "inline-skill".to_string(),
                name: "Inline Skill".to_string(),
                content: "inline body".to_string(),
            }],
            skill_ids: vec!["inline-skill".to_string()],
            default_skill_ids: vec![],
            mcp_policy: None,
            project_policy: None,
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        }
    }

    fn sample_plugin(commands: Vec<MemorySkillPluginCommand>) -> MemorySkillPlugin {
        MemorySkillPlugin {
            id: "plugin-1".to_string(),
            user_id: "user-1".to_string(),
            source: "plugin-a".to_string(),
            name: "Plugin A".to_string(),
            category: Some("tool".to_string()),
            description: Some("Plugin desc".to_string()),
            version: Some("1.0.0".to_string()),
            repository: None,
            branch: None,
            cache_path: Some("plugin-a".to_string()),
            content: Some("plugin content".to_string()),
            commands,
            command_count: 0,
            installed: true,
            discoverable_skills: 0,
            installed_skill_count: 0,
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn infer_description_prefers_frontmatter() {
        let markdown = r#"---
description: "frontmatter desc"
---

# Title

Body paragraph
"#;

        assert_eq!(
            infer_description_from_markdown(markdown),
            Some("frontmatter desc".to_string())
        );
    }

    #[test]
    fn command_display_name_falls_back_to_source_path_stem() {
        assert_eq!(
            command_display_name("   ", "tools/run-build.md"),
            "run-build".to_string()
        );
        assert_eq!(command_display_name("   ", "   "), "unnamed-command".to_string());
    }

    #[test]
    fn build_runtime_commands_deduplicates_same_plugin_path() {
        let agent = sample_agent();
        let plugin = sample_plugin(vec![
            MemorySkillPluginCommand {
                name: "".to_string(),
                source_path: "commands/run.md".to_string(),
                description: None,
                argument_hint: None,
                content: "First paragraph".to_string(),
            },
            MemorySkillPluginCommand {
                name: "Duplicate".to_string(),
                source_path: "commands/run.md".to_string(),
                description: Some("dup".to_string()),
                argument_hint: None,
                content: "Should be deduped".to_string(),
            },
        ]);
        let plugin_map = HashMap::from([(plugin.source.clone(), plugin)]);

        let commands = build_runtime_commands(&agent, &plugin_map);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_ref, "CMD1".to_string());
        assert_eq!(commands[0].name, "run".to_string());
        assert_eq!(commands[0].description, Some("First paragraph".to_string()));
    }

    #[test]
    fn build_runtime_skills_keeps_inline_skill_when_center_skill_missing() {
        let agent = sample_agent();

        let skills = build_runtime_skills_from_sources(&agent, Vec::new());

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "inline-skill".to_string());
        assert_eq!(skills[0].source_type, "inline".to_string());
        assert_eq!(skills[0].updated_at, Some(agent.updated_at));
    }
}
