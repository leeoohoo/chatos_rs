// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginComponentKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::skill_document::{extract_references, parse_skill_document, resolve_reference_path};
use crate::plugins::{ActivePluginInstallation, PluginInstaller};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginSkillLoaderLimits {
    pub max_skill_documents: usize,
    pub max_instructions_bytes: u64,
    pub max_resource_bytes: u64,
    pub max_total_resource_bytes: u64,
    pub max_references: usize,
}

impl Default for PluginSkillLoaderLimits {
    fn default() -> Self {
        Self {
            max_skill_documents: 64,
            max_instructions_bytes: 256 * 1024,
            max_resource_bytes: 1024 * 1024,
            max_total_resource_bytes: 4 * 1024 * 1024,
            max_references: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSkillMetadata {
    pub name: String,
    pub description: Option<String>,
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSkillResourceKind {
    SkillInstructions,
    Reference,
    Script,
    Asset,
    Schema,
    Binary,
    License,
    OtherText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSkillResourceDescriptor {
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub kind: PluginSkillResourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSkillSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub skill_key: String,
    pub relative_skill_path: String,
    pub instructions_sha256: String,
    pub snapshot_sha256: String,
    pub metadata: PluginSkillMetadata,
    pub instructions: String,
    pub resources: Vec<PluginSkillResourceDescriptor>,
}

#[derive(Debug, Clone)]
pub struct PluginSkillLoader {
    installer: PluginInstaller,
    limits: PluginSkillLoaderLimits,
}

impl PluginSkillLoader {
    pub fn new(installer: PluginInstaller) -> Self {
        Self {
            installer,
            limits: PluginSkillLoaderLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: PluginSkillLoaderLimits) -> Self {
        self.limits = limits;
        self
    }

    pub(super) fn installer(&self) -> PluginInstaller {
        self.installer.clone()
    }

    pub(super) fn active_component_kind(
        &self,
        plugin_id: &str,
        component_key: &str,
    ) -> Result<PluginComponentKind> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        installation
            .version
            .inventory
            .components
            .iter()
            .find(|component| component.component_key == component_key)
            .map(|component| component.kind)
            .context("Plugin component is not present in the signed installation inventory")
    }

    pub fn load_component(
        &self,
        plugin_id: &str,
        component_key: &str,
    ) -> Result<Vec<PluginSkillSnapshot>> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        self.load_component_from_installation(&installation, component_key)
    }

    pub fn load_skill(
        &self,
        plugin_id: &str,
        component_key: &str,
        skill_key: &str,
    ) -> Result<PluginSkillSnapshot> {
        self.load_component(plugin_id, component_key)?
            .into_iter()
            .find(|skill| skill.skill_key == skill_key)
            .with_context(|| {
                format!("Plugin Skill is not present in the active component: {skill_key}")
            })
    }

    pub(super) fn validate_component_content_snapshot(
        &self,
        plugin_id: &str,
        component_key: &str,
        expected_content_sha256: Option<&str>,
    ) -> Result<()> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        installation
            .version
            .inventory
            .components
            .iter()
            .find(|component| component.component_key == component_key)
            .context("Plugin Skill component is missing from the signed installation inventory")?;
        if expected_content_sha256
            .is_some_and(|expected| expected != installation.version.artifact_sha256)
        {
            bail!("Plugin Skill component snapshot does not match the installed npm package");
        }
        Ok(())
    }

    pub fn load_text_resource(
        &self,
        snapshot: &PluginSkillSnapshot,
        relative_path: &str,
    ) -> Result<String> {
        let descriptor = snapshot
            .resources
            .iter()
            .find(|resource| resource.relative_path == relative_path)
            .context("Plugin Skill resource was not declared by the prepared snapshot")?;
        if matches!(
            descriptor.kind,
            PluginSkillResourceKind::Asset | PluginSkillResourceKind::Binary
        ) {
            bail!("binary Plugin Skill resources cannot be loaded as prompt text");
        }
        let installation = self
            .installer
            .active_installation(snapshot.plugin_id.as_str())?
            .context("Plugin is no longer installed and active")?;
        validate_active_snapshot(&installation, snapshot)?;
        let bytes = read_verified_file(
            &installation,
            descriptor.relative_path.as_str(),
            self.limits.max_resource_bytes,
        )?;
        if sha256_bytes(bytes.as_slice()) != descriptor.sha256 {
            bail!("Plugin Skill resource no longer matches the prepared snapshot");
        }
        String::from_utf8(bytes).context("Plugin Skill text resource is not UTF-8")
    }

    fn load_component_from_installation(
        &self,
        installation: &ActivePluginInstallation,
        component_key: &str,
    ) -> Result<Vec<PluginSkillSnapshot>> {
        let component = installation
            .version
            .inventory
            .components
            .iter()
            .find(|component| component.component_key == component_key)
            .context("Plugin component is not present in the signed installation inventory")?;
        if component.kind != PluginComponentKind::SkillCollection {
            bail!("Plugin component is not a Skill collection");
        }
        let entrypoint = component
            .entrypoint
            .as_ref()
            .context("Plugin Skill component has no signed entrypoint")?
            .path
            .trim_start_matches("./")
            .to_string();
        if entrypoint != "skills" && !entrypoint.starts_with("skills/") {
            bail!("Plugin Skill component entrypoint must be under skills/");
        }
        let skill_paths = discover_skill_paths(installation, entrypoint.as_str(), self.limits)?;
        let mut seen_names = HashSet::new();
        let mut snapshots = Vec::with_capacity(skill_paths.len());
        for skill_path in skill_paths {
            let snapshot = self.load_skill_snapshot(installation, component_key, skill_path)?;
            if !seen_names.insert(snapshot.skill_key.clone()) {
                bail!(
                    "Plugin Skill component contains duplicate Skill name: {}",
                    snapshot.skill_key
                );
            }
            snapshots.push(snapshot);
        }
        snapshots.sort_by(|left, right| left.relative_skill_path.cmp(&right.relative_skill_path));
        Ok(snapshots)
    }

    fn load_skill_snapshot(
        &self,
        installation: &ActivePluginInstallation,
        component_key: &str,
        skill_path: String,
    ) -> Result<PluginSkillSnapshot> {
        let bytes = read_verified_file(
            installation,
            skill_path.as_str(),
            self.limits.max_instructions_bytes,
        )?;
        let instructions = String::from_utf8(bytes)
            .with_context(|| format!("Plugin Skill instructions are not UTF-8: {skill_path}"))?;
        let fallback_name = Path::new(skill_path.as_str())
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .context("derive Plugin Skill fallback name")?;
        let parsed = parse_skill_document(instructions.as_str(), fallback_name)
            .with_context(|| format!("parse Plugin Skill instructions: {skill_path}"))?;
        let instructions_sha256 = sha256_bytes(instructions.as_bytes());
        let resources =
            self.load_reference_graph(installation, skill_path.as_str(), parsed.references)?;
        let snapshot_sha256 = snapshot_sha256(
            installation,
            component_key,
            skill_path.as_str(),
            instructions_sha256.as_str(),
            resources.as_slice(),
        );
        Ok(PluginSkillSnapshot {
            plugin_id: installation.plugin_id.clone(),
            release_id: installation.version.release_id.clone(),
            version: installation.version.version.clone(),
            artifact_sha256: installation.version.artifact_sha256.clone(),
            component_key: component_key.to_string(),
            skill_key: parsed.metadata.name.clone(),
            relative_skill_path: skill_path,
            instructions_sha256,
            snapshot_sha256,
            metadata: parsed.metadata,
            instructions,
            resources,
        })
    }

    fn load_reference_graph(
        &self,
        installation: &ActivePluginInstallation,
        skill_path: &str,
        initial_references: Vec<String>,
    ) -> Result<Vec<PluginSkillResourceDescriptor>> {
        let mut state = ReferenceGraphState::default();
        state.visiting.insert(skill_path.to_string());
        self.visit_references(installation, skill_path, initial_references, &mut state)?;
        state.visiting.remove(skill_path);
        Ok(state.resources.into_values().collect())
    }

    fn visit_references(
        &self,
        installation: &ActivePluginInstallation,
        current_file: &str,
        references: Vec<String>,
        state: &mut ReferenceGraphState,
    ) -> Result<()> {
        for reference in references {
            let Some(relative_path) = resolve_reference_path(current_file, reference.as_str())?
            else {
                continue;
            };
            if state.visiting.contains(relative_path.as_str()) {
                bail!("Plugin Skill reference graph contains a cycle at {relative_path}");
            }
            if state.visited.contains(relative_path.as_str()) {
                continue;
            }
            if state.resources.len() >= self.limits.max_references {
                bail!("Plugin Skill reference graph exceeds the reference count limit");
            }
            let bytes = read_verified_file(
                installation,
                relative_path.as_str(),
                self.limits.max_resource_bytes,
            )?;
            state.total_bytes = state
                .total_bytes
                .checked_add(bytes.len() as u64)
                .context("Plugin Skill reference byte count overflow")?;
            if state.total_bytes > self.limits.max_total_resource_bytes {
                bail!("Plugin Skill reference graph exceeds the total byte limit");
            }
            let kind = resource_kind(relative_path.as_str());
            state.resources.insert(
                relative_path.clone(),
                PluginSkillResourceDescriptor {
                    relative_path: relative_path.clone(),
                    sha256: sha256_bytes(bytes.as_slice()),
                    size_bytes: bytes.len() as u64,
                    kind,
                },
            );
            state.visiting.insert(relative_path.clone());
            if is_reference_document(relative_path.as_str()) {
                let text = String::from_utf8(bytes).with_context(|| {
                    format!("Plugin Skill reference document is not UTF-8: {relative_path}")
                })?;
                self.visit_references(
                    installation,
                    relative_path.as_str(),
                    extract_references(text.as_str()),
                    state,
                )?;
            }
            state.visiting.remove(relative_path.as_str());
            state.visited.insert(relative_path);
        }
        Ok(())
    }
}

#[derive(Default)]
struct ReferenceGraphState {
    resources: BTreeMap<String, PluginSkillResourceDescriptor>,
    visiting: BTreeSet<String>,
    visited: BTreeSet<String>,
    total_bytes: u64,
}

fn discover_skill_paths(
    installation: &ActivePluginInstallation,
    entrypoint: &str,
    limits: PluginSkillLoaderLimits,
) -> Result<Vec<String>> {
    let root_path = installation.installation_path.join(entrypoint);
    let metadata = fs::symlink_metadata(root_path.as_path())
        .with_context(|| format!("read Plugin Skill component entrypoint: {entrypoint}"))?;
    if metadata.file_type().is_symlink() {
        bail!("Plugin Skill component entrypoint cannot be a symlink");
    }
    let mut paths = if metadata.is_file() {
        if !entrypoint.ends_with("/SKILL.md") && entrypoint != "SKILL.md" {
            bail!("Plugin Skill component file entrypoint must be SKILL.md");
        }
        vec![entrypoint.to_string()]
    } else if metadata.is_dir() {
        let prefix = format!("{}/", entrypoint.trim_end_matches('/'));
        installation
            .version
            .package_file_sha256
            .keys()
            .filter(|path| path.starts_with(prefix.as_str()) && path.ends_with("/SKILL.md"))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        bail!("Plugin Skill component entrypoint is not a regular file or directory");
    };
    paths.sort();
    if paths.is_empty() {
        bail!("Plugin Skill component contains no SKILL.md files");
    }
    if paths.len() > limits.max_skill_documents {
        bail!("Plugin Skill component exceeds the Skill document limit");
    }
    Ok(paths)
}

fn validate_active_snapshot(
    installation: &ActivePluginInstallation,
    snapshot: &PluginSkillSnapshot,
) -> Result<()> {
    if installation.plugin_id != snapshot.plugin_id
        || installation.version.release_id != snapshot.release_id
        || installation.version.version != snapshot.version
        || installation.version.artifact_sha256 != snapshot.artifact_sha256
    {
        bail!("Plugin Skill snapshot does not match the active immutable Release");
    }
    let valid_component = installation
        .version
        .inventory
        .components
        .iter()
        .any(|component| {
            component.component_key == snapshot.component_key
                && component.kind == PluginComponentKind::SkillCollection
        });
    if !valid_component {
        bail!("Plugin Skill snapshot component is no longer active");
    }
    Ok(())
}

fn read_verified_file(
    installation: &ActivePluginInstallation,
    relative_path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let expected = installation
        .version
        .package_file_sha256
        .get(relative_path)
        .with_context(|| {
            format!("Plugin Skill resource is not covered by package checksums: {relative_path}")
        })?;
    let path = installation.installation_path.join(relative_path);
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("read Plugin Skill resource metadata: {relative_path}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        bail!(
            "Plugin Skill resource is missing, unsafe, or exceeds its size limit: {relative_path}"
        );
    }
    let bytes = fs::read(path.as_path())
        .with_context(|| format!("read Plugin Skill resource: {relative_path}"))?;
    if bytes.len() as u64 > max_bytes || sha256_bytes(bytes.as_slice()) != *expected {
        bail!("Plugin Skill resource checksum mismatch: {relative_path}");
    }
    Ok(bytes)
}

fn snapshot_sha256(
    installation: &ActivePluginInstallation,
    component_key: &str,
    skill_path: &str,
    instructions_sha256: &str,
    resources: &[PluginSkillResourceDescriptor],
) -> String {
    let mut payload = format!(
        "chatos.plugin.skill.snapshot.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        installation.plugin_id,
        installation.version.release_id,
        installation.version.version,
        installation.version.artifact_sha256,
        component_key,
        skill_path,
        instructions_sha256,
    );
    for resource in resources {
        payload.push('\n');
        payload.push_str(resource.relative_path.as_str());
        payload.push(':');
        payload.push_str(resource.sha256.as_str());
    }
    sha256_bytes(payload.as_bytes())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn resource_kind(path: &str) -> PluginSkillResourceKind {
    let root = path.split('/').next().unwrap_or_default();
    match root {
        "skills" if path.ends_with("/SKILL.md") => PluginSkillResourceKind::SkillInstructions,
        "skills" | "references" => PluginSkillResourceKind::Reference,
        "scripts" => PluginSkillResourceKind::Script,
        "assets" => PluginSkillResourceKind::Asset,
        "schemas" => PluginSkillResourceKind::Schema,
        "binaries" => PluginSkillResourceKind::Binary,
        "licenses" => PluginSkillResourceKind::License,
        _ => PluginSkillResourceKind::OtherText,
    }
}

fn is_reference_document(path: &str) -> bool {
    path.ends_with("/SKILL.md")
        || Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "md" | "mdx" | "txt"
                )
            })
}
