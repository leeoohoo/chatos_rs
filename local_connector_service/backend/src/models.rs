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
pub const SANDBOX_READINESS_READY: &str = "ready";
pub const SANDBOX_READINESS_SETUP_REQUIRED: &str = "setup_required";
pub const SANDBOX_READINESS_UNSUPPORTED: &str = "unsupported";
pub const SANDBOX_READINESS_UNDER_DEVELOPMENT: &str = "under_development";
pub const PERMISSION_PROFILE_READ_ONLY: &str = "read_only";
pub const PERMISSION_PROFILE_WORKSPACE_WRITE: &str = "workspace_write";
pub const PERMISSION_PROFILE_FULL_ACCESS: &str = "full_access";
pub const APPROVAL_POLICY_ON_REQUEST: &str = "on_request";
pub const APPROVAL_POLICY_NEVER: &str = "never";
pub const APPROVAL_REVIEWER_USER: &str = "user";
pub const APPROVAL_REVIEWER_AUTO_REVIEW: &str = "auto_review";

pub const SESSION_STATUS_CONNECTED: &str = "connected";
pub const SESSION_STATUS_DISCONNECTED: &str = "disconnected";

pub const USER_ROLE_SUPER_ADMIN: &str = "super_admin";
pub const MANAGED_REQUIREMENTS_SCOPE_GLOBAL: &str = "global";
pub const MANAGED_REQUIREMENTS_SCOPE_ROLE: &str = "role";
pub const MANAGED_REQUIREMENTS_SCOPE_USER: &str = "user";

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

    pub fn is_super_admin(&self) -> bool {
        self.principal_type == "human_user" && self.role == USER_ROLE_SUPER_ADMIN
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
    #[serde(default = "default_sandbox_mode")]
    pub sandbox_mode: String,
    #[serde(default = "default_sandbox_readiness")]
    pub sandbox_readiness: String,
    #[serde(default = "default_permission_profile_id")]
    pub permission_profile_id: String,
    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,
    #[serde(default = "default_approval_reviewer")]
    pub approval_reviewer: String,
    #[serde(default)]
    pub policy_revision: Option<String>,
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
        sandbox_readiness: Option<String>,
        permission_profile_id: Option<String>,
        approval_policy: Option<String>,
        approval_reviewer: Option<String>,
        policy_revision: Option<String>,
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
            sandbox_readiness: normalize_sandbox_readiness(sandbox_readiness),
            permission_profile_id: normalize_permission_profile_id(permission_profile_id),
            approval_policy: normalize_approval_policy(approval_policy),
            approval_reviewer: normalize_approval_reviewer(approval_reviewer),
            policy_revision,
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
    match value.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == SANDBOX_MODE_LOCAL_PROCESS => {
            SANDBOX_MODE_LOCAL_PROCESS.to_string()
        }
        _ => SANDBOX_MODE_DOCKER.to_string(),
    }
}

pub fn normalize_sandbox_readiness(value: Option<String>) -> String {
    match value.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == SANDBOX_READINESS_READY => SANDBOX_READINESS_READY.to_string(),
        Some(value) if value == SANDBOX_READINESS_SETUP_REQUIRED => {
            SANDBOX_READINESS_SETUP_REQUIRED.to_string()
        }
        Some(value) if value == SANDBOX_READINESS_UNSUPPORTED => {
            SANDBOX_READINESS_UNSUPPORTED.to_string()
        }
        Some(value) if value == SANDBOX_READINESS_UNDER_DEVELOPMENT => {
            SANDBOX_READINESS_UNDER_DEVELOPMENT.to_string()
        }
        _ => SANDBOX_READINESS_READY.to_string(),
    }
}

pub fn normalize_permission_profile_id(value: Option<String>) -> String {
    match value.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == PERMISSION_PROFILE_READ_ONLY => {
            PERMISSION_PROFILE_READ_ONLY.to_string()
        }
        Some(value) if value == PERMISSION_PROFILE_FULL_ACCESS => {
            PERMISSION_PROFILE_FULL_ACCESS.to_string()
        }
        _ => PERMISSION_PROFILE_WORKSPACE_WRITE.to_string(),
    }
}

pub fn normalize_approval_policy(value: Option<String>) -> String {
    match value.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == APPROVAL_POLICY_NEVER => APPROVAL_POLICY_NEVER.to_string(),
        _ => APPROVAL_POLICY_ON_REQUEST.to_string(),
    }
}

pub fn normalize_approval_reviewer(value: Option<String>) -> String {
    match value.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == APPROVAL_REVIEWER_AUTO_REVIEW => {
            APPROVAL_REVIEWER_AUTO_REVIEW.to_string()
        }
        _ => APPROVAL_REVIEWER_USER.to_string(),
    }
}

fn default_sandbox_mode() -> String {
    SANDBOX_MODE_DOCKER.to_string()
}

fn default_sandbox_readiness() -> String {
    SANDBOX_READINESS_READY.to_string()
}

fn default_permission_profile_id() -> String {
    PERMISSION_PROFILE_WORKSPACE_WRITE.to_string()
}

fn default_approval_policy() -> String {
    APPROVAL_POLICY_ON_REQUEST.to_string()
}

fn default_approval_reviewer() -> String {
    APPROVAL_REVIEWER_USER.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_readiness_normalization_preserves_non_ready_states() {
        assert_eq!(
            normalize_sandbox_readiness(Some("setup_required".to_string())),
            SANDBOX_READINESS_SETUP_REQUIRED
        );
        assert_eq!(
            normalize_sandbox_readiness(Some("UNDER_DEVELOPMENT".to_string())),
            SANDBOX_READINESS_UNDER_DEVELOPMENT
        );
        assert_eq!(
            normalize_sandbox_readiness(Some(" unsupported ".to_string())),
            SANDBOX_READINESS_UNSUPPORTED
        );
        assert_eq!(
            normalize_sandbox_readiness(Some("ready".to_string())),
            SANDBOX_READINESS_READY
        );
    }

    #[test]
    fn sandbox_readiness_normalization_defaults_legacy_unknown_to_ready() {
        assert_eq!(normalize_sandbox_readiness(None), SANDBOX_READINESS_READY);
        assert_eq!(
            normalize_sandbox_readiness(Some(String::new())),
            SANDBOX_READINESS_READY
        );
        assert_eq!(
            normalize_sandbox_readiness(Some("docker_not_installed".to_string())),
            SANDBOX_READINESS_READY
        );
    }
}
