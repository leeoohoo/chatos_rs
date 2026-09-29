// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginInstallationSpec {
    pub installation_id: String,
    pub owner_user_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub component_id: String,
    pub component_revision: String,
    pub server_id: String,
    pub executable_path: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    /// Environment variable name -> native credential-store reference. Values
    /// are resolved transiently by the native client and never stored here.
    #[serde(default)]
    pub environment_secret_refs: BTreeMap<String, String>,
    pub tool_prefix: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl LocalPluginInstallationSpec {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("installation_id", self.installation_id.as_str()),
            ("owner_user_id", self.owner_user_id.as_str()),
            ("plugin_id", self.plugin_id.as_str()),
            ("release_id", self.release_id.as_str()),
            ("component_id", self.component_id.as_str()),
            ("component_revision", self.component_revision.as_str()),
            ("server_id", self.server_id.as_str()),
        ] {
            validate_identifier(field, value)?;
        }
        validate_text("release_digest", &self.release_digest, 512)?;
        validate_text("executable_path", &self.executable_path, 4_096)?;
        if self.args.len() > 256 {
            return Err("plugin args must contain at most 256 entries".to_string());
        }
        for argument in &self.args {
            if argument.len() > 8_192 || argument.contains('\0') {
                return Err(
                    "plugin args must not exceed 8192 characters or contain NUL".to_string()
                );
            }
        }
        if let Some(path) = self.working_directory.as_deref() {
            validate_text("working_directory", path, 4_096)?;
        }
        if self.environment_secret_refs.len() > 256 {
            return Err("environment_secret_refs must contain at most 256 entries".to_string());
        }
        for (variable, secret_ref) in &self.environment_secret_refs {
            if variable.trim().is_empty()
                || variable.len() > 256
                || variable.contains('=')
                || variable.contains('\0')
            {
                return Err("environment variable names are invalid".to_string());
            }
            validate_identifier("environment secret reference", secret_ref)?;
        }
        if let Some(prefix) = self.tool_prefix.as_deref() {
            validate_identifier("tool_prefix", prefix)?;
        }
        if let Some(tools) = self.allowed_tools.as_deref() {
            if tools.is_empty() || tools.len() > 256 {
                return Err("allowed_tools must contain 1..=256 entries".to_string());
            }
            let mut unique = HashSet::with_capacity(tools.len());
            for tool in tools {
                validate_identifier("allowed tool", tool)?;
                if !unique.insert(tool.as_str()) {
                    return Err(format!("allowed_tools contains duplicate: {tool}"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginInstallationRecord {
    #[serde(flatten)]
    pub spec: LocalPluginInstallationSpec,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutPluginInstallationCommand {
    pub installation: LocalPluginInstallationSpec,
    pub expected_version: Option<u64>,
}

impl PutPluginInstallationCommand {
    pub fn validate(&self) -> Result<(), String> {
        self.installation.validate()?;
        if self.expected_version == Some(0) {
            return Err("expected_version must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetPluginInstallationCommand {
    pub owner_user_id: String,
    pub installation_id: String,
}

impl GetPluginInstallationCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("installation_id", &self.installation_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListPluginInstallationsCommand {
    pub owner_user_id: String,
    pub before_updated_at_unix_ms: Option<i64>,
    pub before_installation_id: Option<String>,
    pub limit: u32,
}

impl ListPluginInstallationsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if self.limit == 0 || self.limit > 200 {
            return Err("limit must be between 1 and 200".to_string());
        }
        match (
            self.before_updated_at_unix_ms,
            self.before_installation_id.as_deref(),
        ) {
            (None, None) => Ok(()),
            (Some(timestamp), Some(installation_id)) if timestamp >= 0 => {
                validate_identifier("before_installation_id", installation_id)
            }
            _ => Err(
                "before_updated_at_unix_ms and before_installation_id must be supplied together"
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemovePluginInstallationCommand {
    pub owner_user_id: String,
    pub installation_id: String,
    pub expected_version: u64,
}

impl RemovePluginInstallationCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("installation_id", &self.installation_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginInstallationSummary {
    pub installation_id: String,
    pub owner_user_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_id: String,
    pub component_revision: String,
    pub server_id: String,
    pub enabled: bool,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginInstallationPage {
    pub installations: Vec<LocalPluginInstallationSummary>,
    pub next_before_updated_at_unix_ms: Option<i64>,
    pub next_before_installation_id: Option<String>,
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_persists_secret_references_not_secret_values() {
        let spec = LocalPluginInstallationSpec {
            installation_id: "install-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            release_digest: "sha256:abc".to_string(),
            component_id: "component-1".to_string(),
            component_revision: "revision-1".to_string(),
            server_id: "files".to_string(),
            executable_path: "/plugins/files/server".to_string(),
            args: Vec::new(),
            working_directory: None,
            environment_secret_refs: BTreeMap::from([(
                "API_TOKEN".to_string(),
                "keychain:plugin-1/api-token".to_string(),
            )]),
            tool_prefix: None,
            allowed_tools: Some(vec!["read_file".to_string()]),
            enabled: true,
        };
        spec.validate().expect("valid snapshot");
        let encoded = serde_json::to_string(&spec).expect("serialize");
        assert!(encoded.contains("keychain:plugin-1/api-token"));
        assert!(!encoded.contains("secret_value"));
    }

    #[test]
    fn installation_list_cursor_is_paired_and_bounded() {
        let valid = ListPluginInstallationsCommand {
            owner_user_id: "user-1".to_string(),
            before_updated_at_unix_ms: Some(1_000),
            before_installation_id: Some("install-1".to_string()),
            limit: 50,
        };
        assert!(valid.validate().is_ok());
        assert!(ListPluginInstallationsCommand {
            before_installation_id: None,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListPluginInstallationsCommand {
            owner_user_id: String::new(),
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListPluginInstallationsCommand {
            limit: 201,
            ..valid
        }
        .validate()
        .is_err());
    }
}
