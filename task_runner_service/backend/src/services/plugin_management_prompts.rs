// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{resolve_managed_prompt_for_model_with_client, TaskRunnerAgent};
#[cfg(test)]
use chatos_agent::AgentIdentity;
use chatos_plugin_management_sdk::ResolvedAgentPrompt;

use super::RunService;

pub(crate) async fn resolve_task_runner_agent_prompt(
    service: &RunService,
    agent: &TaskRunnerAgent,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let Some(client) = service.plugin_management_client.as_ref() else {
        #[cfg(test)]
        {
            let vendor = chatos_plugin_management_sdk::required_agent_prompt_vendor(
                prompt_vendor,
                model_provider,
            )
            .map_err(|error| error.to_string())?;
            let content = format!("test prompt for {}", agent.descriptor().key.as_str());
            return Ok(ResolvedAgentPrompt {
                agent_key: agent.descriptor().key.as_str().to_string(),
                vendor,
                checksum: chatos_plugin_management_sdk::agent_prompt_checksum(content.as_str()),
                content,
                revision: 1,
                published_at: "1970-01-01T00:00:00Z".to_string(),
            });
        }
        #[cfg(not(test))]
        return Err("task runner plugin management client is not configured".to_string());
    };
    resolve_managed_prompt_for_model_with_client(client, agent, prompt_vendor, model_provider)
        .await
        .map_err(|error| error.to_string())
}
