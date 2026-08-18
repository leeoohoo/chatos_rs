// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use crate::config::AppConfig;
use crate::store::AppStore;
use chatos_cloud_agent_runtime::CloudAgentStateStore;
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
    pub plugin_management_client: PluginManagementClient,
    pub cloud_agent_store: CloudAgentStateStore,
    pub runtime_environment_image_jobs: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = AppStore::new(&config.database_url).await?;
        let cloud_agent_store = CloudAgentStateStore::connect(&config.database_url).await?;
        let plugin_management_config = PluginManagementClientConfig::from_env("project-service")
            .await
            .map_err(|err| format!("load plugin management client config failed: {err}"))?;
        let plugin_management_client = PluginManagementClient::new(plugin_management_config)
            .map_err(|err| format!("initialize plugin management client failed: {err}"))?;
        Ok(Self {
            config,
            store,
            plugin_management_client,
            cloud_agent_store,
            runtime_environment_image_jobs: Arc::new(Mutex::new(std::collections::HashSet::new())),
        })
    }

    #[cfg(test)]
    pub(crate) async fn new_without_external_dependencies(
        config: AppConfig,
    ) -> Result<Self, String> {
        let store = AppStore::new_without_indexes(&config.database_url).await?;
        let cloud_agent_store = CloudAgentStateStore::memory();
        let plugin_management_client = PluginManagementClient::new(
            PluginManagementClientConfig::new(
                "http://127.0.0.1:1",
                "https://127.0.0.1:1",
                std::time::Duration::from_secs(5),
                None,
                "project-service",
                reqwest::Client::new(),
            )
            .map_err(|err| format!("build test plugin management config failed: {err}"))?,
        )
        .map_err(|err| format!("initialize test plugin management client failed: {err}"))?;
        Ok(Self {
            config,
            store,
            plugin_management_client,
            cloud_agent_store,
            runtime_environment_image_jobs: Arc::new(Mutex::new(std::collections::HashSet::new())),
        })
    }
}
