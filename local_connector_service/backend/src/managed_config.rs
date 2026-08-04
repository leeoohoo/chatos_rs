// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::PathBuf;

use chatos_agent::RemoteControlTrustConfigBundle;
use chatos_config_sdk::ConfigSnapshot;
use serde_json::Value;

pub(crate) const LOCAL_CONNECTOR_RELAY_SIGNING_KEY_PATH_CONFIG_KEY: &str =
    "local_connector.security.relay_signing.key_path";
pub(crate) const LOCAL_CONNECTOR_RELAY_SIGNING_KEY_ID_CONFIG_KEY: &str =
    "local_connector.security.relay_signing.key_id";
pub(crate) const LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY: &str =
    "local_connector.remote_control.require_signed_messages";
pub(crate) const LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY: &str =
    "local_connector.remote_control.signature_max_skew_seconds";
pub(crate) const LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY: &str =
    "local_connector.remote_control.trusted_relay_public_keys";
pub(crate) const LOCAL_CONNECTOR_RELAY_MAX_PENDING_REQUESTS_PER_DEVICE_CONFIG_KEY: &str =
    "local_connector.relay.max_pending_requests_per_device";
pub(crate) const LOCAL_CONNECTOR_TERMINAL_MAX_EVENT_BYTES_CONFIG_KEY: &str =
    "local_connector.terminal.max_event_bytes";
pub(crate) const LOCAL_CONNECTOR_TERMINAL_EVENT_CHANNEL_CAPACITY_CONFIG_KEY: &str =
    "local_connector.terminal.event_channel_capacity";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlatformRelaySigningConfig {
    pub(crate) key_path: PathBuf,
    pub(crate) key_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RelayRuntimeLimits {
    pub(crate) max_pending_requests_per_device: usize,
    pub(crate) terminal_max_event_bytes: usize,
    pub(crate) terminal_event_channel_capacity: usize,
}

impl Default for RelayRuntimeLimits {
    fn default() -> Self {
        Self {
            max_pending_requests_per_device: 256,
            terminal_max_event_bytes: 131_072,
            terminal_event_channel_capacity: 1024,
        }
    }
}

pub(crate) fn resolve_platform_relay_signing_config(
    snapshot: &ConfigSnapshot,
) -> Result<PlatformRelaySigningConfig, String> {
    let key_path = snapshot
        .string(LOCAL_CONNECTOR_RELAY_SIGNING_KEY_PATH_CONFIG_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_RELAY_SIGNING_KEY_PATH_CONFIG_KEY} must be provided by config center"
            )
        })?;
    let key_id = snapshot
        .string(LOCAL_CONNECTOR_RELAY_SIGNING_KEY_ID_CONFIG_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_RELAY_SIGNING_KEY_ID_CONFIG_KEY} must be provided by config center"
            )
        })?;
    Ok(PlatformRelaySigningConfig {
        key_path: PathBuf::from(key_path),
        key_id,
    })
}

pub(crate) fn resolve_remote_control_trust_bundle(
    snapshot: &ConfigSnapshot,
) -> Result<RemoteControlTrustConfigBundle, String> {
    let require_signed_messages = snapshot
        .bool(LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY} must be provided by config center"
            )
        })?;
    let signature_max_skew_seconds = snapshot
        .u64(LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY} must be provided by config center"
            )
        })?
        .clamp(30, 3600);
    let trusted_relay_public_keys = snapshot
        .value(LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY} must be provided by config center"
            )
        })
        .and_then(parse_trusted_relay_public_keys)?;
    if require_signed_messages && trusted_relay_public_keys.is_empty() {
        return Err(format!(
            "{LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY} must contain at least one trusted key when signed remote control is enabled"
        ));
    }
    Ok(RemoteControlTrustConfigBundle {
        require_signed_messages,
        signature_max_skew_seconds,
        trusted_relay_public_keys,
    })
}

pub(crate) fn resolve_relay_runtime_limits(
    snapshot: &ConfigSnapshot,
) -> Result<RelayRuntimeLimits, String> {
    let max_pending_requests_per_device = snapshot
        .usize(LOCAL_CONNECTOR_RELAY_MAX_PENDING_REQUESTS_PER_DEVICE_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_RELAY_MAX_PENDING_REQUESTS_PER_DEVICE_CONFIG_KEY} must be provided by config center"
            )
        })?
        .clamp(1, 100_000);
    let terminal_max_event_bytes = snapshot
        .usize(LOCAL_CONNECTOR_TERMINAL_MAX_EVENT_BYTES_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_TERMINAL_MAX_EVENT_BYTES_CONFIG_KEY} must be provided by config center"
            )
        })?
        .clamp(1024, 8 * 1024 * 1024);
    let terminal_event_channel_capacity = snapshot
        .usize(LOCAL_CONNECTOR_TERMINAL_EVENT_CHANNEL_CAPACITY_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_TERMINAL_EVENT_CHANNEL_CAPACITY_CONFIG_KEY} must be provided by config center"
            )
        })?
        .clamp(1, 65_536);
    Ok(RelayRuntimeLimits {
        max_pending_requests_per_device,
        terminal_max_event_bytes,
        terminal_event_channel_capacity,
    })
}

fn parse_trusted_relay_public_keys(value: &Value) -> Result<BTreeMap<String, String>, String> {
    let object = value.as_object().ok_or_else(|| {
        format!(
            "{LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY} must be a JSON object"
        )
    })?;
    let mut keys = BTreeMap::new();
    for (key_id, public_key) in object {
        let public_key = public_key.as_str().ok_or_else(|| {
            format!(
                "{LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY}.{key_id} must be a string"
            )
        })?;
        let key_id = key_id.trim();
        let public_key = public_key.trim();
        if key_id.is_empty() {
            return Err(format!(
                "{LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY} contains an empty key id"
            ));
        }
        if public_key.is_empty() {
            return Err(format!(
                "{LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY}.{key_id} must not be empty"
            ));
        }
        keys.insert(key_id.to_string(), public_key.to_string());
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chatos_config_sdk::ConfigSnapshot;
    use serde_json::json;

    use super::*;

    fn snapshot(values: BTreeMap<String, Value>) -> ConfigSnapshot {
        ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "local-connector-service".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values,
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: Some("test".to_string()),
        }
    }

    #[test]
    fn resolves_remote_control_trust_bundle_from_config_snapshot() {
        let trust = resolve_remote_control_trust_bundle(&snapshot(BTreeMap::from([
            (
                LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY.to_string(),
                json!(true),
            ),
            (
                LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY.to_string(),
                json!(120),
            ),
            (
                LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY.to_string(),
                json!({"relay-key-1": "ed25519:test"}),
            ),
        ])))
        .expect("trust bundle");

        assert!(trust.require_signed_messages);
        assert_eq!(trust.signature_max_skew_seconds, 120);
        assert_eq!(
            trust.trusted_relay_public_keys.get("relay-key-1"),
            Some(&"ed25519:test".to_string())
        );
    }

    #[test]
    fn resolves_relay_runtime_limits_from_config_snapshot() {
        let limits = resolve_relay_runtime_limits(&snapshot(BTreeMap::from([
            (
                LOCAL_CONNECTOR_RELAY_MAX_PENDING_REQUESTS_PER_DEVICE_CONFIG_KEY.to_string(),
                json!(321),
            ),
            (
                LOCAL_CONNECTOR_TERMINAL_MAX_EVENT_BYTES_CONFIG_KEY.to_string(),
                json!(262144),
            ),
            (
                LOCAL_CONNECTOR_TERMINAL_EVENT_CHANNEL_CAPACITY_CONFIG_KEY.to_string(),
                json!(4096),
            ),
        ])))
        .expect("relay runtime limits");

        assert_eq!(limits.max_pending_requests_per_device, 321);
        assert_eq!(limits.terminal_max_event_bytes, 262_144);
        assert_eq!(limits.terminal_event_channel_capacity, 4096);
    }
}
