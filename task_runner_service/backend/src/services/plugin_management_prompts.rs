// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, ResolveAgentPromptRequest,
    ResolvedAgentPrompt, SystemAgentKey,
};

use super::RunService;

pub(crate) async fn resolve_task_runner_agent_prompt(
    service: &RunService,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let vendor = required_agent_prompt_vendor(prompt_vendor, model_provider)
        .map_err(|err| format!("resolve task runner prompt vendor failed: {err}"))?;
    let client = service
        .plugin_management_client
        .as_ref()
        .ok_or_else(|| "task runner plugin management client is not configured".to_string())?;
    let prompt = client
        .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest {
            agent_key: SystemAgentKey::TaskRunnerRunPhase,
            vendor,
        })
        .await
        .map_err(|err| {
            format!("resolve task runner published prompt failed: vendor={vendor} error={err}")
        })?;
    if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
        return Err(format!(
            "task runner published prompt checksum invalid: vendor={vendor}"
        ));
    }
    Ok(prompt)
}
