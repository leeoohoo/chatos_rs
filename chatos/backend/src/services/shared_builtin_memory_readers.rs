// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;

use chatos_mcp::{
    MemoryFullPlugin, MemoryFullSkill, MemoryInlineSkill, MemoryReaderStore, MemoryRuntimeCommand,
    MemoryRuntimeContext, MemoryRuntimePlugin, MemoryRuntimeSkill,
};

use crate::services::{chatos_agents, chatos_skills};

#[derive(Debug, Clone, Default)]
pub struct ChatosMemoryReaderStore;

#[async_trait]
impl MemoryReaderStore for ChatosMemoryReaderStore {
    async fn get_agent_runtime_context(
        &self,
        agent_id: &str,
    ) -> Result<Option<MemoryRuntimeContext>, String> {
        let Some(context) = chatos_agents::get_agent_runtime_context(agent_id).await? else {
            return Ok(None);
        };
        Ok(Some(MemoryRuntimeContext {
            agent_id: context.agent_id,
            user_id: context.user_id,
            skills: context
                .skills
                .into_iter()
                .map(|skill| MemoryInlineSkill {
                    id: skill.id,
                    name: skill.name,
                    content: skill.content,
                })
                .collect(),
            skill_ids: context.skill_ids,
            runtime_skills: context
                .runtime_skills
                .into_iter()
                .map(|skill| MemoryRuntimeSkill {
                    id: skill.id,
                    name: skill.name,
                    description: skill.description,
                    plugin_source: skill.plugin_source,
                    source_type: skill.source_type,
                    source_path: skill.source_path,
                    updated_at: skill.updated_at,
                })
                .collect(),
            runtime_commands: context
                .runtime_commands
                .into_iter()
                .map(|command| MemoryRuntimeCommand {
                    command_ref: command.command_ref,
                    name: command.name,
                    description: command.description,
                    argument_hint: command.argument_hint,
                    plugin_source: command.plugin_source,
                    source_path: command.source_path,
                    content: command.content,
                    updated_at: command.updated_at,
                })
                .collect(),
            runtime_plugins: context
                .runtime_plugins
                .into_iter()
                .map(|plugin| MemoryRuntimePlugin {
                    source: plugin.source,
                    name: plugin.name,
                    category: plugin.category,
                    description: plugin.description,
                    content_summary: plugin.content_summary,
                    updated_at: plugin.updated_at,
                })
                .collect(),
            updated_at: context.updated_at,
        }))
    }

    async fn get_skill(
        &self,
        user_id: &str,
        skill_id: &str,
    ) -> Result<Option<MemoryFullSkill>, String> {
        Ok(chatos_skills::get_skill(user_id, skill_id)
            .await?
            .map(|skill| MemoryFullSkill {
                name: skill.name,
                description: skill.description,
                content: skill.content,
                plugin_source: Some(skill.plugin_source),
                source_path: Some(skill.source_path),
                updated_at: skill.updated_at,
            }))
    }

    async fn get_skill_plugin(
        &self,
        user_id: &str,
        source: &str,
    ) -> Result<Option<MemoryFullPlugin>, String> {
        Ok(chatos_skills::get_skill_plugin(user_id, source)
            .await?
            .map(|plugin| MemoryFullPlugin {
                source: plugin.source,
                name: plugin.name,
                category: plugin.category,
                description: plugin.description,
                version: plugin.version,
                repository: plugin.repository,
                branch: plugin.branch,
                content: plugin.content,
                commands: plugin
                    .commands
                    .into_iter()
                    .map(|command| serde_json::to_value(command).unwrap_or(serde_json::Value::Null))
                    .collect(),
                command_count: plugin.command_count,
                updated_at: plugin.updated_at,
            }))
    }
}
