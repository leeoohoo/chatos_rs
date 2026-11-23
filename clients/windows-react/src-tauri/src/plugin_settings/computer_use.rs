use super::{
    installation::{
        confined_plugin_root, installation_manifest_path, list_installation_records,
        load_installed_manifest,
    },
    plugin_catalog_error, PluginSettingsManager, PluginSettingsSnapshot,
};
use crate::{
    computer_use_worker::{diagnose, TrustedComputerUseLaunch},
    host_lifecycle::HostManager,
    plugin_installer::resolve_executable,
    plugin_integrity::requires_installation_integrity,
};
use chatos_plugin_management_sdk::{PluginManifest, PluginMcpServer};
use std::path::PathBuf;

use super::presentation::windows_compatible;

pub(super) fn is_computer_use_package(name: &str) -> bool {
    matches!(name, "open-computer-use" | "open-computer-use-windows")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComputerUseInstallation {
    Missing,
    Disabled,
    Ready(TrustedComputerUseLaunch),
}

pub(crate) async fn resolve_computer_use_installation(
    host: &HostManager,
    manager: &PluginSettingsManager,
    owner_user_id: &str,
) -> Result<ComputerUseInstallation, String> {
    let records = list_installation_records(host, owner_user_id).await?;
    let mut matches = Vec::new();
    for record in records {
        let manifest = match load_installed_manifest(&manager.plugin_root, &record) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };
        if !is_computer_use_package(&manifest.name)
            || !windows_compatible(&manifest.dependencies.supported_platforms)
        {
            continue;
        }
        if !requires_installation_integrity(&record.spec.component_revision) {
            return Err("Computer Use 安装缺少可信完整性记录。".into());
        }
        let server = manifest
            .mcp_servers
            .iter()
            .find(|server| server.component_key() == record.spec.server_id)
            .ok_or_else(|| "Computer Use 安装记录与清单不一致。".to_owned())?;
        let PluginMcpServer::Stdio {
            component_key,
            bin,
            args,
            ..
        } = server
        else {
            return Err("Computer Use 必须使用本机 stdio Worker。".into());
        };
        if record.spec.component_id != *component_key
            || record.spec.args != *args
            || !required_component_permission(&manifest, "process.spawn", component_key)
            || !required_component_permission(&manifest, "computer.control", component_key)
        {
            return Err("Computer Use 安装权限或启动参数无效。".into());
        }
        let manifest_path =
            installation_manifest_path(&confined_plugin_root(&manager.plugin_root)?, &record)?;
        let installation_root = manifest_path
            .parent()
            .ok_or_else(plugin_catalog_error)?
            .canonicalize()
            .map_err(|_| plugin_catalog_error())?;
        let executable = resolve_executable(&installation_root, bin)?;
        let recorded_executable = PathBuf::from(&record.spec.executable_path)
            .canonicalize()
            .map_err(|_| plugin_catalog_error())?;
        let working_directory = record
            .spec
            .working_directory
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| "Computer Use 安装目录缺失。".to_owned())?
            .canonicalize()
            .map_err(|_| "Computer Use 安装目录不可用。".to_owned())?;
        if executable != recorded_executable || working_directory != installation_root {
            return Err("Computer Use 启动边界与已验证安装不一致。".into());
        }
        matches.push((
            record.spec.enabled,
            TrustedComputerUseLaunch {
                executable,
                working_directory,
            },
        ));
    }
    match matches.as_slice() {
        [] => Ok(ComputerUseInstallation::Missing),
        [(false, _)] => Ok(ComputerUseInstallation::Disabled),
        [(true, launch)] => Ok(ComputerUseInstallation::Ready(launch.clone())),
        _ => Err("Computer Use 存在重复的本机 Worker 安装记录。".into()),
    }
}

pub(super) fn required_component_permission(
    manifest: &PluginManifest,
    permission_id: &str,
    component_key: &str,
) -> bool {
    manifest.permissions.iter().any(|permission| {
        permission.permission == permission_id
            && permission.required
            && (permission.components.is_empty()
                || permission
                    .components
                    .iter()
                    .any(|component| component == component_key))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PluginRuntimeHealthProjection {
    pub(super) status: &'static str,
    pub(super) checked_at: String,
    pub(super) error: Option<String>,
    pub(super) process_spawn_status: &'static str,
    pub(super) computer_control_status: &'static str,
}

pub(super) async fn computer_use_runtime_health(
    host: &HostManager,
    settings: &PluginSettingsManager,
    owner_user_id: &str,
) -> Option<PluginRuntimeHealthProjection> {
    let checked_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    match resolve_computer_use_installation(host, settings, owner_user_id).await {
        Ok(ComputerUseInstallation::Missing) => None,
        Ok(ComputerUseInstallation::Disabled) => Some(PluginRuntimeHealthProjection {
            status: "disabled",
            checked_at,
            error: None,
            process_spawn_status: "disabled",
            computer_control_status: "disabled",
        }),
        Ok(ComputerUseInstallation::Ready(launch)) => {
            match diagnose(&launch, &settings.resources).await {
                Ok(diagnostics) => Some(PluginRuntimeHealthProjection {
                    status: "ready",
                    checked_at,
                    error: None,
                    process_spawn_status: "granted",
                    computer_control_status: if diagnostics.all_granted {
                        "granted"
                    } else {
                        "needs_attention"
                    },
                }),
                Err(error) => Some(PluginRuntimeHealthProjection {
                    status: "unavailable",
                    checked_at,
                    error: Some(error),
                    process_spawn_status: "unavailable",
                    computer_control_status: "unavailable",
                }),
            }
        }
        Err(error) => Some(PluginRuntimeHealthProjection {
            status: "unavailable",
            checked_at,
            error: Some(error),
            process_spawn_status: "unavailable",
            computer_control_status: "unavailable",
        }),
    }
}

pub(super) fn apply_computer_use_runtime_health(
    snapshot: &mut PluginSettingsSnapshot,
    health: Option<PluginRuntimeHealthProjection>,
) {
    let Some(plugin) = snapshot.plugins.iter_mut().find(|plugin| {
        plugin.installed
            && plugin
                .package_name
                .as_deref()
                .is_some_and(is_computer_use_package)
    }) else {
        return;
    };
    let Some(health) = health else {
        return;
    };
    plugin.runtime_status = health.status;
    plugin.runtime_last_checked_at = Some(health.checked_at);
    plugin.runtime_error = health.error;
    for permission in &mut plugin.permissions {
        permission.status = match permission.id.as_str() {
            "process.spawn" => health.process_spawn_status,
            "computer.control" => health.computer_control_status,
            _ => permission.status,
        };
    }
}
