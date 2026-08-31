// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fmt;
use std::fs;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    plugin_command_snapshot_sha256, plugin_component_descriptors, PluginCommand,
    PluginComponentKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::mcp_runtime::load_verified_manifest;
use crate::plugins::PluginInstaller;

const MAX_COMMAND_BYTES: u64 = 256 * 1024;
const MAX_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommandSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub command_name: String,
    pub relative_source_path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub argument_hint: Option<String>,
    pub requires_confirmation: bool,
    #[serde(default)]
    pub target_agent: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub confirmation_approved: bool,
    pub content_sha256: String,
    pub arguments_present: bool,
    pub arguments_sha256: String,
    #[serde(default, skip_serializing)]
    pub arguments: Option<String>,
    pub snapshot_sha256: String,
    pub prompt: String,
}

impl fmt::Debug for PluginCommandSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginCommandSnapshot")
            .field("plugin_id", &self.plugin_id)
            .field("release_id", &self.release_id)
            .field("version", &self.version)
            .field("artifact_sha256", &self.artifact_sha256)
            .field("component_key", &self.component_key)
            .field("command_name", &self.command_name)
            .field("relative_source_path", &self.relative_source_path)
            .field("description", &self.description)
            .field("argument_hint", &self.argument_hint)
            .field("requires_confirmation", &self.requires_confirmation)
            .field("target_agent", &self.target_agent)
            .field("allowed_tools", &self.allowed_tools)
            .field("confirmation_approved", &self.confirmation_approved)
            .field("content_sha256", &self.content_sha256)
            .field("arguments_present", &self.arguments_present)
            .field("arguments_sha256", &self.arguments_sha256)
            .field("snapshot_sha256", &self.snapshot_sha256)
            .field("prompt_sha256", &sha256_bytes(self.prompt.as_bytes()))
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct PluginCommandLoader {
    installer: PluginInstaller,
}

impl PluginCommandLoader {
    pub fn new(installer: PluginInstaller) -> Self {
        Self { installer }
    }

    pub fn load(
        &self,
        plugin_id: &str,
        component_key: &str,
        expected_content_sha256: &str,
        permission_snapshot: &BTreeSet<String>,
        arguments: Option<&str>,
    ) -> Result<PluginCommandSnapshot> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        let manifest = load_verified_manifest(&installation)?;
        let command = manifest
            .commands
            .iter()
            .find(|command| command.component_key == component_key)
            .context("Plugin Command is not present in the active Manifest")?;
        validate_command_inventory(&installation, &manifest, command)?;
        validate_required_permissions(&installation, component_key, permission_snapshot)?;

        let relative_source_path = command.source.path.as_str();
        let (raw, content_sha256) = {
            let package_source_path = relative_source_path.trim_start_matches("./");
            let expected_package_sha256 = installation
                .version
                .package_file_sha256
                .get(package_source_path)
                .context("Plugin Command source is not covered by package checksums")?;
            let source_path = installation.installation_path.join(relative_source_path);
            let metadata = fs::symlink_metadata(source_path.as_path())
                .context("read Plugin Command source metadata")?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > MAX_COMMAND_BYTES
            {
                bail!("Plugin Command source is missing, unsafe, or exceeds its size limit");
            }
            let bytes = fs::read(source_path.as_path()).context("read Plugin Command source")?;
            if bytes.len() as u64 > MAX_COMMAND_BYTES {
                bail!("Plugin Command source exceeds its size limit");
            }
            let content_sha256 = sha256_bytes(bytes.as_slice());
            if content_sha256 != *expected_package_sha256
                || content_sha256 != expected_content_sha256
            {
                bail!("Plugin Command source does not match the immutable component snapshot");
            }
            (
                String::from_utf8(bytes).context("Plugin Command source is not UTF-8")?,
                content_sha256,
            )
        };
        if raw.contains('\0') {
            bail!("Plugin Command source contains NUL bytes");
        }
        let prompt = command_prompt(raw.as_str())?;
        if prompt.is_empty() {
            bail!("Plugin Command prompt is empty");
        }
        let arguments = normalized_command_arguments(arguments)?;
        let arguments_sha256 = sha256_bytes(arguments.as_deref().unwrap_or_default().as_bytes());
        let snapshot_sha256 = plugin_command_snapshot_sha256(
            plugin_id,
            installation.version.release_id.as_str(),
            component_key,
            command.source.path.as_str(),
            command.description.as_deref(),
            command.argument_hint.as_deref(),
            command.requires_confirmation,
            command.target_agent.as_deref(),
            command.allowed_tools.as_slice(),
            content_sha256.as_str(),
            prompt.as_str(),
            arguments_sha256.as_str(),
        )
        .context("hash Plugin Command snapshot")?;
        Ok(PluginCommandSnapshot {
            plugin_id: plugin_id.to_string(),
            release_id: installation.version.release_id,
            version: installation.version.version,
            artifact_sha256: installation.version.artifact_sha256,
            component_key: component_key.to_string(),
            command_name: component_key.to_string(),
            relative_source_path: relative_source_path.to_string(),
            description: command.description.clone(),
            argument_hint: command.argument_hint.clone(),
            requires_confirmation: command.requires_confirmation,
            target_agent: command.target_agent.clone(),
            allowed_tools: command.allowed_tools.clone(),
            confirmation_approved: false,
            content_sha256,
            arguments_present: arguments.is_some(),
            arguments_sha256,
            arguments,
            snapshot_sha256,
            prompt,
        })
    }
}

fn validate_command_inventory(
    installation: &crate::plugins::ActivePluginInstallation,
    manifest: &chatos_plugin_management_sdk::PluginManifest,
    command: &PluginCommand,
) -> Result<()> {
    let descriptor = plugin_component_descriptors(manifest)
        .into_iter()
        .find(|component| component.component_key == command.component_key)
        .context("Plugin Command component descriptor is unavailable")?;
    if descriptor.kind != PluginComponentKind::Command
        || descriptor.runtime_kind != "command"
        || descriptor.entrypoint.as_ref() != Some(&command.source)
    {
        bail!("Plugin Command descriptor does not match its signed Manifest");
    }
    let installed = installation
        .version
        .inventory
        .components
        .iter()
        .find(|component| component.component_key == command.component_key)
        .context("Plugin Command is missing from the signed installation inventory")?;
    if installed != &descriptor {
        bail!("Plugin Command inventory does not match the active signed Manifest");
    }
    Ok(())
}

fn validate_required_permissions(
    installation: &crate::plugins::ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    for requirement in installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.required
                && (requirement.components.is_empty()
                    || requirement
                        .components
                        .iter()
                        .any(|key| key == component_key))
        })
    {
        if !permission_snapshot.contains(requirement.permission.as_str()) {
            bail!(
                "Plugin Command required permission is missing from the prepared snapshot: {}",
                requirement.permission
            );
        }
    }
    Ok(())
}

fn command_prompt(raw: &str) -> Result<String> {
    let normalized = raw.replace("\r\n", "\n");
    let body = if let Some(rest) = normalized.strip_prefix("---\n") {
        let Some((_frontmatter, body)) = rest.split_once("\n---\n") else {
            bail!("Plugin Command YAML frontmatter is not terminated");
        };
        body
    } else {
        normalized.as_str()
    };
    Ok(body.trim().to_string())
}

fn normalized_command_arguments(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_COMMAND_ARGUMENT_BYTES {
        bail!("Plugin Command arguments exceed their size limit");
    }
    if value.contains('\0') {
        bail!("Plugin Command arguments contain NUL bytes");
    }
    Ok(Some(value.to_string()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::command_prompt;

    #[test]
    fn command_prompt_strips_bounded_frontmatter() {
        assert_eq!(
            command_prompt("---\nname: review\n---\n\nReview the change.\n")
                .expect("command prompt"),
            "Review the change."
        );
        assert!(command_prompt("---\nname: review\n").is_err());
    }
}
