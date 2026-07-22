// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginManagementClient, PluginManagementClientConfig, ResolveAgentCapabilitiesRequest,
    ResolvedAgentCapabilities, SystemAgentKey,
};
use tokio::sync::OnceCell;

use crate::services::access_token_scope;

static CLIENT: OnceCell<PluginManagementClient> = OnceCell::const_new();

pub async fn resolve_for_current_user(
    agent_key: SystemAgentKey,
    owner_user_id: &str,
) -> Result<ResolvedAgentCapabilities, String> {
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err("plugin management owner user id is required".to_string());
    }
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| "current user access token is missing".to_string())?;
    let client = CLIENT
        .get_or_try_init(|| async {
            let config = PluginManagementClientConfig::from_env("chatos-backend").await;
            PluginManagementClient::new(config).map_err(|err| err.to_string())
        })
        .await?;
    client
        .resolve_for_user(
            &ResolveAgentCapabilitiesRequest::new(agent_key, owner_user_id).with_runtime_context(
                None,
                None,
                Some("cloud".to_string()),
                None,
            ),
            access_token.as_str(),
        )
        .await
        .map_err(|err| err.to_string())
}
