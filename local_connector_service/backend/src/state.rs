// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

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
    device_connect_nonces: Arc<Mutex<HashMap<String, i64>>>,
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
            device_connect_nonces: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn consume_device_connect_nonce(
        &self,
        device_id: &str,
        nonce: &str,
        now: i64,
    ) -> bool {
        let retention = self
            .config
            .device_connect_signature_max_skew
            .as_secs()
            .try_into()
            .unwrap_or(300_i64);
        let expires_at = now.saturating_add(retention);
        let min_expires_at = now.saturating_sub(retention);
        let key = format!("{device_id}:{nonce}");
        let mut nonces = self.device_connect_nonces.lock().await;
        nonces.retain(|_, expires_at| *expires_at >= min_expires_at);
        if nonces.contains_key(key.as_str()) {
            return false;
        }
        nonces.insert(key, expires_at);
        true
    }
}
