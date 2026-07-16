// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, PluginManagementClient,
    PluginManagementClientConfig, ResolveAgentPromptRequest, ResolvedAgentPrompt, SystemAgentKey,
};
use tokio::sync::OnceCell;

static CLIENT: OnceCell<PluginManagementClient> = OnceCell::const_new();

pub async fn resolve_for_model(
    agent_key: SystemAgentKey,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let vendor = required_agent_prompt_vendor(prompt_vendor, model_provider)
        .map_err(|err| format!("resolve agent prompt vendor failed: {err}"))?;
    let client = CLIENT
        .get_or_try_init(|| async {
            let config = PluginManagementClientConfig::from_env("chatos-backend").await;
            PluginManagementClient::new(config).map_err(|err| err.to_string())
        })
        .await?;
    let prompt = client
        .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest { agent_key, vendor })
        .await
        .map_err(|err| {
            format!(
                "resolve published agent prompt failed: agent_key={agent_key} vendor={vendor} error={err}"
            )
        })?;
    if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
        return Err(format!(
            "published agent prompt checksum invalid: agent_key={agent_key} vendor={vendor}"
        ));
    }
    Ok(prompt)
}
