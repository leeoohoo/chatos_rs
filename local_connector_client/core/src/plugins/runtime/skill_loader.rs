// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginComponentKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::mcp_runtime::load_verified_manifest;
use super::portable_bundle::validate_local_portable_bundle;
use super::skill_document::{extract_references, parse_skill_document, resolve_reference_path};
use crate::plugins::{ActivePluginInstallation, PluginInstaller};
use crate::skills::{
    internal_skill_bundle_hash, internal_skill_catalog, internal_skill_instructions,
    internal_skill_manifest,
};

const BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginNativeSkillBindingSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub plugin_version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub skill_key: String,
    pub skill_id: String,
    pub bundle_id: String,
    pub bundle_version: String,
    pub bundle_hash: String,
    pub requires_workspace: bool,
    pub permissions: Vec<String>,
    pub skill_snapshot_sha256: String,
    pub snapshot_sha256: String,
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
        let component = installation
            .version
            .inventory
            .components
            .iter()
            .find(|component| component.component_key == component_key)
            .context("Plugin Skill component is missing from the signed installation inventory")?;
        match component.execution_host {
            chatos_plugin_management_sdk::PluginExecutionHost::Local => return Ok(()),
            chatos_plugin_management_sdk::PluginExecutionHost::Portable => {}
        }
        let expected = expected_content_sha256
            .context("portable Plugin Skill is missing its immutable Bundle SHA-256")?;
        let manifest = load_verified_manifest(&installation)?;
        validate_local_portable_bundle(&installation, &manifest, component_key, expected)?;
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

    pub fn load_bundled_native_binding(
        &self,
        plugin_id: &str,
        component_key: &str,
        runtime_kind: Option<&str>,
        metadata: Option<&Value>,
        content_sha256: Option<&str>,
        selected_skills: &BTreeMap<String, PluginSkillSnapshot>,
    ) -> Result<Option<PluginNativeSkillBindingSnapshot>> {
        if selected_skills.len() != 1 {
            if runtime_kind == Some("native_adapter") {
                bail!("bundled native Plugin components must select exactly one Skill");
            }
            return Ok(None);
        }
        let skill = selected_skills
            .values()
            .next()
            .context("selected Plugin Skill is unavailable")?;
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        validate_active_snapshot(&installation, skill)?;
        let skill_root = Path::new(skill.relative_skill_path.as_str())
            .parent()
            .context("Plugin Skill document has no parent directory")?;
        let manifest_path = skill_root.join("skill.json");
        let manifest_path = manifest_path
            .to_str()
            .context("Plugin Skill manifest path is not UTF-8")?;
        let has_bundle_manifest = installation
            .version
            .package_file_sha256
            .contains_key(manifest_path);
        if !has_bundle_manifest {
            if runtime_kind == Some("native_adapter") {
                bail!("native Plugin Skill component is missing its signed skill.json");
            }
            return Ok(None);
        }
        let manifest_bytes =
            read_verified_file(&installation, manifest_path, self.limits.max_resource_bytes)?;
        let manifest: Value = serde_json::from_slice(manifest_bytes.as_slice())
            .context("decode signed Plugin Skill bundle manifest")?;
        let entrypoint_kind = manifest
            .pointer("/entrypoint/kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if entrypoint_kind != "native_adapter" {
            if runtime_kind == Some("native_adapter") {
                bail!("Plugin component runtime_kind does not match signed skill.json");
            }
            return Ok(None);
        }
        if runtime_kind != Some("native_adapter") {
            bail!("signed native Plugin Skill is missing its native_adapter runtime snapshot");
        }

        let registry = self.installer.registry()?;
        let installed_plugin = registry
            .plugins
            .get(plugin_id)
            .context("Plugin is missing from the local registry")?;
        if installed_plugin.marketplace_id != BUNDLED_MARKETPLACE_ID {
            bail!(
                "native internal Skill adapters are restricted to the chatos-bundled Marketplace"
            );
        }

        let skill_id = required_metadata_text(metadata, "skill_id")?;
        let bundle_id = required_metadata_text(metadata, "bundle_id")?;
        let bundle_hash = required_metadata_text(metadata, "bundle_hash")?;
        let content_sha256 = content_sha256
            .context("native Plugin Skill component is missing its immutable content SHA-256")?;
        if content_sha256 != bundle_hash {
            bail!("native Plugin Skill content hash does not match bundle_hash metadata");
        }
        let catalog = internal_skill_catalog()?;
        let item = catalog
            .skills
            .iter()
            .find(|item| item.skill_id == skill_id)
            .context("native Plugin Skill is not present in the embedded inventory")?;
        if item.implementation_status != "ready" || item.entrypoint_kind != "native_adapter" {
            bail!("native Plugin Skill adapter is not ready in this Local Connector build");
        }
        if item.bundle_id != bundle_id
            || item.version != manifest_text(&manifest, "version")?
            || item.name != manifest_text(&manifest, "name")?
            || item.name != component_key
            || item.name != skill.skill_key
            || internal_skill_bundle_hash(item) != bundle_hash
        {
            bail!("native Plugin Skill metadata does not match the embedded inventory");
        }
        for (field, expected) in [
            ("skill_id", item.skill_id.as_str()),
            ("bundle_id", item.bundle_id.as_str()),
            ("version", item.version.as_str()),
            ("name", item.name.as_str()),
        ] {
            if manifest.get(field).and_then(Value::as_str) != Some(expected) {
                bail!("signed native Plugin Skill manifest has mismatched {field}");
            }
        }
        let embedded_manifest = internal_skill_manifest(item.skill_id.as_str())
            .context("embedded native Plugin Skill manifest is missing")?;
        let embedded_manifest: Value = serde_json::from_str(embedded_manifest)
            .context("decode embedded native Plugin Skill manifest")?;
        if manifest != embedded_manifest {
            bail!("signed native Plugin Skill manifest differs from the embedded adapter bundle");
        }
        let instructions_path = skill_root.join("instructions.md");
        let instructions_path = instructions_path
            .to_str()
            .context("Plugin Skill instructions path is not UTF-8")?;
        let packaged_instructions = read_verified_file(
            &installation,
            instructions_path,
            self.limits.max_instructions_bytes,
        )?;
        let embedded_instructions = internal_skill_instructions(item.skill_id.as_str())
            .context("embedded native Plugin Skill instructions are missing")?;
        if packaged_instructions.as_slice() != embedded_instructions.as_bytes() {
            bail!(
                "signed native Plugin Skill instructions differ from the embedded adapter bundle"
            );
        }

        let snapshot_sha256 = native_binding_sha256(
            &installation,
            skill,
            item.skill_id.as_str(),
            item.bundle_id.as_str(),
            item.version.as_str(),
            bundle_hash,
        );
        Ok(Some(PluginNativeSkillBindingSnapshot {
            plugin_id: installation.plugin_id,
            release_id: installation.version.release_id,
            plugin_version: installation.version.version,
            artifact_sha256: installation.version.artifact_sha256,
            component_key: component_key.to_string(),
            skill_key: skill.skill_key.clone(),
            skill_id: item.skill_id.clone(),
            bundle_id: item.bundle_id.clone(),
            bundle_version: item.version.clone(),
            bundle_hash: bundle_hash.to_string(),
            requires_workspace: item.requires_workspace,
            permissions: item.permissions.clone(),
            skill_snapshot_sha256: skill.snapshot_sha256.clone(),
            snapshot_sha256,
        }))
    }

    pub fn validate_bundled_native_binding(
        &self,
        snapshot: &PluginNativeSkillBindingSnapshot,
    ) -> Result<()> {
        let skills = self
            .load_component(snapshot.plugin_id.as_str(), snapshot.component_key.as_str())?
            .into_iter()
            .filter(|skill| skill.skill_key == snapshot.skill_key)
            .map(|skill| (skill.skill_key.clone(), skill))
            .collect::<BTreeMap<_, _>>();
        let metadata = serde_json::json!({
            "skill_id": snapshot.skill_id,
            "bundle_id": snapshot.bundle_id,
            "bundle_hash": snapshot.bundle_hash,
        });
        let active = self
            .load_bundled_native_binding(
                snapshot.plugin_id.as_str(),
                snapshot.component_key.as_str(),
                Some("native_adapter"),
                Some(&metadata),
                Some(snapshot.bundle_hash.as_str()),
                &skills,
            )?
            .context("native Plugin Skill binding is no longer available")?;
        if active != *snapshot {
            bail!("native Plugin Skill binding no longer matches the prepared snapshot");
        }
        Ok(())
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

fn native_binding_sha256(
    installation: &ActivePluginInstallation,
    skill: &PluginSkillSnapshot,
    skill_id: &str,
    bundle_id: &str,
    bundle_version: &str,
    bundle_hash: &str,
) -> String {
    let payload = format!(
        "chatos.plugin.native-skill.snapshot.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        installation.plugin_id,
        installation.version.release_id,
        installation.version.version,
        installation.version.artifact_sha256,
        skill.component_key,
        skill.snapshot_sha256,
        skill_id,
        bundle_id,
        bundle_version,
        bundle_hash,
    );
    sha256_bytes(payload.as_bytes())
}

fn required_metadata_text<'a>(metadata: Option<&'a Value>, field: &str) -> Result<&'a str> {
    metadata
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get(field))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("native Plugin Skill metadata is missing {field}"))
}

fn manifest_text<'a>(manifest: &'a Value, field: &str) -> Result<&'a str> {
    manifest
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("signed native Plugin Skill manifest is missing {field}"))
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
