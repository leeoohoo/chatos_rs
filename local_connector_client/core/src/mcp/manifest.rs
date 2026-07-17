// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::LocalState;

pub(crate) const MASKED_SECRET_VALUE: &str = "********";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LocalMcpTransport {
    Stdio,
    Http,
}

impl LocalMcpTransport {
    pub(crate) fn runtime_kind(self) -> &'static str {
        match self {
            Self::Stdio => "local_connector_stdio",
            Self::Http => "local_connector_http",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct LocalMcpStdioConfig {
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalMcpHttpConfig {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
    #[serde(default = "default_http_timeout_ms")]
    pub(crate) timeout_ms: u64,
}

impl Default for LocalMcpHttpConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            headers: BTreeMap::new(),
            timeout_ms: default_http_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalMcpManifestRecord {
    pub(crate) manifest_id: String,
    pub(crate) plugin_mcp_id: Option<String>,
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) internal_name: String,
    pub(crate) display_name: String,
    pub(crate) description: Option<String>,
    pub(crate) transport: LocalMcpTransport,
    pub(crate) stdio: Option<LocalMcpStdioConfig>,
    pub(crate) http: Option<LocalMcpHttpConfig>,
    pub(crate) enabled: bool,
    pub(crate) sync_status: String,
    pub(crate) last_check_status: String,
    pub(crate) last_checked_at: Option<String>,
    pub(crate) last_error: Option<String>,
    #[serde(default)]
    pub(crate) tool_snapshot: Vec<Value>,
    pub(crate) manifest_hash: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LocalMcpConfigDraft {
    pub(crate) manifest_id: Option<String>,
    pub(crate) display_name: String,
    pub(crate) description: Option<String>,
    pub(crate) transport: LocalMcpTransport,
    pub(crate) enabled: Option<bool>,
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalMcpManifestPublic {
    pub(crate) manifest_id: String,
    pub(crate) plugin_mcp_id: Option<String>,
    pub(crate) internal_name: String,
    pub(crate) display_name: String,
    pub(crate) description: Option<String>,
    pub(crate) transport: LocalMcpTransport,
    pub(crate) command: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) url: Option<String>,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) enabled: bool,
    pub(crate) sync_status: String,
    pub(crate) last_check_status: String,
    pub(crate) last_checked_at: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) tool_count: usize,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

impl LocalMcpManifestRecord {
    pub(crate) fn public_value(&self) -> LocalMcpManifestPublic {
        LocalMcpManifestPublic {
            manifest_id: self.manifest_id.clone(),
            plugin_mcp_id: self.plugin_mcp_id.clone(),
            internal_name: self.internal_name.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            transport: self.transport,
            command: self.stdio.as_ref().map(|config| config.command.clone()),
            args: self
                .stdio
                .as_ref()
                .map(|config| config.args.clone())
                .unwrap_or_default(),
            env: self
                .stdio
                .as_ref()
                .map(|config| masked_map(&config.env))
                .unwrap_or_default(),
            url: self.http.as_ref().map(|config| config.url.clone()),
            headers: self
                .http
                .as_ref()
                .map(|config| masked_map(&config.headers))
                .unwrap_or_default(),
            timeout_ms: self.http.as_ref().map(|config| config.timeout_ms),
            enabled: self.enabled,
            sync_status: self.sync_status.clone(),
            last_check_status: self.last_check_status.clone(),
            last_checked_at: self.last_checked_at.clone(),
            last_error: self.last_error.clone(),
            tool_count: self.tool_snapshot.len(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    pub(crate) fn refresh_hash(&mut self) -> Result<()> {
        let payload = json!({
            "manifest_id": self.manifest_id,
            "owner_user_id": self.owner_user_id,
            "device_id": self.device_id,
            "internal_name": self.internal_name,
            "transport": self.transport,
            "stdio": self.stdio,
            "http": self.http,
            "enabled": self.enabled,
        });
        let encoded = serde_json::to_vec(&payload)?;
        self.manifest_hash = hex::encode(Sha256::digest(encoded));
        Ok(())
    }

    pub(crate) fn is_executable(&self) -> bool {
        self.enabled
            && self.sync_status == "synced"
            && self.last_check_status == "available"
            && self.plugin_mcp_id.is_some()
    }

    pub(crate) fn is_locally_executable(&self) -> bool {
        self.enabled && self.last_check_status == "available"
    }
}

pub(crate) fn current_owner_user_id(state: &LocalState) -> Option<&str> {
    state
        .auth
        .as_ref()
        .and_then(|auth| auth.user.as_ref())
        .map(|user| user.id.as_str())
        .or(state.paired_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn current_device_id(state: &LocalState) -> Option<&str> {
    state
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn validate_manifest_for_execution(
    manifest: &LocalMcpManifestRecord,
    owner_user_id: &str,
    device_id: &str,
    manifest_id: &str,
    plugin_mcp_id: &str,
) -> Result<()> {
    if manifest.owner_user_id != owner_user_id
        || manifest.device_id != device_id
        || manifest.manifest_id != manifest_id
        || manifest.plugin_mcp_id.as_deref() != Some(plugin_mcp_id)
    {
        return Err(anyhow!(
            "Local Connector MCP manifest identity does not match"
        ));
    }
    if !manifest.is_executable() {
        return Err(anyhow!(
            "Local Connector MCP manifest is disabled or unavailable"
        ));
    }
    Ok(())
}

pub(crate) fn merge_masked_map(
    incoming: BTreeMap<String, String>,
    existing: Option<&BTreeMap<String, String>>,
) -> BTreeMap<String, String> {
    incoming
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_string();
            if key.is_empty() {
                return None;
            }
            if value == MASKED_SECRET_VALUE {
                return existing
                    .and_then(|existing| existing.get(key.as_str()))
                    .cloned()
                    .map(|value| (key, value));
            }
            Some((key, value))
        })
        .collect()
}

pub(crate) fn mcp_status_message(manifests: &[LocalMcpManifestRecord]) -> Value {
    let items = manifests
        .iter()
        .filter(|manifest| manifest.enabled && manifest.plugin_mcp_id.is_some())
        .map(|manifest| {
            json!({
                "plugin_mcp_id": manifest.plugin_mcp_id,
                "manifest_id": manifest.manifest_id,
                "status": manifest.last_check_status,
                "last_error": manifest.last_error,
                "tool_snapshot": manifest.tool_snapshot,
                "manifest_hash": manifest.manifest_hash,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "type": "mcp_manifest_status",
        "items": items,
    })
}

fn masked_map(values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .keys()
        .map(|key| (key.clone(), MASKED_SECRET_VALUE.to_string()))
        .collect()
}

fn default_http_timeout_ms() -> u64 {
    15_000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable_manifest() -> LocalMcpManifestRecord {
        LocalMcpManifestRecord {
            manifest_id: "manifest-1".to_string(),
            plugin_mcp_id: Some("plugin-1".to_string()),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            internal_name: "user_mcp_manifest1".to_string(),
            display_name: "Demo".to_string(),
            description: None,
            transport: LocalMcpTransport::Stdio,
            stdio: Some(LocalMcpStdioConfig {
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                env: BTreeMap::from([("TOKEN".to_string(), "secret".to_string())]),
            }),
            http: None,
            enabled: true,
            sync_status: "synced".to_string(),
            last_check_status: "available".to_string(),
            last_checked_at: Some("now".to_string()),
            last_error: None,
            tool_snapshot: vec![json!({"name": "demo"})],
            manifest_hash: "hash-1".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn masked_values_preserve_existing_local_secrets() {
        let existing = BTreeMap::from([
            ("TOKEN".to_string(), "old-secret".to_string()),
            ("REMOVED".to_string(), "removed-secret".to_string()),
        ]);
        let incoming = BTreeMap::from([
            ("TOKEN".to_string(), MASKED_SECRET_VALUE.to_string()),
            ("NEW_TOKEN".to_string(), "new-secret".to_string()),
            (" ".to_string(), "ignored".to_string()),
        ]);

        assert_eq!(
            merge_masked_map(incoming, Some(&existing)),
            BTreeMap::from([
                ("NEW_TOKEN".to_string(), "new-secret".to_string()),
                ("TOKEN".to_string(), "old-secret".to_string()),
            ])
        );
    }

    #[test]
    fn public_manifest_masks_secret_values() {
        let public = executable_manifest().public_value();

        assert_eq!(
            public.env.get("TOKEN").map(String::as_str),
            Some(MASKED_SECRET_VALUE)
        );
    }

    #[test]
    fn legacy_workspace_fields_are_ignored_when_loading_manifest() {
        let mut value = serde_json::to_value(executable_manifest()).expect("serialize manifest");
        value["workspace_id"] = json!("legacy-workspace");
        value["stdio"]["cwd_relative"] = json!("legacy-cwd");

        let record = serde_json::from_value::<LocalMcpManifestRecord>(value)
            .expect("legacy manifest should remain readable");

        assert_eq!(record.manifest_id, "manifest-1");
        assert_eq!(record.stdio.expect("stdio config").command, "node");
    }

    #[test]
    fn execution_requires_exact_owner_device_manifest_and_resource() {
        let manifest = executable_manifest();

        assert!(validate_manifest_for_execution(
            &manifest,
            "owner-1",
            "device-1",
            "manifest-1",
            "plugin-1",
        )
        .is_ok());
        assert!(validate_manifest_for_execution(
            &manifest,
            "owner-2",
            "device-1",
            "manifest-1",
            "plugin-1",
        )
        .is_err());
        assert!(validate_manifest_for_execution(
            &manifest,
            "owner-1",
            "device-1",
            "manifest-1",
            "plugin-2",
        )
        .is_err());
    }

    #[test]
    fn disabled_manifest_cannot_execute() {
        let mut manifest = executable_manifest();
        manifest.enabled = false;

        assert!(validate_manifest_for_execution(
            &manifest,
            "owner-1",
            "device-1",
            "manifest-1",
            "plugin-1",
        )
        .is_err());
    }
}
