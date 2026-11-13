// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::info;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{EngineJobPolicy, EngineModelProfile};
use crate::services::summary::RollupSettings;
use chatos_agent::MemoryEngineAgent;

use super::model_runtime_resolver::{
    resolve_memory_summary_model_runtime, resolve_model_runtime_by_id,
};

pub struct ManagedMemoryAgentRuntime {
    pub ai: AiClient,
    pub prompt: String,
    pub model_config_id: String,
    pub model_provider: String,
    pub model_id: String,
    pub prompt_revision: i64,
    pub prompt_checksum: String,
    pub budget: super::model_budget::MemoryModelBudget,
}

pub async fn get_effective_model_profile_for_job(
    config: &AppConfig,
    _job_type: &str,
    owner_user_id: &str,
) -> Result<EngineModelProfile, String> {
    resolve_memory_summary_model_runtime(config, owner_user_id).await
}

pub(crate) async fn build_ai_client_for_profile_id(
    config: &AppConfig,
    _db: &Db,
    profile_id: &str,
    owner_user_id: &str,
) -> Result<AiClient, String> {
    if profile_id == "memory_engine_default" {
        return Err("legacy Memory Engine default model is no longer supported".to_string());
    }
    let profile = resolve_model_runtime_by_id(config, owner_user_id, profile_id).await?;
    AiClient::new_with_profile(config, Some(&profile))
}

pub async fn build_managed_memory_agent_runtime(
    config: &AppConfig,
    _db: &Db,
    agent: &MemoryEngineAgent,
    owner_user_id: &str,
) -> Result<ManagedMemoryAgentRuntime, String> {
    let profile =
        get_effective_model_profile_for_job(config, agent.job_type(), owner_user_id).await?;
    let budget = super::model_budget::MemoryModelBudget::from_profile(config, &profile)?;
    let model_provider = profile.provider.trim();
    let prompt = agent
        .resolve_prompt(model_provider)
        .await
        .map_err(|error| error.to_string())?;
    let ai = AiClient::new_with_profile(config, Some(&profile))?;
    info!(
        agent_key = prompt.agent_key.as_str(),
        vendor = prompt.vendor.as_str(),
        revision = prompt.revision,
        job_type = agent.job_type(),
        "resolved managed Memory Engine Agent Prompt"
    );
    Ok(ManagedMemoryAgentRuntime {
        ai,
        budget,
        prompt: prompt.content,
        model_config_id: profile.id,
        model_provider: profile.provider,
        model_id: profile.model,
        prompt_revision: prompt.revision,
        prompt_checksum: prompt.checksum,
    })
}

pub fn get_effective_job_policy(
    config: &AppConfig,
    job_type: &str,
) -> Result<EngineJobPolicy, String> {
    policy_from_published_common_capacity(
        &config.model_capabilities,
        &config.active_model_capability_revision,
        job_type,
    )
}

fn policy_from_published_common_capacity(
    catalog: &std::collections::HashMap<String, crate::config::PublishedModelCapability>,
    revision: &str,
    job_type: &str,
) -> Result<EngineJobPolicy, String> {
    if !["summary", "rollup", "subject_memory"].contains(&job_type) {
        return Err(format!("unsupported Memory generation job {job_type}"));
    }
    let capability = catalog.get(revision).ok_or_else(|| {
        format!(
            "common model capability revision {revision} is not published in Configuration Center"
        )
    })?;
    let input_limit = capability.input_limit()?;
    Ok(EngineJobPolicy {
        job_type: job_type.into(),
        model_capability_revision: revision.to_string(),
        context_window_tokens: i64::try_from(capability.context_window_tokens)
            .map_err(|_| "common context capacity exceeds supported integer range")?,
        output_reserve_tokens: i64::try_from(capability.output_reserve_tokens)
            .map_err(|_| "common output capacity exceeds supported integer range")?,
        max_model_input_tokens: i64::try_from(input_limit)
            .map_err(|_| "common input capacity exceeds supported integer range")?,
    })
}

pub fn load_rollup_settings(config: &AppConfig) -> Result<RollupSettings, String> {
    let policy = get_effective_job_policy(config, "rollup")?;
    Ok(RollupSettings {
        token_limit: policy.max_model_input_tokens,
        source_batch_token_limit: policy.max_model_input_tokens,
        cloud_owner_entity_id: None,
        cloud_source_id: None,
        cloud_thread_id: None,
    })
}

#[cfg(test)]
mod common_capacity_tests {
    use super::*;

    #[test]
    fn all_jobs_use_the_global_capacity_without_an_account_or_selected_model() {
        let capability = crate::config::PublishedModelCapability {
            context_window_tokens: 100_000,
            output_reserve_tokens: 20_000,
            protocol_overhead_estimator_id: "responses-json-v1".into(),
            tokenizer_id: None,
        };
        let mut catalog = std::collections::HashMap::from([("common".into(), capability)]);
        for job_type in ["summary", "rollup", "subject_memory"] {
            let policy =
                policy_from_published_common_capacity(&catalog, "common", job_type).unwrap();
            assert_eq!(policy.max_model_input_tokens, 80_000);
            let view = serde_json::to_value(policy).unwrap();
            for field in ["model_config_id", "provider", "model", "owner_user_id"] {
                assert!(
                    view.get(field).is_none(),
                    "global policy must not depend on {field}"
                );
            }
        }
        catalog.get_mut("common").unwrap().context_window_tokens = 110_000;
        assert_eq!(
            policy_from_published_common_capacity(&catalog, "common", "summary")
                .unwrap()
                .max_model_input_tokens,
            90_000
        );
        assert!(policy_from_published_common_capacity(&catalog, "missing", "summary").is_err());
        assert!(policy_from_published_common_capacity(&catalog, "common", "unknown").is_err());
    }
}
