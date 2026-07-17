// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, AgentPromptVendor,
    PluginManagementClient, PluginManagementClientConfig, ResolveAgentPromptRequest,
    ResolvedAgentPrompt, SystemAgentKey,
};

use super::{AgentError, AgentIdentity};

static CLIENTS: OnceLock<Mutex<HashMap<String, PluginManagementClient>>> = OnceLock::new();

pub async fn resolve_managed_prompt_for_model<A>(
    caller_service: &str,
    agent: &A,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, AgentError>
where
    A: AgentIdentity,
{
    resolve_managed_prompt_by_key_for_model(
        caller_service,
        agent.descriptor().key,
        prompt_vendor,
        model_provider,
    )
    .await
}

pub async fn resolve_managed_prompt_for_model_with_client<A>(
    client: &PluginManagementClient,
    agent: &A,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, AgentError>
where
    A: AgentIdentity,
{
    let agent_key = agent.descriptor().key;
    let vendor = required_agent_prompt_vendor(prompt_vendor, model_provider)
        .map_err(|error| AgentError::execution(agent_key.as_str(), error.to_string()))?;
    resolve_managed_prompt_by_key_with_vendor(client, agent_key, vendor).await
}

pub async fn resolve_managed_prompt_by_key_for_model(
    caller_service: &str,
    agent_key: SystemAgentKey,
    prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, AgentError> {
    let vendor = required_agent_prompt_vendor(prompt_vendor, model_provider)
        .map_err(|error| AgentError::execution(agent_key.as_str(), error.to_string()))?;
    let client = client_for_service(caller_service)
        .await
        .map_err(|error| AgentError::execution(agent_key.as_str(), error))?;
    resolve_managed_prompt_by_key_with_vendor(&client, agent_key, vendor).await
}

async fn resolve_managed_prompt_by_key_with_vendor(
    client: &PluginManagementClient,
    agent_key: SystemAgentKey,
    vendor: AgentPromptVendor,
) -> Result<ResolvedAgentPrompt, AgentError> {
    let prompt = client
        .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest { agent_key, vendor })
        .await
        .map_err(|error| {
            AgentError::execution(
                agent_key.as_str(),
                format!("resolve published prompt for vendor {vendor} failed: {error}"),
            )
        })?;
    if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
        return Err(AgentError::execution(
            agent_key.as_str(),
            format!("published prompt checksum is invalid for vendor {vendor}"),
        ));
    }
    Ok(prompt)
}

async fn client_for_service(caller_service: &str) -> Result<PluginManagementClient, String> {
    let caller_service = caller_service.trim();
    if caller_service.is_empty() {
        return Err("plugin management caller service is required".to_string());
    }
    if let Some(client) = cached_client(caller_service)? {
        return Ok(client);
    }

    let config = PluginManagementClientConfig::from_env(caller_service).await;
    let new_client = PluginManagementClient::new(config).map_err(|error| error.to_string())?;
    let clients = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut clients = clients
        .lock()
        .map_err(|_| "plugin management client cache is unavailable".to_string())?;
    Ok(clients
        .entry(caller_service.to_string())
        .or_insert_with(|| new_client.clone())
        .clone())
}

fn cached_client(caller_service: &str) -> Result<Option<PluginManagementClient>, String> {
    let Some(clients) = CLIENTS.get() else {
        return Ok(None);
    };
    clients
        .lock()
        .map_err(|_| "plugin management client cache is unavailable".to_string())
        .map(|clients| clients.get(caller_service).cloned())
}
