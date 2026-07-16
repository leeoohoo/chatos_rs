// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, ResolveAgentPromptRequest,
    ResolvedAgentPrompt, SystemAgentKey,
};

use crate::state::AppState;

pub(super) async fn resolve_project_environment_agent_prompt(
    state: &AppState,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let vendor = required_agent_prompt_vendor(prompt_vendor, model_provider)
        .map_err(|err| format!("resolve project agent prompt vendor failed: {err}"))?;
    let prompt = state
        .plugin_management_client
        .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest {
            agent_key: SystemAgentKey::ProjectManagementAgent,
            vendor,
        })
        .await
        .map_err(|err| {
            format!(
                "resolve project management published prompt failed: vendor={vendor} error={err}"
            )
        })?;
    if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
        return Err(format!(
            "project management published prompt checksum invalid: vendor={vendor}"
        ));
    }
    Ok(prompt)
}
