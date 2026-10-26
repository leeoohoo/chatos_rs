// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const LOCAL_PLUGIN_HOOK_MAX_BUNDLES: usize = 200;
pub const LOCAL_PLUGIN_HOOK_MAX_DEFINITIONS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginHookDefinition {
    pub hook_id: String,
    pub event: String,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_ms: u64,
    pub additional_context_limit: u64,
}

impl LocalPluginHookDefinition {
    fn validate(&self) -> Result<(), String> {
        validate_identifier("hook_id", &self.hook_id)?;
        const EVENTS: &[&str] = &[
            "SessionStart",
            "UserPromptSubmit",
            "SubagentStart",
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "Stop",
            "SubagentStop",
            "Interrupt",
            "SessionEnd",
            "PostCompact",
        ];
        if !EVENTS.contains(&self.event.as_str()) {
            return Err(format!("unsupported plugin hook event: {}", self.event));
        }
        if let Some(matcher) = self.matcher.as_deref() {
            validate_text("hook matcher", matcher, 1_024)?;
        }
        validate_text("hook command", &self.command, 8 * 1_024)?;
        if !(100..=30_000).contains(&self.timeout_ms) {
            return Err("hook timeout_ms must be between 100 and 30000".to_string());
        }
        if self.additional_context_limit > 256 * 1_024 {
            return Err("hook additional_context_limit exceeds 262144 bytes".to_string());
        }
        Ok(())
    }
}

/// Immutable, user-approved Hook material. Every digest is bound into the
/// native trust decision before this value reaches the Host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginHookBundleSpec {
    pub bundle_id: String,
    pub owner_user_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub artifact_sha256: String,
    pub content_sha256: String,
    pub normalized_hook_sha256: String,
    pub command_sha256: String,
    pub permission_snapshot_sha256: String,
    pub runner_policy_version: u32,
    pub installation_root: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub hooks: Vec<LocalPluginHookDefinition>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trusted_at_unix_ms: i64,
}

impl LocalPluginHookBundleSpec {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("bundle_id", self.bundle_id.as_str()),
            ("owner_user_id", self.owner_user_id.as_str()),
            ("plugin_id", self.plugin_id.as_str()),
            ("release_id", self.release_id.as_str()),
            ("component_key", self.component_key.as_str()),
        ] {
            validate_identifier(name, value)?;
        }
        for (name, digest) in [
            ("artifact_sha256", self.artifact_sha256.as_str()),
            ("content_sha256", self.content_sha256.as_str()),
            (
                "normalized_hook_sha256",
                self.normalized_hook_sha256.as_str(),
            ),
            ("command_sha256", self.command_sha256.as_str()),
            (
                "permission_snapshot_sha256",
                self.permission_snapshot_sha256.as_str(),
            ),
        ] {
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(format!("{name} must be a lowercase SHA-256 digest"));
            }
        }
        if self.runner_policy_version == 0 {
            return Err("runner_policy_version must be greater than zero".to_string());
        }
        validate_text("installation_root", &self.installation_root, 4_096)?;
        if !self.installation_root.starts_with('/') {
            return Err("installation_root must be an absolute path".to_string());
        }
        if self.permissions.len() > 256 {
            return Err("permissions must contain at most 256 entries".to_string());
        }
        let mut permissions = HashSet::with_capacity(self.permissions.len());
        for permission in &self.permissions {
            validate_identifier("permission", permission)?;
            if !permissions.insert(permission.as_str()) {
                return Err(format!("permissions contains duplicate: {permission}"));
            }
        }
        if self.hooks.is_empty() || self.hooks.len() > LOCAL_PLUGIN_HOOK_MAX_DEFINITIONS {
            return Err(format!(
                "hooks must contain 1..={LOCAL_PLUGIN_HOOK_MAX_DEFINITIONS} entries"
            ));
        }
        let mut hook_ids = HashSet::with_capacity(self.hooks.len());
        for hook in &self.hooks {
            hook.validate()?;
            if !hook_ids.insert(hook.hook_id.as_str()) {
                return Err(format!(
                    "hooks contains duplicate hook_id: {}",
                    hook.hook_id
                ));
            }
        }
        if self.trusted_at_unix_ms < 0 {
            return Err("trusted_at_unix_ms must not be negative".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalPluginHookBundleRecord {
    #[serde(flatten)]
    pub spec: LocalPluginHookBundleSpec,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutPluginHookBundleCommand {
    pub bundle: LocalPluginHookBundleSpec,
}

impl PutPluginHookBundleCommand {
    pub fn validate(&self) -> Result<(), String> {
        self.bundle.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListPluginHookBundlesCommand {
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetActivePluginHookBundlesCommand {
    pub owner_user_id: String,
    #[serde(default)]
    pub bundle_ids: Vec<String>,
}

impl SetActivePluginHookBundlesCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if self.bundle_ids.len() > LOCAL_PLUGIN_HOOK_MAX_BUNDLES {
            return Err(format!(
                "bundle_ids must contain at most {LOCAL_PLUGIN_HOOK_MAX_BUNDLES} entries"
            ));
        }
        let mut unique = HashSet::with_capacity(self.bundle_ids.len());
        for bundle_id in &self.bundle_ids {
            validate_identifier("bundle_id", bundle_id)?;
            if !unique.insert(bundle_id.as_str()) {
                return Err(format!("bundle_ids contains duplicate: {bundle_id}"));
            }
        }
        Ok(())
    }
}

impl ListPluginHookBundlesCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)
    }
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_rejects_non_sha_digest_and_unsupported_event() {
        let mut bundle = fixture();
        bundle.artifact_sha256 = "ABC".to_string();
        assert!(bundle.validate().is_err());
        let mut bundle = fixture();
        bundle.hooks[0].event = "PreCompact".to_string();
        assert!(bundle.validate().is_err());
    }

    fn fixture() -> LocalPluginHookBundleSpec {
        LocalPluginHookBundleSpec {
            bundle_id: "bundle-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            component_key: "hooks-1".to_string(),
            artifact_sha256: "a".repeat(64),
            content_sha256: "b".repeat(64),
            normalized_hook_sha256: "c".repeat(64),
            command_sha256: "d".repeat(64),
            permission_snapshot_sha256: "e".repeat(64),
            runner_policy_version: 1,
            installation_root: "/plugins/plugin-1".to_string(),
            permissions: vec!["process.spawn".to_string()],
            hooks: vec![LocalPluginHookDefinition {
                hook_id: "hook-1".to_string(),
                event: "SessionStart".to_string(),
                matcher: None,
                command: "node hook.js".to_string(),
                timeout_ms: 5_000,
                additional_context_limit: 65_536,
            }],
            enabled: true,
            trusted_at_unix_ms: 1,
        }
    }
}
