use super::{
    installation::{group_records, load_installed_manifest},
    presented_credentials, InstalledProjection, PluginSettingsSnapshot, PresentedPlugin,
    PresentedPluginPermission, MAXIMUM_PRESENTED_TEXT_BYTES,
};
use crate::plugin_application::PluginApplicationManager;
use chatos_local_agent_protocol::LocalPluginInstallationRecord;
use chatos_plugin_management_sdk::{
    validate_plugin_manifest, PluginInstallSource, PluginInstallSourceList, PluginManifest,
    PluginMcpServer, PluginPermissionRequirement,
};
use std::{collections::BTreeMap, path::Path};

pub(super) fn project_snapshot(
    plugin_root: &Path,
    sources: Result<PluginInstallSourceList, String>,
    records: Result<Vec<LocalPluginInstallationRecord>, String>,
    owner_user_id: &str,
    applications: &PluginApplicationManager,
) -> Result<PluginSettingsSnapshot, String> {
    let marketplace_available = sources.is_ok();
    let local_catalog_available = records.is_ok();
    let marketplace_error = sources.as_ref().err().cloned();
    let local_catalog_error = records.as_ref().err().cloned();
    let sources = sources.unwrap_or_default();
    let records = records.unwrap_or_default();
    let grouped = group_records(records, owner_user_id)?;
    let mut plugins = BTreeMap::<String, PresentedPlugin>::new();
    for source in &sources.items {
        validate_plugin_manifest(&source.release.normalized_manifest)
            .map_err(|_| "Marketplace 返回了无效的 Plugin 清单。".to_owned())?;
        plugins.insert(
            source.catalog.id.clone(),
            project_marketplace_plugin(
                source,
                grouped.get(&source.catalog.id),
                plugin_root,
                owner_user_id,
                applications,
            )?,
        );
    }
    for (plugin_id, records) in &grouped {
        if !plugins.contains_key(plugin_id) {
            plugins.insert(
                plugin_id.clone(),
                project_local_plugin(plugin_id, records, plugin_root, owner_user_id, applications)?,
            );
        }
    }
    let mut plugins = plugins.into_values().collect::<Vec<_>>();
    plugins.sort_by(|left, right| {
        right
            .installed
            .cmp(&left.installed)
            .then_with(|| {
                left.display_name
                    .to_lowercase()
                    .cmp(&right.display_name.to_lowercase())
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(PluginSettingsSnapshot {
        plugins,
        marketplace_available,
        local_catalog_available,
        marketplace_error,
        local_catalog_error,
        secure_marketplace_install_available: true,
        manual_install_available: true,
    })
}

fn project_marketplace_plugin(
    source: &PluginInstallSource,
    installed: Option<&Vec<LocalPluginInstallationRecord>>,
    plugin_root: &Path,
    owner_user_id: &str,
    applications: &PluginApplicationManager,
) -> Result<PresentedPlugin, String> {
    let manifest = &source.release.normalized_manifest;
    let installed_projection = installed.map(|records| installed_projection(plugin_root, records));
    let installed_version = installed_projection.as_ref().and_then(|projection| {
        projection
            .manifest
            .as_ref()
            .map(|manifest| manifest.version.clone())
    });
    let enabled = installed_projection
        .as_ref()
        .is_some_and(|projection| projection.enabled);
    let installed = installed.is_some();
    let compatible = windows_compatible(&source.release.supported_platforms)
        && windows_compatible(&manifest.dependencies.supported_platforms);
    let credentials = match installed_projection
        .as_ref()
        .and_then(|projection| projection.manifest.as_ref())
    {
        Some(installed_manifest) => presented_credentials(
            installed_manifest,
            owner_user_id,
            &source.catalog.id,
            applications,
        )?,
        None => Vec::new(),
    };
    Ok(PresentedPlugin {
        id: source.catalog.id.clone(),
        package_name: Some(source.catalog.name.clone()),
        plugin_key: Some(source.catalog.plugin_key.clone()),
        display_name: safe_text(&source.catalog.display_name, "Plugin"),
        description: safe_text(&source.catalog.description, ""),
        category: safe_text(&source.catalog.interface.category, "Plugin"),
        publisher: safe_text(&source.catalog.publisher.name, "ChatOS"),
        latest_version: source.release.version.clone(),
        update_available: installed
            && installed_version.as_deref() != Some(source.release.version.as_str()),
        installed_version,
        installed,
        enabled,
        has_ui: source.catalog.has_ui || !manifest.ui.is_empty(),
        source: "marketplace",
        runtime_status: runtime_status(installed, enabled),
        runtime_tool_count: None,
        runtime_last_checked_at: None,
        runtime_error: None,
        mcp_server_count: manifest.mcp_servers.len(),
        hook_count: manifest.hooks.len(),
        hook_runtime_status: (!manifest.hooks.is_empty()).then_some("ready"),
        skills: presented_skills(manifest),
        permissions: presented_permissions(&manifest.permissions),
        installation_issue: installed_projection.and_then(|projection| projection.issue),
        windows_compatible: compatible,
        install_action_available: compatible
            && (manifest
                .mcp_servers
                .iter()
                .any(|server| matches!(server, PluginMcpServer::Stdio { .. }))
                || !manifest.skills.is_empty()
                || !manifest.hooks.is_empty()),
        credentials,
    })
}

fn project_local_plugin(
    plugin_id: &str,
    records: &[LocalPluginInstallationRecord],
    plugin_root: &Path,
    owner_user_id: &str,
    applications: &PluginApplicationManager,
) -> Result<PresentedPlugin, String> {
    let projection = installed_projection(plugin_root, records);
    let manifest = projection.manifest.as_ref();
    let credentials = match manifest {
        Some(manifest) => presented_credentials(manifest, owner_user_id, plugin_id, applications)?,
        None => Vec::new(),
    };
    Ok(PresentedPlugin {
        id: plugin_id.to_owned(),
        package_name: manifest.map(|manifest| manifest.name.clone()),
        plugin_key: None,
        display_name: manifest
            .map(|manifest| safe_text(&manifest.interface.display_name, &manifest.name))
            .unwrap_or_else(|| plugin_id.to_owned()),
        description: manifest
            .map(|manifest| safe_text(&manifest.description, ""))
            .unwrap_or_default(),
        category: manifest
            .map(|manifest| safe_text(&manifest.interface.category, "Plugin"))
            .unwrap_or_else(|| "Plugin".into()),
        publisher: manifest
            .map(|manifest| safe_text(&manifest.interface.developer_name, &manifest.author.name))
            .unwrap_or_else(|| "Local".into()),
        latest_version: manifest
            .map(|manifest| manifest.version.clone())
            .unwrap_or_else(|| "unknown".into()),
        installed_version: manifest.map(|manifest| manifest.version.clone()),
        installed: true,
        update_available: false,
        enabled: projection.enabled,
        has_ui: manifest.is_some_and(|manifest| !manifest.ui.is_empty()),
        source: "local",
        runtime_status: runtime_status(true, projection.enabled),
        runtime_tool_count: None,
        runtime_last_checked_at: None,
        runtime_error: None,
        mcp_server_count: manifest.map_or(0, |manifest| manifest.mcp_servers.len()),
        hook_count: manifest.map_or(0, |manifest| manifest.hooks.len()),
        hook_runtime_status: manifest
            .is_some_and(|manifest| !manifest.hooks.is_empty())
            .then_some("ready"),
        skills: manifest.map_or_else(Vec::new, presented_skills),
        permissions: manifest.map_or_else(Vec::new, |manifest| {
            presented_permissions(&manifest.permissions)
        }),
        installation_issue: projection.issue,
        windows_compatible: manifest
            .is_none_or(|manifest| windows_compatible(&manifest.dependencies.supported_platforms)),
        install_action_available: false,
        credentials,
    })
}

pub(super) fn installed_projection(
    plugin_root: &Path,
    records: &[LocalPluginInstallationRecord],
) -> InstalledProjection {
    let enabled = !records.is_empty() && records.iter().all(|record| record.spec.enabled);
    let Some(record) = records
        .iter()
        .max_by_key(|record| record.updated_at_unix_ms)
    else {
        return InstalledProjection {
            manifest: None,
            enabled: false,
            issue: Some("metadata_invalid"),
        };
    };
    match load_installed_manifest(plugin_root, record) {
        Ok(manifest) => InstalledProjection {
            manifest: Some(manifest),
            enabled,
            issue: None,
        },
        Err(_) => InstalledProjection {
            manifest: None,
            enabled,
            issue: Some("integrity_check_failed"),
        },
    }
}

pub(super) fn presented_skills(manifest: &PluginManifest) -> Vec<String> {
    let mut skills = manifest
        .skills
        .iter()
        .filter_map(|skill| {
            skill
                .path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .map(|name| name.trim_end_matches(".md").to_owned())
        })
        .collect::<Vec<_>>();
    skills.sort_by_key(|value| value.to_lowercase());
    skills.dedup();
    skills.truncate(128);
    skills
}

fn presented_permissions(
    permissions: &[PluginPermissionRequirement],
) -> Vec<PresentedPluginPermission> {
    permissions
        .iter()
        .take(128)
        .map(|permission| PresentedPluginPermission {
            id: safe_text(&permission.permission, "unknown"),
            required: permission.required,
            reason: permission
                .reason
                .as_deref()
                .map(|reason| safe_text(reason, ""))
                .filter(|reason| !reason.is_empty()),
            // A declaration is not proof that Windows granted or enforced the permission.
            status: "declared_not_checked",
        })
        .collect()
}

pub(super) fn runtime_status(installed: bool, enabled: bool) -> &'static str {
    if !installed {
        "not_installed"
    } else if enabled {
        "not_checked"
    } else {
        "disabled"
    }
}

pub(super) fn windows_compatible(platforms: &[String]) -> bool {
    platforms.is_empty()
        || platforms.iter().any(|platform| {
            let platform = platform.trim().to_ascii_lowercase();
            platform == "windows" || platform.starts_with("windows-")
        })
}

pub(super) fn safe_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAXIMUM_PRESENTED_TEXT_BYTES
        || value.chars().any(|character| character == '\0')
    {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}
