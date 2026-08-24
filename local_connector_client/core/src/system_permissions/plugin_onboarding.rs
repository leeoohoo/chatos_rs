// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_plugin_management_sdk::{normalize_plugin_relative_path, PluginMcpServer};
use serde::Deserialize;
use tokio::process::Command;

use super::{
    PluginSystemPermissionSubject, PERMISSION_ACCESSIBILITY_CONTROL, PERMISSION_SCREEN_RECORDING,
};
use crate::plugins::{ActivePluginInstallation, InstalledPluginVersion, PluginInstaller};

pub(crate) async fn request_plugin_system_permission(
    plugin_installer: &PluginInstaller,
    permission_id: &str,
) -> Result<usize> {
    let Some(permission) = plugin_capability_for_system_permission(permission_id) else {
        return Ok(0);
    };
    if std::env::consts::OS != "macos" {
        return Ok(0);
    }
    let registry = plugin_installer.registry()?;
    let mut launched = 0_usize;
    let mut errors = Vec::new();
    for plugin in registry.plugins.values() {
        let Some(active_version) = plugin.active_version.as_deref() else {
            continue;
        };
        let Some(version) = plugin.versions.get(active_version) else {
            continue;
        };
        if !version.granted_permissions.contains(permission) {
            continue;
        }
        let installation = match plugin_installer.active_installation(plugin.plugin_id.as_str()) {
            Ok(Some(installation)) => installation,
            Ok(None) => continue,
            Err(error) => {
                errors.push(format!(
                    "{}: {error:#}",
                    version.manifest.interface.display_name
                ));
                continue;
            }
        };
        match launch_permission_doctor(&installation, permission).await {
            Ok(count) => launched += count,
            Err(error) => errors.push(format!(
                "{}: {error:#}",
                installation.version.manifest.interface.display_name
            )),
        }
    }
    if !errors.is_empty() {
        return Err(anyhow!(errors.join("; ")));
    }
    if launched == 0 {
        return Err(anyhow!(
            "no installed Plugin with an available permission doctor declares {permission}"
        ));
    }
    Ok(launched)
}

pub(super) fn installed_plugin_permission_subjects(
    plugin_installer: &PluginInstaller,
    permission: &str,
) -> Result<Vec<PluginSystemPermissionSubject>> {
    let registry = plugin_installer.registry()?;
    let mut subjects = Vec::new();
    for plugin in registry.plugins.values() {
        let Some(active_version) = plugin.active_version.as_deref() else {
            continue;
        };
        let Some(version) = plugin.versions.get(active_version) else {
            continue;
        };
        let component_keys = permission_component_keys(version, permission);
        if component_keys.is_empty()
            && !version
                .manifest
                .permissions
                .iter()
                .any(|requirement| requirement.permission == permission)
        {
            continue;
        }
        let onboarding_available = version.manifest.mcp_servers.iter().any(|server| {
            matches!(
                server,
                PluginMcpServer::Stdio { component_key, .. }
                    if (component_keys.is_empty() || component_keys.contains(component_key))
                        && component_has_permission(version, component_key, "process.spawn")
            )
        });
        subjects.push(PluginSystemPermissionSubject {
            plugin_id: plugin.plugin_id.clone(),
            display_name: version.manifest.interface.display_name.clone(),
            version: version.version.clone(),
            component_keys: component_keys.into_iter().collect(),
            runtime_granted: version.granted_permissions.contains(permission),
            onboarding_available,
        });
    }
    subjects.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.plugin_id.cmp(&right.plugin_id))
    });
    Ok(subjects)
}

fn plugin_capability_for_system_permission(permission_id: &str) -> Option<&'static str> {
    match permission_id {
        PERMISSION_ACCESSIBILITY_CONTROL => Some("computer.accessibility"),
        PERMISSION_SCREEN_RECORDING => Some("computer.screen-recording"),
        _ => None,
    }
}

async fn launch_permission_doctor(
    installation: &ActivePluginInstallation,
    permission: &str,
) -> Result<usize> {
    let component_keys = permission_component_keys(&installation.version, permission);
    let mut launched_bins = BTreeSet::new();
    for server in &installation.version.manifest.mcp_servers {
        let PluginMcpServer::Stdio {
            component_key, bin, ..
        } = server
        else {
            continue;
        };
        if !component_keys.is_empty() && !component_keys.contains(component_key) {
            continue;
        }
        if !component_has_permission(&installation.version, component_key, "process.spawn") {
            continue;
        }
        if !launched_bins.insert(bin.clone()) {
            continue;
        }
        run_permission_doctor(installation, bin).await?;
    }
    Ok(launched_bins.len())
}

fn permission_component_keys(
    version: &InstalledPluginVersion,
    permission: &str,
) -> BTreeSet<String> {
    version
        .manifest
        .permissions
        .iter()
        .filter(|requirement| requirement.permission == permission)
        .flat_map(|requirement| requirement.components.iter().cloned())
        .collect()
}

fn component_has_permission(
    version: &InstalledPluginVersion,
    component_key: &str,
    permission: &str,
) -> bool {
    version.granted_permissions.contains(permission)
        && version.manifest.permissions.iter().any(|requirement| {
            requirement.permission == permission
                && (requirement.components.is_empty()
                    || requirement
                        .components
                        .iter()
                        .any(|key| key == component_key))
        })
}

async fn run_permission_doctor(installation: &ActivePluginInstallation, bin: &str) -> Result<()> {
    let resolved = resolve_installed_npm_bin(installation, bin)?;
    let mut args = resolved.prefix_args;
    args.push("doctor".to_string());
    let mut command = Command::new(resolved.command);
    command
        .args(args)
        .current_dir(installation.installation_path.as_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(30), command.status())
        .await
        .map_err(|_| anyhow!("Plugin permission doctor timed out"))?
        .context("start Plugin permission doctor")?;
    if !status.success() {
        return Err(anyhow!(
            "Plugin permission doctor exited with status {status}"
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct InstalledNpmPackage {
    name: String,
    bin: InstalledNpmBin,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum InstalledNpmBin {
    One(String),
    Many(BTreeMap<String, String>),
}

struct ResolvedNpmBin {
    command: String,
    prefix_args: Vec<String>,
}

fn resolve_installed_npm_bin(
    installation: &ActivePluginInstallation,
    bin: &str,
) -> Result<ResolvedNpmBin> {
    let package_json_path = installation.installation_path.join("package.json");
    let package_json =
        fs::read(package_json_path.as_path()).context("read installed package.json")?;
    let package: InstalledNpmPackage =
        serde_json::from_slice(package_json.as_slice()).context("parse installed package.json")?;
    let bins = match package.bin {
        InstalledNpmBin::One(path) => BTreeMap::from([(
            package
                .name
                .rsplit('/')
                .next()
                .unwrap_or(package.name.as_str())
                .to_string(),
            path,
        )]),
        InstalledNpmBin::Many(values) => values,
    };
    let declared_path = bins
        .get(bin)
        .with_context(|| format!("installed npm package does not publish bin: {bin}"))?;
    let relative = normalize_plugin_relative_path(declared_path)
        .map_err(|message| anyhow!("invalid npm permission doctor bin path: {message}"))?;
    let relative = relative.trim_start_matches("./");
    if !installation
        .version
        .package_file_sha256
        .contains_key(relative)
    {
        return Err(anyhow!(
            "npm permission doctor bin is not covered by package checksums"
        ));
    }
    let path = installation.installation_path.join(relative);
    ensure_safe_executable(path.as_path())?;
    let prefix = fs::read(path.as_path())?
        .into_iter()
        .take(256)
        .collect::<Vec<_>>();
    let node_launcher = matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("js" | "cjs" | "mjs")
    ) || String::from_utf8_lossy(prefix.as_slice())
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("#!") && line.contains("node"));
    if node_launcher {
        return Ok(ResolvedNpmBin {
            command: "node".to_string(),
            prefix_args: vec![path.to_string_lossy().into_owned()],
        });
    }
    Ok(ResolvedNpmBin {
        command: path.to_string_lossy().into_owned(),
        prefix_args: Vec::new(),
    })
}

fn ensure_safe_executable(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("read npm permission doctor bin")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "npm permission doctor bin is not a safe regular file"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(anyhow!(
                "npm permission doctor native bin is not executable"
            ));
        }
    }
    Ok(())
}
