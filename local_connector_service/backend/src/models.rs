// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEVICE_STATUS_REGISTERED: &str = "registered";
pub const DEVICE_STATUS_ONLINE: &str = "online";
pub const DEVICE_STATUS_OFFLINE: &str = "offline";
pub const DEVICE_STATUS_REVOKED: &str = "revoked";

pub const WORKSPACE_STATUS_ACTIVE: &str = "active";
pub const WORKSPACE_STATUS_DISABLED: &str = "disabled";

pub const SESSION_STATUS_CONNECTED: &str = "connected";
pub const SESSION_STATUS_DISCONNECTED: &str = "disconnected";

pub const USER_ROLE_SUPER_ADMIN: &str = "super_admin";
pub const MANAGED_REQUIREMENTS_SCOPE_GLOBAL: &str = "global";
pub const MANAGED_REQUIREMENTS_SCOPE_ROLE: &str = "role";
pub const MANAGED_REQUIREMENTS_SCOPE_USER: &str = "user";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub principal_type: String,
    #[serde(default, skip_serializing)]
    pub token_jti: Option<String>,
    pub user_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: String,
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl CurrentUser {
    pub fn effective_owner_user_id(&self) -> &str {
        self.owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(self.user_id.as_str())
    }

    pub fn is_super_admin(&self) -> bool {
        self.principal_type == "human_user" && self.role == USER_ROLE_SUPER_ADMIN
    }

    pub fn is_wechat_companion(&self) -> bool {
        self.scopes.iter().any(|scope| scope == "wechat_companion")
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
}

#[derive(Debug, Serialize)]
pub struct LocalConnectorSystemStatsResponse {
    pub ok: bool,
    pub service: String,
    pub now: String,
    pub pressure_level: chatos_config_sdk::PlatformPressureLevel,
    pub relay: LocalConnectorRelayStats,
    pub store: LocalConnectorStoreStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalConnectorRelayStats {
    pub active_device_sessions: usize,
    pub pending_relay_requests: usize,
    pub max_pending_requests_per_device: usize,
    pub relay_signing_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalConnectorStoreStats {
    pub devices_total: usize,
    pub devices_online: usize,
    pub devices_offline: usize,
    pub devices_revoked: usize,
    pub workspaces_total: usize,
    pub active_sessions_total: usize,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorDevice {
    pub id: String,
    pub owner_user_id: String,
    pub display_name: String,
    pub public_key: String,
    pub client_version: Option<String>,
    pub os: Option<String>,
    #[serde(default, skip_serializing)]
    pub windows_user_sid: Option<String>,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionDeviceSummary {
    pub id: String,
    pub display_name: String,
    pub client_version: Option<String>,
    pub os: Option<String>,
    pub status: String,
    pub is_online: bool,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedRequirementsPolicy {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub requirements_toml: String,
    pub content_sha256: String,
    pub version: i64,
    pub enabled: bool,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedRequirementsAssignment {
    pub id: String,
    pub policy_id: String,
    pub scope: String,
    pub subject: Option<String>,
    pub priority: i32,
    pub enabled: bool,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ApplicableManagedRequirementsLayer {
    pub policy: ManagedRequirementsPolicy,
    pub assignment: ManagedRequirementsAssignment,
}

impl LocalConnectorDevice {
    pub fn new(
        owner_user_id: String,
        display_name: String,
        public_key: String,
        client_version: Option<String>,
        os: Option<String>,
    ) -> Self {
        let now = now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            owner_user_id,
            display_name,
            public_key,
            client_version,
            os,
            windows_user_sid: None,
            status: DEVICE_STATUS_REGISTERED.to_string(),
            last_seen_at: None,
            revoked_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorWorkspace {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub display_name: String,
    pub local_path_alias: String,
    pub local_path_fingerprint: String,
    pub capabilities: Vec<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl LocalConnectorWorkspace {
    pub fn new(
        owner_user_id: String,
        device_id: String,
        display_name: String,
        local_path_alias: String,
        local_path_fingerprint: String,
        capabilities: Vec<String>,
    ) -> Self {
        let now = now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            owner_user_id,
            device_id,
            display_name,
            local_path_alias,
            local_path_fingerprint,
            capabilities: normalize_capabilities(capabilities),
            status: WORKSPACE_STATUS_ACTIVE.to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorSession {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub connection_id: String,
    pub status: String,
    pub connected_at: String,
    pub last_heartbeat_at: String,
    pub expires_at: String,
    pub disconnected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl LocalConnectorSession {
    pub fn new(owner_user_id: String, device_id: String, lease_ttl: std::time::Duration) -> Self {
        let now = Utc::now();
        let now_text = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        let ttl = Duration::from_std(lease_ttl).unwrap_or_else(|_| Duration::seconds(90));
        Self {
            id: Uuid::new_v4().to_string(),
            owner_user_id,
            device_id,
            connection_id: Uuid::new_v4().to_string(),
            status: SESSION_STATUS_CONNECTED.to_string(),
            connected_at: now_text.clone(),
            last_heartbeat_at: now_text.clone(),
            expires_at: (now + ttl).to_rfc3339_opts(SecondsFormat::Millis, true),
            disconnected_at: None,
            created_at: now_text.clone(),
            updated_at: now_text,
        }
    }
}

pub fn lease_deadline_rfc3339(lease_ttl: std::time::Duration) -> String {
    let ttl = Duration::from_std(lease_ttl).unwrap_or_else(|_| Duration::seconds(90));
    (Utc::now() + ttl).to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn lease_now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub fn normalize_workspace_status(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some(WORKSPACE_STATUS_DISABLED) => WORKSPACE_STATUS_DISABLED.to_string(),
        _ => WORKSPACE_STATUS_ACTIVE.to_string(),
    }
}

pub fn normalize_capabilities(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() || out.iter().any(|item: &String| item == &normalized) {
            continue;
        }
        out.push(normalized);
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_api_serialization_never_exposes_registered_windows_sid() {
        let mut device = LocalConnectorDevice::new(
            "owner-1".to_string(),
            "Windows PC".to_string(),
            "ed25519:key".to_string(),
            Some("1.0.0-windows".to_string()),
            Some("Windows".to_string()),
        );
        device.windows_user_sid = Some("S-1-5-21-100-200-300-400".to_string());

        let serialized = serde_json::to_value(device).expect("serialize device");

        assert!(serialized.get("windows_user_sid").is_none());
    }
}
