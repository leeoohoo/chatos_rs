// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{resolve_managed_prompt_for_model_with_client, TASK_RUNNER_AGENT};
use chatos_plugin_management_sdk::ResolvedAgentPrompt;

use super::RunService;

pub(crate) async fn resolve_task_runner_agent_prompt(
    service: &RunService,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let client = service
        .plugin_management_client
        .as_ref()
        .ok_or_else(|| "task runner plugin management client is not configured".to_string())?;
    resolve_managed_prompt_for_model_with_client(
        client,
        &TASK_RUNNER_AGENT,
        prompt_vendor,
        model_provider,
    )
    .await
    .map_err(|error| error.to_string())
}
