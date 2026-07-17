// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use memory_engine_sdk::ManagedMemoryPolicyBundle;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::approval::ApprovalState;
use crate::history::CommandHistoryEntry;
use crate::model_configs::ModelConfigState;
use crate::sandbox::types::LocalSandboxState;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct LocalState {
    #[serde(default)]
    pub(crate) auth: Option<AuthState>,
    #[serde(default)]
    pub(crate) paired_cloud_base_url: Option<String>,
    #[serde(default)]
    pub(crate) paired_user_id: Option<String>,
    pub(crate) device_id: Option<String>,
    pub(crate) device_public_key: Option<String>,
    #[serde(default)]
    pub(crate) workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    pub(crate) sandbox: LocalSandboxState,
    #[serde(default)]
    pub(crate) command_history: Vec<CommandHistoryEntry>,
    #[serde(default)]
    pub(crate) approval: ApprovalState,
    #[serde(default)]
    pub(crate) model_configs: ModelConfigState,
    #[serde(default)]
    pub(crate) runtime_settings: LocalRuntimeSettings,
    #[serde(default)]
    pub(crate) managed_memory_policy: Option<ManagedMemoryPolicyCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ManagedMemoryPolicyCache {
    pub(crate) source_instance_id: String,
    pub(crate) bundle: ManagedMemoryPolicyBundle,
    pub(crate) last_synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthState {
    pub(crate) cloud_base_url: String,
    pub(crate) user_service_base_url: String,
    pub(crate) access_token: String,
    pub(crate) device_name: String,
    pub(crate) user: Option<AuthUserState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthUserState {
    pub(crate) id: String,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkspaceState {
    pub(crate) id: String,
    pub(crate) absolute_root: PathBuf,
    pub(crate) alias: String,
    pub(crate) fingerprint: String,
    #[serde(default)]
    pub(crate) project_config_trust: Option<WorkspaceProjectConfigTrust>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkspaceProjectConfigTrust {
    pub(crate) identity_fingerprint: String,
    pub(crate) trusted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalRuntimeSettings {
    #[serde(default)]
    pub(crate) developer_mode: bool,
    #[serde(default = "default_developer_cloud_base_url")]
    pub(crate) developer_cloud_base_url: String,
    #[serde(default = "default_developer_user_service_base_url")]
    pub(crate) developer_user_service_base_url: String,
    #[serde(default = "default_developer_chatos_web_url")]
    pub(crate) developer_chatos_web_url: String,
}

impl Default for LocalRuntimeSettings {
    fn default() -> Self {
        Self {
            developer_mode: false,
            developer_cloud_base_url: default_developer_cloud_base_url(),
            developer_user_service_base_url: default_developer_user_service_base_url(),
            developer_chatos_web_url: default_developer_chatos_web_url(),
        }
    }
}

impl LocalRuntimeSettings {
    pub(crate) fn normalized(mut self) -> Self {
        self.developer_cloud_base_url = normalize_local_developer_url(
            self.developer_cloud_base_url,
            default_developer_cloud_base_url(),
        );
        self.developer_user_service_base_url = normalize_local_developer_url(
            self.developer_user_service_base_url,
            default_developer_user_service_base_url(),
        );
        self.developer_chatos_web_url = normalize_local_developer_url(
            self.developer_chatos_web_url,
            default_developer_chatos_web_url(),
        );
        self
    }
}

fn default_developer_cloud_base_url() -> String {
    "http://127.0.0.1:39230".to_string()
}

fn default_developer_user_service_base_url() -> String {
    "http://127.0.0.1:39190".to_string()
}

fn default_developer_chatos_web_url() -> String {
    "http://127.0.0.1:8088".to_string()
}

fn normalize_local_developer_url(value: String, fallback: String) -> String {
    let value = value.trim().trim_end_matches('/');
    if value.starts_with("http://127.0.0.1:") || value.starts_with("http://localhost:") {
        value.to_string()
    } else {
        fallback
    }
}

impl LocalState {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("read state file {}", path.display()))?;
        let mut value = serde_json::from_str::<Value>(content.as_str())
            .with_context(|| format!("parse state file {}", path.display()))?;
        if remove_legacy_runtime_settings(&mut value) {
            write_state_value(path, &value)?;
        }
        serde_json::from_value(value)
            .with_context(|| format!("parse state file {}", path.display()))
    }

    pub(crate) fn save(&self, path: &Path) -> Result<()> {
        write_state_value(path, &serde_json::to_value(self)?)
    }

    pub(crate) fn workspace_by_id(&self, workspace_id: &str) -> Option<&WorkspaceState> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
    }

    pub(crate) fn workspace_index_by_fingerprint(&self, fingerprint: &str) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|workspace| workspace.fingerprint == fingerprint)
    }

    pub(crate) fn pairing_context_matches(&self, cloud_base_url: &str, user_id: &str) -> bool {
        let stored_cloud_base_url = self
            .paired_cloud_base_url
            .as_deref()
            .or_else(|| self.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()));
        let stored_user_id = self.paired_user_id.as_deref().or_else(|| {
            self.auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref().map(|user| user.id.as_str()))
        });
        matches!(
            (stored_cloud_base_url, stored_user_id),
            (Some(stored_cloud_base_url), Some(stored_user_id))
                if stored_cloud_base_url == cloud_base_url && stored_user_id == user_id
        )
    }
}

fn remove_legacy_runtime_settings(value: &mut Value) -> bool {
    value
        .get_mut("runtime_settings")
        .and_then(Value::as_object_mut)
        .is_some_and(|settings| settings.remove("ai_agent_max_iterations").is_some())
}

fn write_state_value(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create state dir {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, content).with_context(|| format!("write state file {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restrict state file permissions {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_mode_defaults_to_local_stack_endpoints() {
        let settings = LocalRuntimeSettings::default();
        assert!(!settings.developer_mode);
        assert_eq!(settings.developer_cloud_base_url, "http://127.0.0.1:39230");
        assert_eq!(settings.developer_chatos_web_url, "http://127.0.0.1:8088");
    }

    #[test]
    fn removes_legacy_user_configured_agent_iteration_limit() {
        let mut value = serde_json::json!({
            "runtime_settings": {
                "ai_agent_max_iterations": 25,
                "developer_mode": false
            },
            "future_field": { "keep": true }
        });

        assert!(remove_legacy_runtime_settings(&mut value));
        assert_eq!(
            value.pointer("/runtime_settings/ai_agent_max_iterations"),
            None
        );
        assert_eq!(
            value.pointer("/future_field/keep"),
            Some(&Value::Bool(true))
        );
    }

    #[test]
    fn developer_endpoints_cannot_be_changed_to_remote_hosts() {
        let settings = LocalRuntimeSettings {
            developer_mode: true,
            developer_cloud_base_url: "https://unexpected.example.com".to_string(),
            developer_user_service_base_url: "http://192.168.1.5:39190".to_string(),
            developer_chatos_web_url: "javascript:alert(1)".to_string(),
            ..LocalRuntimeSettings::default()
        }
        .normalized();
        assert_eq!(settings.developer_cloud_base_url, "http://127.0.0.1:39230");
        assert_eq!(
            settings.developer_user_service_base_url,
            "http://127.0.0.1:39190"
        );
        assert_eq!(settings.developer_chatos_web_url, "http://127.0.0.1:8088");
    }

    #[test]
    fn legacy_mcp_configs_are_not_persisted_back_to_state_json() {
        let state = serde_json::from_value::<LocalState>(serde_json::json!({
            "mcp_configs": {"manifests": [{"manifest_id": "legacy"}]}
        }))
        .expect("legacy state remains readable");
        let value = serde_json::to_value(state).expect("serialize current state");
        assert!(value.get("mcp_configs").is_none());
    }
}
