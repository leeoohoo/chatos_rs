// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{agent_prompt_checksum, AgentPromptVendor};

use crate::models::{
    AgentPromptBundleVersionRecord, AgentProviderPromptRecord, SOURCE_KIND_SYSTEM_SEED,
};
use crate::store::{now_rfc3339, AppStore};

pub(super) async fn seed_agent_prompts(
    store: &AppStore,
    admin_user_id: &str,
) -> Result<(), String> {
    for (agent_key, content) in baseline_prompts() {
        for vendor in AgentPromptVendor::ALL {
            if store.get_agent_prompt(agent_key, vendor).await?.is_some() {
                continue;
            }
            let now = now_rfc3339();
            let content = content.trim().to_string();
            let record = AgentProviderPromptRecord {
                id: format!("{agent_key}__prompt__{vendor}"),
                agent_key: agent_key.to_string(),
                vendor,
                draft_content: Some(content.clone()),
                published_content: Some(content.clone()),
                published_revision: 1,
                published_checksum: Some(agent_prompt_checksum(content.as_str())),
                enabled: true,
                source_kind: SOURCE_KIND_SYSTEM_SEED.to_string(),
                generated_by_model_config_id: None,
                created_by: admin_user_id.to_string(),
                updated_by: admin_user_id.to_string(),
                published_by: Some(admin_user_id.to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
                published_at: Some(now),
            };
            store.replace_agent_prompt(&record).await?;
        }
    }

    if store.get_agent_prompt_bundle_version().await?.is_none() {
        let record = AgentPromptBundleVersionRecord {
            id: "system_agent_prompts".to_string(),
            version: 1,
            updated_at: now_rfc3339(),
            required: false,
        };
        store.replace_agent_prompt_bundle_version(&record).await?;
    }
    Ok(())
}

fn baseline_prompts() -> [(&'static str, &'static str); 6] {
    [
        (
            "chatos_conversation_agent",
            include_str!("../../seed_data/agent_prompts/chatos_conversation_agent.md"),
        ),
        (
            "chatos_planning_agent",
            include_str!("../../seed_data/agent_prompts/chatos_planning_agent.md"),
        ),
        (
            "project_requirement_execution_planner_agent",
            include_str!(
                "../../seed_data/agent_prompts/project_requirement_execution_planner_agent.md"
            ),
        ),
        (
            "task_runner_run_phase",
            include_str!("../../seed_data/agent_prompts/task_runner_run_phase.md"),
        ),
        (
            "project_management_agent",
            include_str!("../../seed_data/agent_prompts/project_management_agent.md"),
        ),
        (
            "local_connector_command_approval_agent",
            include_str!("../../seed_data/agent_prompts/local_connector_command_approval_agent.md"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_catalog_covers_all_system_agents() {
        let prompts = baseline_prompts();
        assert_eq!(prompts.len(), 6);
        assert!(prompts
            .iter()
            .all(|(_, content)| !content.trim().is_empty()));
    }
}
