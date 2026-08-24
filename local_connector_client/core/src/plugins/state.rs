// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginManifest;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;

use super::verifier::PluginRequirementInventory;

const REGISTRY_SCHEMA_VERSION: u32 = 2;
const RETIRED_BUNDLED_REGISTRY_SCHEMA_VERSION: u64 = 1;
const RETIRED_BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";
const RETIRED_BUNDLED_PLUGIN_PREFIX: &str = "bundled-plugin-";
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
    #[serde(default)]
    pub granted_permissions: BTreeSet<String>,
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
    let payload = fs::read(&path)
        .with_context(|| format!("read local Plugin registry: {}", path.display()))?;
    if let Some(registry) = migrate_retired_bundled_registry(plugin_root, payload.as_slice())? {
        return Ok(registry);
    }
    let registry: LocalPluginRegistry =
        serde_json::from_slice(payload.as_slice()).context("parse local Plugin registry")?;
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

fn migrate_retired_bundled_registry(
    plugin_root: &Path,
    payload: &[u8],
) -> Result<Option<LocalPluginRegistry>> {
    let document: Value = serde_json::from_slice(payload).context("parse local Plugin registry")?;
    if document.get("schema_version").and_then(Value::as_u64)
        != Some(RETIRED_BUNDLED_REGISTRY_SCHEMA_VERSION)
    {
        return Ok(None);
    }
    let plugins = document
        .get("plugins")
        .and_then(Value::as_object)
        .context("legacy local Plugin registry is missing plugins")?;
    let only_retired_bundled_plugins = plugins.iter().all(|(plugin_id, plugin)| {
        plugin_id.starts_with(RETIRED_BUNDLED_PLUGIN_PREFIX)
            && plugin.get("plugin_id").and_then(Value::as_str) == Some(plugin_id.as_str())
            && plugin.get("marketplace_id").and_then(Value::as_str)
                == Some(RETIRED_BUNDLED_MARKETPLACE_ID)
    });
    if !only_retired_bundled_plugins {
        bail!(
            "legacy local Plugin registry contains non-bundled entries and cannot be migrated automatically"
        );
    }

    let mut retired_installation_paths = Vec::new();
    for plugin in plugins.values() {
        let Some(versions) = plugin.get("versions").and_then(Value::as_object) else {
            continue;
        };
        for version in versions.values() {
            let Some(relative_path) = version
                .get("relative_installation_path")
                .and_then(Value::as_str)
            else {
                continue;
            };
            if !is_safe_installation_path(relative_path) {
                bail!("legacy bundled Plugin registry contains an unsafe installation path");
            }
            retired_installation_paths.push(relative_path.to_string());
        }
    }

    let registry = LocalPluginRegistry::default();
    save_registry(plugin_root, &registry)?;
    for relative_path in retired_installation_paths {
        let installation_path = plugin_root.join(relative_path);
        if installation_path.exists() {
            fs::remove_dir_all(&installation_path).with_context(|| {
                format!(
                    "remove retired bundled Plugin installation: {}",
                    installation_path.display()
                )
            })?;
        }
        if let Some(parent) = installation_path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
    Ok(Some(registry))
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn migrates_registry_containing_only_retired_bundled_plugins() {
        let temp = tempfile::tempdir().expect("tempdir");
        let installation = temp.path().join("installed/computer-use--legacy/1.0.0");
        fs::create_dir_all(&installation).expect("legacy installation");
        fs::write(installation.join("payload"), b"legacy").expect("legacy payload");
        fs::write(
            registry_path(temp.path()),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "plugins": {
                    "bundled-plugin-computer-use": {
                        "plugin_id": "bundled-plugin-computer-use",
                        "marketplace_id": "chatos-bundled",
                        "versions": {
                            "1.0.0": {
                                "relative_installation_path": "installed/computer-use--legacy/1.0.0"
                            }
                        }
                    }
                }
            }))
            .expect("legacy registry"),
        )
        .expect("write legacy registry");

        let registry = load_registry(temp.path()).expect("migrated registry");
        assert_eq!(registry, LocalPluginRegistry::default());
        assert!(!installation.exists());
        assert_eq!(
            serde_json::from_slice::<LocalPluginRegistry>(
                fs::read(registry_path(temp.path()))
                    .expect("read migrated registry")
                    .as_slice(),
            )
            .expect("parse migrated registry"),
            LocalPluginRegistry::default(),
        );
    }

    #[test]
    fn refuses_to_discard_non_bundled_legacy_plugins() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            registry_path(temp.path()),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "plugins": {
                    "marketplace-plugin": {
                        "plugin_id": "marketplace-plugin",
                        "marketplace_id": "marketplace",
                        "versions": {}
                    }
                }
            }))
            .expect("legacy registry"),
        )
        .expect("write legacy registry");

        let error = load_registry(temp.path()).expect_err("mixed legacy registry must fail");
        assert!(error.to_string().contains("non-bundled entries"));
    }
}
