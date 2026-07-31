// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use crate::models::agent::Agent;
use crate::models::chatos_agent_types::{
    ChatosAgentRuntimeContextDto, ChatosAgentRuntimeSkillSummaryDto,
};

pub(super) async fn build_agent_runtime_context(
    agent: Agent,
) -> Result<ChatosAgentRuntimeContextDto, String> {
    let runtime_skills = build_runtime_skills(&agent);
    let runtime_skill_ids = runtime_skills
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();

    Ok(ChatosAgentRuntimeContextDto {
        agent_id: agent.id.clone(),
        user_id: agent.user_id.clone(),
        name: agent.name.clone(),
        description: agent.description.clone(),
        category: agent.category.clone(),
        role_definition: agent.role_definition.clone(),
        task_runner_agent_account_id: agent.task_runner_agent_account_id.clone(),
        plugin_sources: Vec::new(),
        runtime_plugins: Vec::new(),
        skills: super::dto_skills_from_agent(agent.skills.as_slice()),
        skill_ids: runtime_skill_ids,
        runtime_skills,
        runtime_commands: Vec::new(),
        mcp_policy: agent.mcp_policy.clone(),
        project_policy: agent.project_policy.clone(),
        updated_at: agent.updated_at.clone(),
    })
}

fn build_runtime_skills(agent: &Agent) -> Vec<ChatosAgentRuntimeSkillSummaryDto> {
    let inline_skill_map = agent
        .skills
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut added_inline = HashSet::new();
    let mut out = Vec::new();

    for skill_id in &agent.skill_ids {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentSkill;

    #[tokio::test]
    async fn legacy_plugin_and_skill_references_are_not_published_at_runtime() {
        let agent = Agent {
            id: "agent-1".to_string(),
            user_id: "user-1".to_string(),
            name: "Agent".to_string(),
            description: None,
            category: None,
            role_definition: "Help the user.".to_string(),
            task_runner_agent_account_id: None,
            plugin_sources: vec!["legacy/plugin".to_string()],
            skills: vec![AgentSkill {
                id: "inline-review".to_string(),
                name: "Review".to_string(),
                content: "Review changes carefully.".to_string(),
            }],
            skill_ids: vec!["legacy-skill".to_string(), "inline-review".to_string()],
            default_skill_ids: vec!["legacy-skill".to_string()],
            mcp_policy: None,
            project_policy: None,
            enabled: true,
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };

        let context = build_agent_runtime_context(agent)
            .await
            .expect("inline-only runtime context");

        assert!(context.plugin_sources.is_empty());
        assert!(context.runtime_plugins.is_empty());
        assert!(context.runtime_commands.is_empty());
        assert_eq!(context.skill_ids, vec!["inline-review".to_string()]);
        assert_eq!(context.runtime_skills.len(), 1);
        assert_eq!(context.runtime_skills[0].source_type, "inline");
    }
}
