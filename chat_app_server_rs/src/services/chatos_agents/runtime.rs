use std::collections::{HashMap, HashSet};

use crate::models::agent::Agent;
use crate::models::chatos_agent_types::{
    ChatosAgentRuntimeCommandSummaryDto, ChatosAgentRuntimeContextDto,
    ChatosAgentRuntimePluginSummaryDto, ChatosAgentRuntimeSkillSummaryDto, ChatosSkillDto,
    ChatosSkillPluginCommandDto, ChatosSkillPluginDto,
};
use crate::services::chatos_skills;

pub(super) async fn build_agent_runtime_context(
    agent: Agent,
) -> Result<ChatosAgentRuntimeContextDto, String> {
    let visible_skills = if agent.skill_ids.is_empty() {
        Vec::new()
    } else {
        chatos_skills::list_skills(agent.user_id.as_str(), None, None, Some(5000), 0).await?
    };
    let skill_map = visible_skills
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect::<HashMap<_, _>>();

    let mut runtime_plugins = Vec::new();
    let mut runtime_commands = Vec::new();
    let mut seen_command_keys = HashSet::new();
    for plugin_source in &agent.plugin_sources {
        let plugin =
            match chatos_skills::get_skill_plugin(agent.user_id.as_str(), plugin_source.as_str())
                .await
            {
                Ok(Some(item)) => Some(item),
                Ok(None) => None,
                Err(_) => None,
            };
        if let Some(plugin) = plugin {
            runtime_plugins.push(plugin_to_runtime_plugin(&plugin));
            push_runtime_commands(
                &mut runtime_commands,
                &mut seen_command_keys,
                plugin.source.as_str(),
                plugin.commands.as_slice(),
                plugin.updated_at.as_str(),
            );
        }
    }

    let runtime_skills = build_runtime_skills(&agent, &skill_map);

    Ok(ChatosAgentRuntimeContextDto {
        agent_id: agent.id.clone(),
        user_id: agent.user_id.clone(),
        name: agent.name.clone(),
        description: agent.description.clone(),
        category: agent.category.clone(),
        role_definition: agent.role_definition.clone(),
        task_runner_agent_account_id: agent.task_runner_agent_account_id.clone(),
        plugin_sources: agent.plugin_sources.clone(),
        runtime_plugins,
        skills: super::dto_skills_from_agent(agent.skills.as_slice()),
        skill_ids: agent.skill_ids.clone(),
        runtime_skills,
        runtime_commands,
        mcp_policy: agent.mcp_policy.clone(),
        project_policy: agent.project_policy.clone(),
        updated_at: agent.updated_at.clone(),
    })
}

fn plugin_to_runtime_plugin(plugin: &ChatosSkillPluginDto) -> ChatosAgentRuntimePluginSummaryDto {
    ChatosAgentRuntimePluginSummaryDto {
        source: plugin.source.clone(),
        name: plugin.name.clone(),
        category: plugin.category.clone(),
        description: plugin.description.clone(),
        content_summary: plugin
            .description
            .clone()
            .or_else(|| plugin.content.as_deref().and_then(first_non_empty_line)),
        updated_at: Some(plugin.updated_at.clone()),
    }
}

fn push_runtime_commands(
    out: &mut Vec<ChatosAgentRuntimeCommandSummaryDto>,
    seen: &mut HashSet<String>,
    plugin_source: &str,
    commands: &[ChatosSkillPluginCommandDto],
    updated_at: &str,
) {
    let mut items = commands.to_vec();
    items.sort_by(|left, right| {
        left.source_path
            .cmp(&right.source_path)
            .then_with(|| left.name.cmp(&right.name))
    });
    for item in items {
        let key = format!("{plugin_source}::{}", item.source_path);
        if !seen.insert(key) {
            continue;
        }
        out.push(ChatosAgentRuntimeCommandSummaryDto {
            command_ref: format!("CMD{}", out.len() + 1),
            name: item.name,
            description: item.description,
            argument_hint: item.argument_hint,
            plugin_source: plugin_source.to_string(),
            source_path: item.source_path,
            content: item.content,
            updated_at: Some(updated_at.to_string()),
        });
    }
}

fn build_runtime_skills(
    agent: &Agent,
    skill_map: &HashMap<String, ChatosSkillDto>,
) -> Vec<ChatosAgentRuntimeSkillSummaryDto> {
    let inline_skill_map = agent
        .skills
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut added_inline = HashSet::new();
    let mut out = Vec::new();

    for skill_id in &agent.skill_ids {
        if let Some(skill) = skill_map.get(skill_id) {
            out.push(ChatosAgentRuntimeSkillSummaryDto {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                plugin_source: Some(skill.plugin_source.clone()),
                source_type: "skill_center".to_string(),
                source_path: Some(skill.source_path.clone()),
                updated_at: Some(skill.updated_at.clone()),
            });
            continue;
        }
        if let Some(skill) = inline_skill_map.get(skill_id.as_str()) {
            added_inline.insert(skill.id.clone());
            out.push(ChatosAgentRuntimeSkillSummaryDto {
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
        if added_inline.contains(skill.id.as_str()) {
            continue;
        }
        out.push(ChatosAgentRuntimeSkillSummaryDto {
            id: skill.id.clone(),
            name: skill.name.clone(),
            description: None,
            plugin_source: None,
            source_type: "inline".to_string(),
            source_path: None,
            updated_at: Some(agent.updated_at.clone()),
        });
    }

    out
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}
