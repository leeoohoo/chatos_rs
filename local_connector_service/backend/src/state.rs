// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::managed_requirements::ManagedRequirementsSigner;
use crate::relay::ConnectorRelay;
use crate::store::ConnectorStore;
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub relay: ConnectorRelay,
    pub store: ConnectorStore,
    pub plugin_management_client: PluginManagementClient,
    user_service_http: reqwest::Client,
    memory_engine_http: reqwest::Client,
    pub(crate) managed_requirements_signer: Option<Arc<ManagedRequirementsSigner>>,
    device_connect_nonces: Arc<Mutex<HashMap<String, i64>>>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let managed_requirements_signer = ManagedRequirementsSigner::load(&config)?;
        let store = ConnectorStore::connect(&config.database_url).await?;
        let plugin_management_client = PluginManagementClient::new(
            PluginManagementClientConfig::from_env("local-connector-service").await,
        )
        .map_err(|err| format!("initialize plugin management client failed: {err}"))?;
        let user_service_http =
            build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
                .map_err(|err| format!("build user_service client failed: {err}"))?;
        let memory_engine_http = build_http_client(HttpClientTimeouts::new(
            config.memory_engine_request_timeout,
        ))
        .map_err(|err| format!("build Memory Engine client failed: {err}"))?;
        if let Some(signer) = managed_requirements_signer.as_ref() {
            tracing::info!(
                key_id = signer.key_id(),
                public_key = signer.public_key(),
                "managed requirements bundle signing is enabled"
            );
        }
        Ok(Self {
            config,
            relay: ConnectorRelay::default(),
            store,
            plugin_management_client,
            user_service_http,
            memory_engine_http,
            managed_requirements_signer,
            device_connect_nonces: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub(crate) fn user_service_http(&self) -> &reqwest::Client {
        &self.user_service_http
    }

    pub(crate) fn memory_engine_http(&self) -> &reqwest::Client {
        &self.memory_engine_http
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
