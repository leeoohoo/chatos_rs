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
    let mut changed_agents = Vec::new();
    for (agent_key, content) in baseline_prompts() {
        let content = content.trim().to_string();
        let checksum = agent_prompt_checksum(content.as_str());
        for vendor in AgentPromptVendor::ALL {
            if let Some(mut existing) = store.get_agent_prompt(agent_key, vendor).await? {
                let should_sync = should_sync_system_seed(&existing);
                let baseline_changed = existing.published_checksum.as_deref() != Some(&checksum);
                if should_sync && baseline_changed {
                    let now = now_rfc3339();
                    existing.draft_content = Some(content.clone());
                    existing.published_content = Some(content.clone());
                    existing.published_revision =
                        existing.published_revision.saturating_add(1).max(1);
                    existing.published_checksum = Some(checksum.clone());
                    existing.seed_checksum = Some(checksum.clone());
                    existing.enabled = true;
                    existing.updated_by = admin_user_id.to_string();
                    existing.published_by = Some(admin_user_id.to_string());
                    existing.updated_at = now.clone();
                    existing.published_at = Some(now);
                    store.replace_agent_prompt(&existing).await?;
                    seeded_any = true;
                    if !changed_agents.contains(&agent_key) {
                        changed_agents.push(agent_key);
                    }
                } else if should_sync && existing.seed_checksum.is_none() {
                    existing.seed_checksum = existing.published_checksum.clone();
                    store.replace_agent_prompt(&existing).await?;
                }
                continue;
            }
            let now = now_rfc3339();
            let record = AgentProviderPromptRecord {
                id: format!("{agent_key}__prompt__{vendor}"),
                agent_key: agent_key.to_string(),
                vendor,
                draft_content: Some(content.clone()),
                published_content: Some(content.clone()),
                published_revision: 1,
                published_checksum: Some(checksum.clone()),
                seed_checksum: Some(checksum.clone()),
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
            if !changed_agents.contains(&agent_key) {
                changed_agents.push(agent_key);
            }
        }
    }

    let bundle = if store.get_agent_prompt_bundle_version().await?.is_none() {
        let record = AgentPromptBundleVersionRecord {
            id: "system_agent_prompts".to_string(),
            version: 1,
            updated_at: now_rfc3339(),
            required: false,
        };
        store.replace_agent_prompt_bundle_version(&record).await?;
        Some(record)
    } else if seeded_any {
        Some(store.increment_agent_prompt_bundle_version().await?)
    } else {
        None
    };
    if let Some(bundle) = bundle {
        for agent_key in changed_agents {
            persist_seed_prompt_version(store, agent_key, &bundle, admin_user_id).await?;
        }
    }
    Ok(())
}

fn should_sync_system_seed(record: &AgentProviderPromptRecord) -> bool {
    if record.source_kind != SOURCE_KIND_SYSTEM_SEED {
        return false;
    }
    let Some(published_content) = record.published_content.as_deref() else {
        return false;
    };
    if record.draft_content.as_deref() != Some(published_content) {
        return false;
    }
    match record.seed_checksum.as_deref() {
        Some(seed_checksum) => record.published_checksum.as_deref() == Some(seed_checksum),
        None => record.generated_by_model_config_id.is_none(),
    }
}

async fn persist_seed_prompt_version(
    store: &AppStore,
    agent_key: &str,
    bundle: &AgentPromptBundleVersionRecord,
    admin_user_id: &str,
) -> Result<(), String> {
    let prompts = store
        .list_agent_prompts(agent_key)
        .await?
        .into_iter()
        .filter_map(|record| {
            let content = record.published_content?;
            let checksum = record.published_checksum?;
            if !record.enabled || record.published_revision <= 0 {
                return None;
            }
            Some(AgentPromptVersionPrompt {
                vendor: record.vendor,
                content,
                revision: record.published_revision,
                checksum,
                published_at: record.published_at.unwrap_or(record.updated_at),
            })
        })
        .collect::<Vec<_>>();
    if prompts.is_empty() {
        return Ok(());
    }
    store
        .replace_agent_prompt_version(&AgentPromptVersionRecord {
            id: format!("{agent_key}__bundle__{}", bundle.version),
            agent_key: agent_key.to_string(),
            bundle_version: bundle.version,
            changed_vendor: None,
            prompts,
            published_by: admin_user_id.to_string(),
            published_at: bundle.updated_at.clone(),
        })
        .await
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

fn baseline_prompts() -> [(&'static str, &'static str); 12] {
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
            "task_runner_plan_phase",
            include_str!("../../seed_data/agent_prompts/task_runner_plan_phase.md"),
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
        assert_eq!(prompts.len(), 12);
        assert!(prompts
            .iter()
            .all(|(_, content)| !content.trim().is_empty()));
    }

    #[test]
    fn language_sensitive_agents_follow_the_user_language() {
        let prompts = baseline_prompts();
        for agent_key in [
            "chatos_conversation_agent",
            "chatos_planning_agent",
            "project_requirement_execution_planner_agent",
            "task_runner_plan_phase",
            "task_runner_run_phase",
        ] {
            let content = prompts
                .iter()
                .find(|(key, _)| *key == agent_key)
                .map(|(_, content)| *content)
                .unwrap_or_else(|| panic!("missing prompt: {agent_key}"));
            assert!(content.contains("用户语言") || content.contains("用户当前语言"));
            assert!(content.contains("代码标识符"));
            if agent_key == "chatos_planning_agent" {
                assert!(content.contains("内部 ID"));
                assert!(content.contains("普通用户"));
                assert!(content.contains("临时操作约束"));
                assert!(content.contains("is_planning_task"));
                assert!(content.contains("定量约束"));
                assert!(content.contains("逐项核对数量"));
                assert!(content.contains("不得用重复需求、重复文档或重复任务凑数量"));
            }
            if agent_key == "task_runner_plan_phase" {
                assert!(content.contains("不得执行工程实现"));
                assert!(content.contains("不得创建、修改、移动或删除项目文件"));
                assert!(content.contains("不得运行终端命令"));
            }
            if agent_key == "task_runner_run_phase" {
                assert!(content.contains("自动收集沙箱输出"));
                assert!(content.contains("逐个复制"));
                assert!(content.contains("及时收口任务"));
                assert!(content.contains("容量受限的内存盘"));
                assert!(content.contains(".chatos/tmp"));
                assert!(content.contains("不得写入项目依赖清单"));
            }
        }
    }

    fn prompt_record() -> AgentProviderPromptRecord {
        let content = "旧系统 Prompt".to_string();
        let checksum = agent_prompt_checksum(content.as_str());
        AgentProviderPromptRecord {
            id: "agent__prompt__gpt".to_string(),
            agent_key: "agent".to_string(),
            vendor: AgentPromptVendor::Gpt,
            draft_content: Some(content.clone()),
            published_content: Some(content),
            published_revision: 1,
            published_checksum: Some(checksum.clone()),
            seed_checksum: Some(checksum),
            enabled: true,
            source_kind: SOURCE_KIND_SYSTEM_SEED.to_string(),
            generated_by_model_config_id: None,
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            published_by: Some("system".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            published_at: Some("2026-01-01T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn untouched_system_seed_can_follow_a_new_baseline() {
        assert!(should_sync_system_seed(&prompt_record()));
    }

    #[test]
    fn legacy_untouched_system_seed_can_be_migrated() {
        let mut record = prompt_record();
        record.seed_checksum = None;
        record.published_revision = 2;
        assert!(should_sync_system_seed(&record));
    }

    #[test]
    fn customized_or_unpublished_prompt_is_not_overwritten() {
        let mut customized = prompt_record();
        customized.published_revision = 2;
        customized.published_checksum = Some(agent_prompt_checksum("管理员版本"));
        assert!(!should_sync_system_seed(&customized));

        let mut draft = prompt_record();
        draft.draft_content = Some("尚未发布的管理员草稿".to_string());
        assert!(!should_sync_system_seed(&draft));
    }
}
