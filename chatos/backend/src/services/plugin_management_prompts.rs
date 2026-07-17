// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::resolve_managed_prompt_by_key_for_model;
use chatos_plugin_management_sdk::{ResolvedAgentPrompt, SystemAgentKey};

pub async fn resolve_for_model(
    agent_key: SystemAgentKey,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    resolve_managed_prompt_by_key_for_model(
        "chatos-backend",
        agent_key,
        prompt_vendor,
        model_provider,
    )
    .await
    .map_err(|error| error.to_string())
}
