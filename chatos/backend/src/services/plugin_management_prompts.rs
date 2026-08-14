// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::resolve_managed_prompt_by_key_for_model_with_profile;
use chatos_plugin_management_sdk::{ResolvedAgentPrompt, SystemAgentKey};

pub async fn resolve_for_model(
    agent_key: SystemAgentKey,
    prompt_vendor: Option<&str>,
    model_provider: &str,
    profile: Option<&str>,
) -> Result<ResolvedAgentPrompt, String> {
    resolve_managed_prompt_by_key_for_model_with_profile(
        "chatos-backend",
        agent_key,
        prompt_vendor,
        model_provider,
        profile,
    )
    .await
    .map_err(|error| error.to_string())
}
