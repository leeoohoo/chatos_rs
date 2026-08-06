// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chatos_agent::ManagedRuntimeConfigBundle;
use chatos_config_sdk::ConfigClient;
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use futures::StreamExt;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::managed_config::{
    resolve_platform_relay_signing_config, resolve_relay_runtime_limits,
    resolve_remote_control_trust_bundle,
};
use crate::managed_requirements::ManagedRequirementsSigner;
use crate::pressure::LocalConnectorPressureState;
use crate::relay::{ConnectorRelay, InterInstanceRelayMessage};
use crate::relay_signature::{validate_active_relay_signer_trust, PlatformRelaySigner};
use crate::store::ConnectorStore;
use crate::valkey_coordination::{DevicePresence, ValkeyCoordinator};
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};

const LOCAL_CONNECTOR_CONFIG_WATCH_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub relay: ConnectorRelay,
    pub store: ConnectorStore,
    pub plugin_management_client: PluginManagementClient,
    local_connector_config_center_client: ConfigClient,
    user_service_http: reqwest::Client,
    pub(crate) managed_requirements_signer: Option<Arc<ManagedRequirementsSigner>>,
    pub(crate) pressure: LocalConnectorPressureState,
    instance_id: String,
    valkey: ValkeyCoordinator,
}

impl AppState {
    pub async fn new(
        config: AppConfig,
        pressure: LocalConnectorPressureState,
    ) -> Result<Self, String> {
        let managed_requirements_signer = ManagedRequirementsSigner::load(&config)?;
        let local_connector_config_center_client =
            ConfigClient::from_env("local-connector-service").map_err(|error| {
                format!("initialize Local Connector control-plane config client failed: {error}")
            })?;
        let local_connector_snapshot = local_connector_config_center_client
            .load_strict()
            .await
            .map_err(|error| {
                format!("load fresh Local Connector control-plane config snapshot failed: {error}")
            })?;
        let relay_signing_config =
            resolve_platform_relay_signing_config(&local_connector_snapshot)?;
        let remote_control_trust = resolve_remote_control_trust_bundle(&local_connector_snapshot)?;
        let relay_runtime_limits = resolve_relay_runtime_limits(&local_connector_snapshot)?;
        let platform_relay_signer = PlatformRelaySigner::load(&relay_signing_config)?;
        validate_active_relay_signer_trust(&platform_relay_signer, &remote_control_trust)?;
        let store = ConnectorStore::connect(&config.database_url).await?;
        let instance_id = format!("local-connector-{}", Uuid::new_v4());
        let valkey = ValkeyCoordinator::connect(
            config.valkey_url.as_str(),
            config.valkey_key_prefix.as_str(),
            config.device_presence_ttl,
            config.terminal_subscriber_ttl,
        )
        .await?;
        let plugin_management_config =
            PluginManagementClientConfig::from_env("local-connector-service")
                .await
                .map_err(|err| format!("load plugin management client config failed: {err}"))?;
        let plugin_management_client = PluginManagementClient::new(plugin_management_config)
            .map_err(|err| format!("initialize plugin management client failed: {err}"))?;
        chatos_agent::require_task_runner_runtime_settings(&local_connector_snapshot)?;
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
            terminal_max_active_sessions = relay_runtime_limits.terminal_max_active_sessions,
            terminal_new_session_soft_limit = relay_runtime_limits.terminal_new_session_soft_limit,
            terminal_max_subscribers_per_session =
                relay_runtime_limits.terminal_max_subscribers_per_session,
            "local connector relay runtime limits are loaded from configuration center"
        );
        let relay = ConnectorRelay::new_distributed(
            Some(platform_relay_signer),
            relay_runtime_limits,
            instance_id.clone(),
            valkey.clone(),
            config.relay_correlation_grace_ttl,
            config.relay_delivery_ack_timeout,
        );
        relay.set_platform_pressure_level(pressure.snapshot().level);
        spawn_local_connector_runtime_config_watcher(
            local_connector_config_center_client.clone(),
            relay.clone(),
        );
        spawn_valkey_relay_listener(
            valkey.clone(),
            instance_id.clone(),
            relay.clone(),
            config.valkey_reconnect_delay,
        );
        tracing::info!(
            instance_id = instance_id.as_str(),
            "Local Connector distributed relay instance registered"
        );
        Ok(Self {
            config,
            relay,
            store,
            plugin_management_client,
            local_connector_config_center_client,
            user_service_http,
            managed_requirements_signer,
            pressure,
            instance_id,
            valkey,
        })
    }

    pub(crate) fn user_service_http(&self) -> &reqwest::Client {
        &self.user_service_http
    }

    pub(crate) async fn managed_runtime_config_bundle(
        &self,
    ) -> Result<ManagedRuntimeConfigBundle, String> {
        let local_connector_snapshot = self
            .local_connector_config_center_client
            .load_strict()
            .await
            .map_err(|error| {
                format!("load fresh Local Connector control-plane config snapshot failed: {error}")
            })?;
        let remote_control_trust = resolve_remote_control_trust_bundle(&local_connector_snapshot)?;
        let active_relay_signer = self
            .relay
            .active_signer()
            .ok_or_else(|| "active relay signer is unavailable".to_string())?;
        validate_active_relay_signer_trust(&active_relay_signer, &remote_control_trust)?;
        let task_runner_runtime_settings =
            chatos_agent::require_task_runner_runtime_settings(&local_connector_snapshot)?;
        Ok(ManagedRuntimeConfigBundle {
            environment: local_connector_snapshot.environment,
            revision: local_connector_snapshot.revision,
            checksum: local_connector_snapshot.checksum,
            generated_at: local_connector_snapshot.generated_at,
            stale: false,
            source: Some("configuration_center".to_string()),
            task_runner_runtime_settings,
            remote_control_trust,
        })
    }

    pub async fn consume_device_connect_nonce(
        &self,
        device_id: &str,
        nonce: &str,
    ) -> Result<bool, String> {
        self.valkey
            .consume_device_nonce(
                device_id,
                nonce,
                self.config.device_connect_signature_max_skew,
            )
            .await
    }

    pub(crate) async fn register_device_presence(
        &self,
        owner_user_id: &str,
        device_id: &str,
        session_id: &str,
    ) -> Result<DevicePresence, String> {
        let presence = DevicePresence {
            instance_id: self.instance_id.clone(),
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            session_id: session_id.to_string(),
        };
        self.valkey.register_device_presence(&presence).await?;
        Ok(presence)
    }

    pub(crate) async fn refresh_device_presence(
        &self,
        presence: &DevicePresence,
    ) -> Result<bool, String> {
        self.valkey.refresh_device_presence(presence).await
    }

    pub(crate) async fn unregister_device_presence(
        &self,
        presence: &DevicePresence,
    ) -> Result<bool, String> {
        self.valkey.unregister_device_presence(presence).await
    }

    pub(crate) async fn device_presence(
        &self,
        device_id: &str,
    ) -> Result<Option<DevicePresence>, String> {
        self.valkey.device_presence(device_id).await
    }
}

fn spawn_valkey_relay_listener(
    coordinator: ValkeyCoordinator,
    instance_id: String,
    relay: ConnectorRelay,
    reconnect_delay: Duration,
) {
    tokio::spawn(async move {
        loop {
            match coordinator.subscribe_instance(instance_id.as_str()).await {
                Ok(mut pubsub) => {
                    tracing::info!(
                        instance_id = instance_id.as_str(),
                        "Local Connector subscribed to its Valkey relay channel"
                    );
                    let mut messages = pubsub.on_message();
                    while let Some(message) = messages.next().await {
                        let payload = match message.get_payload::<String>() {
                            Ok(payload) => payload,
                            Err(error) => {
                                tracing::warn!(
                                    instance_id = instance_id.as_str(),
                                    error = error.to_string().as_str(),
                                    "decode Local Connector instance message failed"
                                );
                                continue;
                            }
                        };
                        let message = match serde_json::from_str::<InterInstanceRelayMessage>(
                            payload.as_str(),
                        ) {
                            Ok(message) => message,
                            Err(error) => {
                                tracing::warn!(
                                    instance_id = instance_id.as_str(),
                                    error = error.to_string().as_str(),
                                    "parse Local Connector instance message failed"
                                );
                                continue;
                            }
                        };
                        if let Err(error) = relay.handle_inter_instance_message(message).await {
                            tracing::warn!(
                                instance_id = instance_id.as_str(),
                                error = error.as_str(),
                                "handle Local Connector instance message failed"
                            );
                        }
                    }
                }
                Err(error) => tracing::warn!(
                    instance_id = instance_id.as_str(),
                    error = error.as_str(),
                    "subscribe Local Connector Valkey relay channel failed"
                ),
            }
            tokio::time::sleep(reconnect_delay).await;
        }
    });
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
    validate_active_relay_signer_trust(&platform_relay_signer, &remote_control_trust)?;
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
        terminal_max_active_sessions = relay_runtime_limits.terminal_max_active_sessions,
        terminal_new_session_soft_limit = relay_runtime_limits.terminal_new_session_soft_limit,
        terminal_max_subscribers_per_session =
            relay_runtime_limits.terminal_max_subscribers_per_session,
        "applied refreshed Local Connector relay runtime config from configuration center"
    );
    Ok(())
}
