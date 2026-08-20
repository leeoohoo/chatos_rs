// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginManifest;
use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::verifier::PluginRequirementInventory;

const REGISTRY_SCHEMA_VERSION: u32 = 2;
const MAX_REGISTRY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledPluginVersion {
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub manifest_sha256: String,
    pub manifest: PluginManifest,
    pub signature_key_id: String,
    pub relative_installation_path: String,
    pub installed_at: String,
    pub package_file_sha256: BTreeMap<String, String>,
    pub inventory: PluginRequirementInventory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalInstalledPlugin {
    pub plugin_id: String,
    pub marketplace_id: String,
    pub plugin_name: String,
    pub active_version: Option<String>,
    pub previous_version: Option<String>,
    pub versions: BTreeMap<String, InstalledPluginVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalPluginRegistry {
    pub schema_version: u32,
    pub plugins: BTreeMap<String, LocalInstalledPlugin>,
}

impl Default for LocalPluginRegistry {
    fn default() -> Self {
        Self {
            schema_version: REGISTRY_SCHEMA_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

pub(super) fn registry_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join("state.json")
}

pub(super) fn load_registry(plugin_root: &Path) -> Result<LocalPluginRegistry> {
    let path = registry_path(plugin_root);
    if !path.exists() {
        return Ok(LocalPluginRegistry::default());
    }
    let metadata = fs::metadata(&path)
        .with_context(|| format!("read local Plugin registry metadata: {}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_REGISTRY_BYTES {
        bail!("local Plugin registry is invalid or exceeds the size limit");
    }
    let registry: LocalPluginRegistry = serde_json::from_slice(
        fs::read(&path)
            .with_context(|| format!("read local Plugin registry: {}", path.display()))?
            .as_slice(),
    )
    .context("parse local Plugin registry")?;
    if registry.schema_version != REGISTRY_SCHEMA_VERSION {
        bail!("unsupported local Plugin registry schema version");
    }
    for (plugin_id, plugin) in &registry.plugins {
        if plugin_id != &plugin.plugin_id {
            bail!("local Plugin registry key does not match Plugin identity");
        }
        if !is_safe_plugin_name(plugin.plugin_name.as_str()) {
            bail!("local Plugin registry contains an unsafe Plugin name");
        }
        for (version, installed) in &plugin.versions {
            if version != &installed.version
                || Version::parse(version).is_err()
                || !is_safe_installation_path(installed.relative_installation_path.as_str())
            {
                bail!("local Plugin registry contains an invalid installed version");
            }
        }
        if plugin
            .active_version
            .as_ref()
            .is_some_and(|version| !plugin.versions.contains_key(version))
            || plugin
                .previous_version
                .as_ref()
                .is_some_and(|version| !plugin.versions.contains_key(version))
        {
            bail!("local Plugin registry references an unknown installed version");
        }
    }
    Ok(registry)
}

fn is_safe_plugin_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_safe_installation_path(value: &str) -> bool {
    value.starts_with("installed/")
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}

pub(super) fn save_registry(plugin_root: &Path, registry: &LocalPluginRegistry) -> Result<()> {
    fs::create_dir_all(plugin_root).with_context(|| {
        format!(
            "create local Plugin storage directory: {}",
            plugin_root.display()
        )
    })?;
    let payload = serde_json::to_vec_pretty(registry).context("serialize local Plugin registry")?;
    if payload.len() as u64 > MAX_REGISTRY_BYTES {
        bail!("local Plugin registry exceeds the size limit");
    }
    let mut temporary =
        NamedTempFile::new_in(plugin_root).context("create temporary Plugin registry")?;
    temporary
        .write_all(payload.as_slice())
        .context("write temporary Plugin registry")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary Plugin registry")?;
    temporary
        .persist(registry_path(plugin_root))
        .map_err(|error| error.error)
        .context("atomically replace local Plugin registry")?;
    sync_directory(plugin_root)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("open Plugin storage directory: {}", path.display()))?
        .sync_all()
        .with_context(|| format!("sync Plugin storage directory: {}", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}
