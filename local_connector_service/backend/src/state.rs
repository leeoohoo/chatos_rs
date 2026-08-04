// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chatos_agent::ManagedRuntimeConfigBundle;
use chatos_config_sdk::ConfigClient;
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::managed_config::{
    resolve_platform_relay_signing_config, resolve_relay_runtime_limits,
    resolve_remote_control_trust_bundle,
};
use crate::managed_requirements::ManagedRequirementsSigner;
use crate::relay::ConnectorRelay;
use crate::relay_signature::PlatformRelaySigner;
use crate::store::ConnectorStore;
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};

const LOCAL_CONNECTOR_CONFIG_WATCH_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub relay: ConnectorRelay,
    pub store: ConnectorStore,
    pub plugin_management_client: PluginManagementClient,
    local_connector_config_center_client: ConfigClient,
    task_runner_config_center_client: Option<ConfigClient>,
    user_service_http: reqwest::Client,
    pub(crate) managed_requirements_signer: Option<Arc<ManagedRequirementsSigner>>,
    device_connect_nonces: Arc<Mutex<HashMap<String, i64>>>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let managed_requirements_signer = ManagedRequirementsSigner::load(&config)?;
        let local_connector_config_center_client =
            ConfigClient::from_env("local-connector-service").map_err(|error| {
                format!("initialize Local Connector control-plane config client failed: {error}")
            })?;
        let local_connector_snapshot =
            local_connector_config_center_client
                .load()
                .await
                .map_err(|error| {
                    format!("load Local Connector control-plane config snapshot failed: {error}")
                })?;
        let relay_signing_config =
            resolve_platform_relay_signing_config(&local_connector_snapshot)?;
        let remote_control_trust = resolve_remote_control_trust_bundle(&local_connector_snapshot)?;
        let relay_runtime_limits = resolve_relay_runtime_limits(&local_connector_snapshot)?;
        let platform_relay_signer = PlatformRelaySigner::load(&relay_signing_config)?;
        let store = ConnectorStore::connect(&config.database_url).await?;
        let plugin_management_config =
            PluginManagementClientConfig::from_env("local-connector-service")
                .await
                .map_err(|err| format!("load plugin management client config failed: {err}"))?;
        let plugin_management_client = PluginManagementClient::new(plugin_management_config)
            .map_err(|err| format!("initialize plugin management client failed: {err}"))?;
        let task_runner_config_center_client =
            initialize_config_center_client("task-runner", "Task Runner runtime").await;
        let user_service_http =
            build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
                .map_err(|err| format!("build user_service client failed: {err}"))?;
        if let Some(signer) = managed_requirements_signer.as_ref() {
            tracing::info!(
                key_id = signer.key_id(),
                public_key = signer.public_key(),
                "managed requirements bundle signing is enabled"
            );
        }
        tracing::info!(
            key_id = platform_relay_signer.key_id(),
            public_key = platform_relay_signer.public_key(),
            "local connector relay signing is enabled"
        );
        tracing::info!(
            require_signed_messages = remote_control_trust.require_signed_messages,
            trusted_key_count = remote_control_trust.trusted_relay_public_keys.len(),
            signature_max_skew_seconds = remote_control_trust.signature_max_skew_seconds,
            "local connector remote-control trust config is loaded from configuration center"
        );
        tracing::info!(
            max_pending_requests_per_device = relay_runtime_limits.max_pending_requests_per_device,
            terminal_max_event_bytes = relay_runtime_limits.terminal_max_event_bytes,
            terminal_event_channel_capacity = relay_runtime_limits.terminal_event_channel_capacity,
            "local connector relay runtime limits are loaded from configuration center"
        );
        let relay = ConnectorRelay::new(Some(platform_relay_signer), relay_runtime_limits);
        spawn_local_connector_runtime_config_watcher(
            local_connector_config_center_client.clone(),
            relay.clone(),
        );
        Ok(Self {
            config,
            relay,
            store,
            plugin_management_client,
            local_connector_config_center_client,
            task_runner_config_center_client,
            user_service_http,
            managed_requirements_signer,
            device_connect_nonces: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub(crate) fn user_service_http(&self) -> &reqwest::Client {
        &self.user_service_http
    }

    pub(crate) async fn managed_runtime_config_bundle(
        &self,
    ) -> Result<ManagedRuntimeConfigBundle, String> {
        let local_connector_snapshot = refresh_or_current_snapshot(
            &self.local_connector_config_center_client,
            "local-connector-service",
            "Local Connector control-plane config",
        )
        .await;
        let local_connector_snapshot = local_connector_snapshot.ok_or_else(|| {
            "Local Connector control-plane config snapshot is unavailable".to_string()
        })?;
        let remote_control_trust = resolve_remote_control_trust_bundle(&local_connector_snapshot)?;
        let task_runner_snapshot = if let Some(client) =
            self.task_runner_config_center_client.as_ref()
        {
            refresh_or_current_snapshot(client, "task-runner", "Task Runner runtime config").await
        } else {
            None
        };

        let mut bundle = if let Some(snapshot) = task_runner_snapshot {
            ManagedRuntimeConfigBundle::from_config_snapshot(snapshot)
        } else {
            let mut bundle = ManagedRuntimeConfigBundle::defaults();
            bundle.environment = local_connector_snapshot.environment.clone();
            bundle.revision = local_connector_snapshot.revision;
            bundle.checksum = format!(
                "local-connector:{}+task-runner:defaults",
                local_connector_snapshot.checksum
            );
            bundle.generated_at = local_connector_snapshot.generated_at.clone();
            bundle.stale = local_connector_snapshot.stale;
            bundle.source = Some(
                local_connector_snapshot
                    .source
                    .clone()
                    .unwrap_or_else(|| "configuration_center".to_string()),
            );
            bundle
        };
        bundle.remote_control_trust = remote_control_trust;
        Ok(bundle)
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

fn spawn_local_connector_runtime_config_watcher(client: ConfigClient, relay: ConnectorRelay) {
    tokio::spawn(async move {
        let mut snapshots = client.watch(LOCAL_CONNECTOR_CONFIG_WATCH_INTERVAL).await;
        while snapshots.changed().await.is_ok() {
            let Some(snapshot) = snapshots.borrow().clone() else {
                continue;
            };
            if let Err(error) = apply_local_connector_runtime_snapshot(&relay, &snapshot) {
                tracing::warn!(
                    error = error.as_str(),
                    "skip invalid Local Connector runtime config refresh; keeping previous relay settings"
                );
            }
        }
    });
}

fn apply_local_connector_runtime_snapshot(
    relay: &ConnectorRelay,
    snapshot: &chatos_config_sdk::ConfigSnapshot,
) -> Result<(), String> {
    let relay_signing_config = resolve_platform_relay_signing_config(snapshot)?;
    let remote_control_trust = resolve_remote_control_trust_bundle(snapshot)?;
    let relay_runtime_limits = resolve_relay_runtime_limits(snapshot)?;
    let platform_relay_signer = PlatformRelaySigner::load(&relay_signing_config)?;
    relay.update_runtime_config(Some(platform_relay_signer.clone()), relay_runtime_limits);
    tracing::info!(
        key_id = platform_relay_signer.key_id(),
        public_key = platform_relay_signer.public_key(),
        require_signed_messages = remote_control_trust.require_signed_messages,
        trusted_key_count = remote_control_trust.trusted_relay_public_keys.len(),
        signature_max_skew_seconds = remote_control_trust.signature_max_skew_seconds,
        max_pending_requests_per_device = relay_runtime_limits.max_pending_requests_per_device,
        terminal_max_event_bytes = relay_runtime_limits.terminal_max_event_bytes,
        terminal_event_channel_capacity = relay_runtime_limits.terminal_event_channel_capacity,
        "applied refreshed Local Connector relay runtime config from configuration center"
    );
    Ok(())
}

async fn refresh_or_current_snapshot(
    client: &ConfigClient,
    service_name: &'static str,
    label: &'static str,
) -> Option<chatos_config_sdk::ConfigSnapshot> {
    match client.refresh().await {
        Ok(Some(snapshot)) => Some(snapshot),
        Ok(None) => client.current().await,
        Err(error) => {
            tracing::warn!(
                service_name,
                error = error.as_str(),
                "refresh {label} failed; using last-known-good"
            );
            client.current().await
        }
    }
}

async fn initialize_config_center_client(
    service_name: &'static str,
    label: &'static str,
) -> Option<ConfigClient> {
    match ConfigClient::from_env(service_name) {
        Ok(client) => {
            if let Err(error) = client.load().await {
                tracing::warn!(
                    service_name,
                    error = error.as_str(),
                    "load {label} configuration snapshot failed; keeping managed defaults"
                );
            }
            Some(client)
        }
        Err(error) => {
            tracing::warn!(
                service_name,
                error = error.as_str(),
                "initialize {label} configuration client failed"
            );
            None
        }
    }
}
