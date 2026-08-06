// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;

use super::{SandboxManagerAuth, SandboxManagerAuthMode};
use crate::services::sandbox_runtime::SandboxRuntimeContext;

impl SandboxManagerAuth {
    pub(in crate::services::sandbox_runtime) fn from_config(config: &AppConfig) -> Option<Self> {
        match (
            config.sandbox_manager_client_id.clone(),
            config.sandbox_manager_client_key.clone(),
        ) {
            (Some(_client_id), Some(client_key)) => Some(Self {
                client_key,
                mode: SandboxManagerAuthMode::Cloud,
                owner_user_id: None,
                cloud_http: Some(config.sandbox_manager_http_client.clone()),
            }),
            _ => None,
        }
    }

    pub(in crate::services::sandbox_runtime) fn local_connector(
        client_key: String,
        owner_user_id: String,
        http: reqwest::Client,
    ) -> Self {
        Self {
            client_key,
            mode: SandboxManagerAuthMode::LocalConnector,
            owner_user_id: Some(owner_user_id),
            cloud_http: Some(http),
        }
    }

    pub(in crate::services::sandbox_runtime) fn for_context(
        config: &AppConfig,
        context: &SandboxRuntimeContext,
    ) -> Result<Option<Self>, String> {
        match context.provider_kind()? {
            chatos_mcp_management_sdk::SandboxProviderKind::LocalConnector => {
                let owner_user_id = context.owner_user_id.trim();
                if owner_user_id.is_empty() {
                    return Err(
                        "Local Connector sandbox context is missing owner user id".to_string()
                    );
                }
                let client_key = config
                    .local_connector_internal_api_secret
                    .clone()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for local sandbox release"
                            .to_string()
                    })?;
                Ok(Some(Self::local_connector(
                    client_key,
                    owner_user_id.to_string(),
                    config.local_connector_http_client.clone(),
                )))
            }
            chatos_mcp_management_sdk::SandboxProviderKind::Cloud => Ok(Self::from_config(config)),
            chatos_mcp_management_sdk::SandboxProviderKind::None => {
                Err("sandbox runtime context provider is unresolved".to_string())
            }
        }
    }
}
