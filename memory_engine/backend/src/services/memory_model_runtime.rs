// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::sync::OnceCell;
use tracing::info;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::models::EngineModelProfile;
use crate::services::memory_ai_job::MemoryAiJobKind;
use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, PluginManagementClient,
    PluginManagementClientConfig, ResolveAgentPromptRequest, ResolvedAgentPrompt,
};

use super::model_runtime_resolver::resolve_memory_summary_model_runtime;

static PLUGIN_MANAGEMENT_CLIENT: OnceCell<PluginManagementClient> = OnceCell::const_new();

/// The complete model boundary for one Memory Engine-owned, tool-free job.
///
/// This deliberately contains no Agent Loop, Run state, tool executor, queue
/// consumer, or remote execution adapter. Queue workers may invoke it, but one
/// value only owns the resolved model client and immutable published prompt for
/// a single Memory Engine generation pipeline.
pub(crate) struct MemoryModelJobRuntime {
    pub(crate) ai: AiClient,
    pub(crate) prompt: String,
    pub(crate) model_config_id: String,
    pub(crate) prompt_revision: i64,
    pub(crate) prompt_checksum: String,
}

pub(crate) async fn effective_model_profile(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<EngineModelProfile, String> {
    resolve_memory_summary_model_runtime(config, owner_user_id).await
}

pub(crate) async fn build_ai_client(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<AiClient, String> {
    let profile = effective_model_profile(config, owner_user_id).await?;
    AiClient::new_with_profile(config, Some(&profile))
}

pub(crate) async fn build_memory_model_job_runtime(
    config: &AppConfig,
    job: MemoryAiJobKind,
    owner_user_id: &str,
) -> Result<MemoryModelJobRuntime, String> {
    let profile = effective_model_profile(config, owner_user_id).await?;
    let model_provider = profile.provider.trim();
    let prompt = resolve_memory_job_prompt(job, model_provider).await?;
    let ai = AiClient::new_with_profile(config, Some(&profile))?;
    info!(
        prompt_key = prompt.agent_key.as_str(),
        vendor = prompt.vendor.as_str(),
        revision = prompt.revision,
        job_type = job.job_type(),
        "resolved Memory Engine model job prompt"
    );
    Ok(MemoryModelJobRuntime {
        ai,
        prompt: prompt.content,
        model_config_id: profile.id,
        prompt_revision: prompt.revision,
        prompt_checksum: prompt.checksum,
    })
}

async fn resolve_memory_job_prompt(
    job: MemoryAiJobKind,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let prompt_key = job.prompt_key();
    let vendor = required_agent_prompt_vendor(None, model_provider).map_err(|error| {
        format!(
            "resolve prompt vendor for {} failed: {error}",
            prompt_key.as_str()
        )
    })?;
    let client = PLUGIN_MANAGEMENT_CLIENT
        .get_or_try_init(|| async {
            let config = PluginManagementClientConfig::from_env("memory-engine").await?;
            PluginManagementClient::new(config).map_err(|error| error.to_string())
        })
        .await?;
    let prompt = client
        .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest {
            agent_key: prompt_key,
            vendor,
            profile: None,
        })
        .await
        .map_err(|error| {
            format!(
                "resolve published prompt for {} and vendor {vendor} failed: {error}",
                prompt_key.as_str()
            )
        })?;
    if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
        return Err(format!(
            "published prompt checksum is invalid for {} and vendor {vendor}",
            prompt_key.as_str()
        ));
    }
    Ok(prompt)
}
