// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{agent_prompt_checksum, AgentPromptVendor};

use crate::models::{
    AgentPromptBundleVersionRecord, AgentPromptVersionPrompt, AgentPromptVersionRecord,
    AgentProviderPromptRecord, SOURCE_KIND_SYSTEM_SEED,
};
use crate::store::{now_rfc3339, AppStore};

pub(super) async fn seed_agent_prompts(
    store: &AppStore,
    admin_user_id: &str,
) -> Result<(), String> {
    let mut seeded_any = false;
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
            seeded_any = true;
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
    } else if seeded_any {
        store.increment_agent_prompt_bundle_version().await?;
    }
    Ok(())
}

pub(super) async fn backfill_agent_prompt_versions(store: &AppStore) -> Result<(), String> {
    let Some(bundle) = store.get_agent_prompt_bundle_version().await? else {
        return Ok(());
    };
    for agent in store.list_agents().await? {
        let agent_key = agent.agent_key;
        if !store
            .list_agent_prompt_versions(agent_key.as_str())
            .await?
            .is_empty()
        {
            continue;
        }
        let records = store.list_agent_prompts(agent_key.as_str()).await?;
        let published_by = records
            .iter()
            .find_map(|record| record.published_by.clone())
            .or_else(|| records.first().map(|record| record.updated_by.clone()))
            .unwrap_or_else(|| "system".to_string());
        let prompts = records
            .into_iter()
            .filter_map(|record| {
                let content = record
                    .published_content
                    .filter(|content| !content.trim().is_empty())?;
                let checksum = record
                    .published_checksum
                    .filter(|checksum| !checksum.trim().is_empty())?;
                if !record.enabled || record.published_revision <= 0 {
                    return None;
                }
                Some(AgentPromptVersionPrompt {
                    vendor: record.vendor,
                    content,
                    revision: record.published_revision,
                    checksum,
                    published_at: record
                        .published_at
                        .unwrap_or_else(|| record.updated_at.clone()),
                })
            })
            .collect::<Vec<_>>();
        if prompts.is_empty() {
            continue;
        }
        store
            .replace_agent_prompt_version(&AgentPromptVersionRecord {
                id: format!("{agent_key}__bundle__{}", bundle.version),
                agent_key,
                bundle_version: bundle.version,
                changed_vendor: None,
                prompts,
                published_by,
                published_at: bundle.updated_at.clone(),
            })
            .await?;
    }
    Ok(())
}

fn baseline_prompts() -> [(&'static str, &'static str); 11] {
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
        (
            "memory_engine_summary_agent",
            include_str!("../../seed_data/agent_prompts/memory_engine_summary_agent.md"),
        ),
        (
            "memory_engine_rollup_agent",
            include_str!("../../seed_data/agent_prompts/memory_engine_rollup_agent.md"),
        ),
        (
            "memory_engine_subject_memory_agent",
            include_str!("../../seed_data/agent_prompts/memory_engine_subject_memory_agent.md"),
        ),
        (
            "memory_engine_memory_rollup_agent",
            include_str!("../../seed_data/agent_prompts/memory_engine_memory_rollup_agent.md"),
        ),
        (
            "memory_engine_thread_repair_agent",
            include_str!("../../seed_data/agent_prompts/memory_engine_thread_repair_agent.md"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_catalog_covers_all_system_agents() {
        let prompts = baseline_prompts();
        assert_eq!(prompts.len(), 11);
        assert!(prompts
            .iter()
            .all(|(_, content)| !content.trim().is_empty()));
    }
}
