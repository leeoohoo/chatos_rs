// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::relay::ConnectorRelay;
use crate::store::ConnectorStore;
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub relay: ConnectorRelay,
    pub store: ConnectorStore,
    pub plugin_management_client: PluginManagementClient,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = ConnectorStore::connect(&config.database_url).await?;
        let plugin_management_client = PluginManagementClient::new(
            PluginManagementClientConfig::from_env("local-connector-service").await,
        )
        .map_err(|err| format!("initialize plugin management client failed: {err}"))?;
        Ok(Self {
            config,
            relay: ConnectorRelay::default(),
            store,
            plugin_management_client,
        })
    }
}
