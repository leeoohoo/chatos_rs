// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fmt;
use std::fs;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    plugin_agent_snapshot_sha256, plugin_component_descriptors, PluginAgent, PluginComponentKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::mcp_adapter::load_verified_manifest;
use super::portable_bundle::validate_local_portable_bundle;
use crate::plugins::PluginInstaller;

const MAX_AGENT_BYTES: u64 = 256 * 1024;
const MAX_AGENT_FRONTMATTER_BYTES: usize = 32 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAgentSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub agent_name: String,
    pub relative_source_path: String,
    #[serde(default)]
    pub description: Option<String>,
    pub base_agent: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub max_iterations: usize,
    pub content_sha256: String,
    pub snapshot_sha256: String,
    pub prompt: String,
}

impl fmt::Debug for PluginAgentSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginAgentSnapshot")
            .field("plugin_id", &self.plugin_id)
            .field("release_id", &self.release_id)
            .field("version", &self.version)
            .field("artifact_sha256", &self.artifact_sha256)
            .field("component_key", &self.component_key)
            .field("agent_name", &self.agent_name)
            .field("relative_source_path", &self.relative_source_path)
            .field("description", &self.description)
            .field("base_agent", &self.base_agent)
            .field("allowed_tools", &self.allowed_tools)
            .field("max_iterations", &self.max_iterations)
            .field("content_sha256", &self.content_sha256)
            .field("snapshot_sha256", &self.snapshot_sha256)
            .field("prompt_sha256", &sha256_bytes(self.prompt.as_bytes()))
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct PluginAgentLoader {
    installer: PluginInstaller,
}

impl PluginAgentLoader {
    pub fn new(installer: PluginInstaller) -> Self {
        Self { installer }
    }

    pub fn load(
        &self,
        plugin_id: &str,
        component_key: &str,
        expected_content_sha256: &str,
        permission_snapshot: &BTreeSet<String>,
    ) -> Result<PluginAgentSnapshot> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        let manifest = load_verified_manifest(&installation)?;
        let agent = manifest
            .agents
            .iter()
            .find(|agent| agent.component_key == component_key)
            .context("Plugin Agent is not present in the active Manifest")?;
        validate_agent_inventory(&installation, &manifest, agent)?;
        validate_required_permissions(&installation, component_key, permission_snapshot)?;

        let relative_source_path = agent.source.path.as_str();
        let portable_bundle = validate_local_portable_bundle(
            &installation,
            &manifest,
            component_key,
            expected_content_sha256,
        )?;
        let (raw, content_sha256) = if let Some(bundle) = portable_bundle {
            (bundle.primary_text, bundle.bundle_sha256)
        } else {
            let package_source_path = relative_source_path.trim_start_matches("./");
            let expected_package_sha256 = installation
                .version
                .package_file_sha256
                .get(package_source_path)
                .context("Plugin Agent source is not covered by package checksums")?;
            let source_path = installation.installation_path.join(relative_source_path);
            let metadata = fs::symlink_metadata(source_path.as_path())
                .context("read Plugin Agent source metadata")?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > MAX_AGENT_BYTES
            {
                bail!("Plugin Agent source is missing, unsafe, or exceeds its size limit");
            }
            let bytes = fs::read(source_path.as_path()).context("read Plugin Agent source")?;
            if bytes.len() as u64 > MAX_AGENT_BYTES {
                bail!("Plugin Agent source exceeds its size limit");
            }
            let content_sha256 = sha256_bytes(bytes.as_slice());
            if content_sha256 != *expected_package_sha256
                || content_sha256 != expected_content_sha256
            {
                bail!("Plugin Agent source does not match the immutable component snapshot");
            }
            (
                String::from_utf8(bytes).context("Plugin Agent source is not UTF-8")?,
                content_sha256,
            )
        };
        if raw.contains('\0') {
            bail!("Plugin Agent source contains NUL bytes");
        }
        let prompt = agent_prompt(raw.as_str())?;
        if prompt.is_empty() {
            bail!("Plugin Agent prompt is empty");
        }
        let snapshot_sha256 = plugin_agent_snapshot_sha256(
            plugin_id,
            installation.version.release_id.as_str(),
            component_key,
            manifest.execution.host_for(component_key),
            agent.source.path.as_str(),
            agent.description.as_deref(),
            agent.base_agent.as_str(),
            agent.allowed_tools.as_slice(),
            agent.max_iterations,
            content_sha256.as_str(),
            prompt.as_str(),
        )
        .context("hash Plugin Agent snapshot")?;
        Ok(PluginAgentSnapshot {
            plugin_id: plugin_id.to_string(),
            release_id: installation.version.release_id,
            version: installation.version.version,
            artifact_sha256: installation.version.artifact_sha256,
            component_key: component_key.to_string(),
            agent_name: component_key.to_string(),
            relative_source_path: relative_source_path.to_string(),
            description: agent.description.clone(),
            base_agent: agent.base_agent.clone(),
            allowed_tools: agent.allowed_tools.clone(),
            max_iterations: agent.max_iterations,
            content_sha256,
            snapshot_sha256,
            prompt,
        })
    }
}

fn validate_agent_inventory(
    installation: &crate::plugins::ActivePluginInstallation,
    manifest: &chatos_plugin_management_sdk::PluginManifest,
    agent: &PluginAgent,
) -> Result<()> {
    let descriptor = plugin_component_descriptors(manifest)
        .into_iter()
        .find(|component| component.component_key == agent.component_key)
        .context("Plugin Agent component descriptor is unavailable")?;
    if descriptor.kind != PluginComponentKind::Agent
        || descriptor.runtime_kind != "agent_profile"
        || descriptor.entrypoint.as_ref() != Some(&agent.source)
    {
        bail!("Plugin Agent descriptor does not match its signed Manifest");
    }
    let installed = installation
        .version
        .inventory
        .components
        .iter()
        .find(|component| component.component_key == agent.component_key)
        .context("Plugin Agent is missing from the signed installation inventory")?;
    if installed != &descriptor {
        bail!("Plugin Agent inventory does not match the active signed Manifest");
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
                "Plugin Agent required permission is missing from the prepared snapshot: {}",
                requirement.permission
            );
        }
    }
    Ok(())
}

fn agent_prompt(raw: &str) -> Result<String> {
    let normalized = raw.replace("\r\n", "\n");
    let body = if let Some(rest) = normalized.strip_prefix("---\n") {
        let Some((frontmatter, body)) = rest.split_once("\n---\n") else {
            bail!("Plugin Agent YAML frontmatter is not terminated");
        };
        if frontmatter.len() > MAX_AGENT_FRONTMATTER_BYTES {
            bail!("Plugin Agent YAML frontmatter exceeds its size limit");
        }
        body
    } else {
        normalized.as_str()
    };
    Ok(body.trim().to_string())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::agent_prompt;

    #[test]
    fn agent_prompt_strips_bounded_frontmatter() {
        assert_eq!(
            agent_prompt("---\nname: reviewer\n---\n\nReview carefully.\n").expect("agent prompt"),
            "Review carefully."
        );
        assert!(agent_prompt("---\nname: reviewer\n").is_err());
    }
}
