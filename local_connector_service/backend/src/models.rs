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

pub const BINDING_MODE_MCP: &str = "local_mcp";
pub const BINDING_MODE_TERMINAL: &str = "local_terminal";
pub const BINDING_MODE_SANDBOX: &str = "local_sandbox";

pub const SANDBOX_MODE_DOCKER: &str = "docker";
pub const SANDBOX_MODE_LOCAL_PROCESS: &str = "local_process";

pub const SESSION_STATUS_CONNECTED: &str = "connected";
pub const SESSION_STATUS_DISCONNECTED: &str = "disconnected";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub principal_type: String,
    pub user_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: String,
    pub owner_user_id: Option<String>,
}

impl CurrentUser {
    pub fn effective_owner_user_id(&self) -> &str {
        self.owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(self.user_id.as_str())
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
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
    pub status: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
pub struct LocalConnectorProjectBinding {
    pub id: String,
    pub owner_user_id: String,
    pub project_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub mode: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl LocalConnectorProjectBinding {
    pub fn new(
        owner_user_id: String,
        project_id: String,
        device_id: String,
        workspace_id: String,
        mode: String,
        enabled: bool,
    ) -> Self {
        let now = now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            owner_user_id,
            project_id,
            device_id,
            workspace_id,
            mode: normalize_binding_mode(Some(mode)),
            enabled,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorSandboxPairing {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub enabled: bool,
    pub sandbox_mode: String,
    pub facade_base_url: Option<String>,
    pub access_client_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl LocalConnectorSandboxPairing {
    pub fn new(
        owner_user_id: String,
        device_id: String,
        workspace_id: String,
        enabled: bool,
        sandbox_mode: String,
        facade_base_url: Option<String>,
        access_client_id: Option<String>,
    ) -> Self {
        let now = now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            owner_user_id,
            device_id,
            workspace_id,
            enabled,
            sandbox_mode: normalize_sandbox_mode(Some(sandbox_mode)),
            facade_base_url,
            access_client_id,
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

pub fn normalize_device_status(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some(DEVICE_STATUS_ONLINE) => DEVICE_STATUS_ONLINE.to_string(),
        Some(DEVICE_STATUS_OFFLINE) => DEVICE_STATUS_OFFLINE.to_string(),
        Some(DEVICE_STATUS_REVOKED) => DEVICE_STATUS_REVOKED.to_string(),
        _ => DEVICE_STATUS_REGISTERED.to_string(),
    }
}

pub fn normalize_workspace_status(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some(WORKSPACE_STATUS_DISABLED) => WORKSPACE_STATUS_DISABLED.to_string(),
        _ => WORKSPACE_STATUS_ACTIVE.to_string(),
    }
}

pub fn normalize_binding_mode(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some(BINDING_MODE_TERMINAL) => BINDING_MODE_TERMINAL.to_string(),
        Some(BINDING_MODE_SANDBOX) => BINDING_MODE_SANDBOX.to_string(),
        _ => BINDING_MODE_MCP.to_string(),
    }
}

pub fn normalize_sandbox_mode(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some(SANDBOX_MODE_LOCAL_PROCESS) => SANDBOX_MODE_LOCAL_PROCESS.to_string(),
        _ => SANDBOX_MODE_DOCKER.to_string(),
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
pub fn capabilities_from_json(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .map(normalize_capabilities)
        .unwrap_or_default()
}
