use chatos_local_agent_protocol::{
    GetPluginInstallationCommand, HostCommand, HostResult, ListPluginInstallationsCommand,
    LocalPluginInstallationRecord, LocalPluginInstallationSummary, PutPluginInstallationCommand,
    RemovePluginInstallationCommand,
};
use chatos_plugin_management_sdk::{
    parse_plugin_manifest, validate_plugin_manifest, PluginInstallSource, PluginInstallSourceList,
    PluginManifest, PluginPermissionRequirement,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};
use tauri::{AppHandle, Manager, State};

use crate::{
    authentication::AuthenticationManager, connector_state, host_lifecycle::HostManager,
    plugin_application::PluginApplicationManager,
};

const MAXIMUM_INSTALLATIONS: usize = 1_000;
const MAXIMUM_MANIFEST_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_PRESENTED_TEXT_BYTES: usize = 16 * 1_024;

type PluginHostFuture<'a> = Pin<Box<dyn Future<Output = Result<HostResult, String>> + Send + 'a>>;

trait PluginSettingsHost: Send + Sync {
    fn execute(&self, command: HostCommand) -> PluginHostFuture<'_>;
}

impl PluginSettingsHost for HostManager {
    fn execute(&self, command: HostCommand) -> PluginHostFuture<'_> {
        Box::pin(HostManager::execute(self, command))
    }
}

#[derive(Clone)]
pub(crate) struct PluginSettingsManager {
    plugin_root: PathBuf,
}

impl PluginSettingsManager {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            plugin_root: app_data_directory.join("NativeConnector").join("Plugins"),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsSnapshot {
    plugins: Vec<PresentedPlugin>,
    marketplace_available: bool,
    local_catalog_available: bool,
    marketplace_error: Option<String>,
    local_catalog_error: Option<String>,
    secure_marketplace_install_available: bool,
    manual_install_available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PresentedPlugin {
    id: String,
    package_name: Option<String>,
    plugin_key: Option<String>,
    display_name: String,
    description: String,
    category: String,
    publisher: String,
    latest_version: String,
    installed_version: Option<String>,
    installed: bool,
    update_available: bool,
    enabled: bool,
    has_ui: bool,
    source: &'static str,
    runtime_status: &'static str,
    mcp_server_count: usize,
    skills: Vec<String>,
    permissions: Vec<PresentedPluginPermission>,
    installation_issue: Option<&'static str>,
    windows_compatible: bool,
    install_action_available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PresentedPluginPermission {
    id: String,
    required: bool,
    reason: Option<String>,
    status: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginIdentityInput {
    owner_user_id: String,
    plugin_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginEnabledInput {
    owner_user_id: String,
    plugin_id: String,
    enabled: bool,
}

#[tauri::command]
pub async fn load_plugin_settings(
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    manager: State<'_, PluginSettingsManager>,
    owner_user_id: String,
) -> Result<PluginSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let (sources, records) = tokio::join!(
        authentication.plugin_install_sources_for_owner(&owner_user_id),
        list_installation_records(&*host, &owner_user_id),
    );
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    if sources.is_err() && records.is_err() {
        return Err(
            "Plugin Marketplace 与本机安装目录均不可用，请检查网络和 Local Agent Host。".into(),
        );
    }
    project_snapshot(
        &manager.plugin_root,
        sources.map_err(|error| error.message),
        records,
        &owner_user_id,
    )
}

#[tauri::command]
pub async fn update_plugin_enabled(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    settings: State<'_, PluginSettingsManager>,
    applications: State<'_, PluginApplicationManager>,
    input: PluginEnabledInput,
) -> Result<PluginSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    let plugin_id = required_identifier("plugin_id", input.plugin_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let records = list_installation_records(&*host, &owner_user_id).await?;
    let targets = matching_records(records, &plugin_id)?;
    let sources = authentication
        .plugin_install_sources_for_owner(&owner_user_id)
        .await;
    let is_marketplace = sources.as_ref().is_ok_and(|sources| {
        sources
            .items
            .iter()
            .any(|source| source.catalog.id == plugin_id)
    });
    if sources.is_err() {
        return Err("无法确认 Plugin 来源，未更改本机运行状态。".into());
    }

    let device_id = if is_marketplace {
        Some(load_device_id(&app, &owner_user_id).await?)
    } else {
        None
    };
    if let Some(device_id) = device_id.as_deref() {
        authentication
            .update_plugin_preference_for_owner(
                &owner_user_id,
                &plugin_id,
                device_id,
                input.enabled,
            )
            .await
            .map_err(|error| error.message)?;
    }
    if let Err(error) = set_records_enabled(&*host, targets, input.enabled).await {
        if let Some(device_id) = device_id.as_deref() {
            let _ = authentication
                .update_plugin_preference_for_owner(
                    &owner_user_id,
                    &plugin_id,
                    device_id,
                    !input.enabled,
                )
                .await;
        }
        return Err(error);
    }
    applications.reset().await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(
        &authentication,
        &*host,
        &settings.plugin_root,
        &owner_user_id,
    )
    .await
}

#[tauri::command]
pub async fn uninstall_plugin(
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    settings: State<'_, PluginSettingsManager>,
    applications: State<'_, PluginApplicationManager>,
    input: PluginIdentityInput,
) -> Result<PluginSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    let plugin_id = required_identifier("plugin_id", input.plugin_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let records = list_installation_records(&*host, &owner_user_id).await?;
    let targets = matching_records(records, &plugin_id)?;
    let installation_roots = installation_roots(&settings.plugin_root, &targets)?;
    for record in &targets {
        let result = host
            .execute(HostCommand::RemovePluginInstallation(
                RemovePluginInstallationCommand {
                    owner_user_id: owner_user_id.clone(),
                    installation_id: record.spec.installation_id.clone(),
                    expected_version: record.version,
                },
            ))
            .await?;
        let HostResult::PluginInstallation { installation } = result else {
            return Err(metadata_response_error());
        };
        if installation.spec.installation_id != record.spec.installation_id
            || installation.spec.owner_user_id != owner_user_id
        {
            return Err(metadata_response_error());
        }
    }
    applications.reset().await;
    remove_installation_roots(&settings.plugin_root, installation_roots).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(
        &authentication,
        &*host,
        &settings.plugin_root,
        &owner_user_id,
    )
    .await
}

async fn load_snapshot(
    authentication: &AuthenticationManager,
    host: &dyn PluginSettingsHost,
    plugin_root: &Path,
    owner_user_id: &str,
) -> Result<PluginSettingsSnapshot, String> {
    let (sources, records) = tokio::join!(
        authentication.plugin_install_sources_for_owner(owner_user_id),
        list_installation_records(host, owner_user_id),
    );
    project_snapshot(
        plugin_root,
        sources.map_err(|error| error.message),
        records,
        owner_user_id,
    )
}

fn project_snapshot(
    plugin_root: &Path,
    sources: Result<PluginInstallSourceList, String>,
    records: Result<Vec<LocalPluginInstallationRecord>, String>,
    owner_user_id: &str,
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
            project_marketplace_plugin(source, grouped.get(&source.catalog.id), plugin_root),
        );
    }
    for (plugin_id, records) in &grouped {
        if !plugins.contains_key(plugin_id) {
            plugins.insert(
                plugin_id.clone(),
                project_local_plugin(plugin_id, records, plugin_root),
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
        // Windows does not yet have the macOS installer's download, signature, archive-path,
        // per-file integrity, rollback, and dependency-install chain. Keep these actions closed.
        secure_marketplace_install_available: false,
        manual_install_available: false,
    })
}

fn project_marketplace_plugin(
    source: &PluginInstallSource,
    installed: Option<&Vec<LocalPluginInstallationRecord>>,
    plugin_root: &Path,
) -> PresentedPlugin {
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
    PresentedPlugin {
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
        mcp_server_count: manifest.mcp_servers.len(),
        skills: presented_skills(manifest),
        permissions: presented_permissions(&manifest.permissions),
        installation_issue: installed_projection.and_then(|projection| projection.issue),
        windows_compatible: compatible,
        install_action_available: false,
    }
}

fn project_local_plugin(
    plugin_id: &str,
    records: &[LocalPluginInstallationRecord],
    plugin_root: &Path,
) -> PresentedPlugin {
    let projection = installed_projection(plugin_root, records);
    let manifest = projection.manifest.as_ref();
    PresentedPlugin {
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
        mcp_server_count: manifest.map_or(0, |manifest| manifest.mcp_servers.len()),
        skills: manifest.map_or_else(Vec::new, presented_skills),
        permissions: manifest.map_or_else(Vec::new, |manifest| {
            presented_permissions(&manifest.permissions)
        }),
        installation_issue: projection.issue,
        windows_compatible: manifest
            .is_none_or(|manifest| windows_compatible(&manifest.dependencies.supported_platforms)),
        install_action_available: false,
    }
}

struct InstalledProjection {
    manifest: Option<PluginManifest>,
    enabled: bool,
    issue: Option<&'static str>,
}

fn installed_projection(
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

fn presented_skills(manifest: &PluginManifest) -> Vec<String> {
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

fn runtime_status(installed: bool, enabled: bool) -> &'static str {
    if !installed {
        "not_installed"
    } else if enabled {
        "not_checked"
    } else {
        "disabled"
    }
}

fn windows_compatible(platforms: &[String]) -> bool {
    platforms.is_empty()
        || platforms.iter().any(|platform| {
            let platform = platform.trim().to_ascii_lowercase();
            platform == "windows" || platform.starts_with("windows-")
        })
}

fn safe_text(value: &str, fallback: &str) -> String {
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

fn group_records(
    records: Vec<LocalPluginInstallationRecord>,
    owner_user_id: &str,
) -> Result<BTreeMap<String, Vec<LocalPluginInstallationRecord>>, String> {
    let mut grouped = BTreeMap::<String, Vec<LocalPluginInstallationRecord>>::new();
    for record in records {
        record
            .spec
            .validate()
            .map_err(|_| metadata_response_error())?;
        if record.spec.owner_user_id != owner_user_id
            || record.version == 0
            || record.created_at_unix_ms < 0
            || record.updated_at_unix_ms < record.created_at_unix_ms
        {
            return Err(metadata_response_error());
        }
        grouped
            .entry(record.spec.plugin_id.clone())
            .or_default()
            .push(record);
    }
    Ok(grouped)
}

async fn list_installation_records(
    host: &dyn PluginSettingsHost,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginInstallationRecord>, String> {
    let mut records = Vec::new();
    for summary in list_installation_summaries(host, owner_user_id).await? {
        let result = host
            .execute(HostCommand::GetPluginInstallation(
                GetPluginInstallationCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    installation_id: summary.installation_id.clone(),
                },
            ))
            .await?;
        let HostResult::PluginInstallation { installation } = result else {
            return Err(metadata_response_error());
        };
        validate_record(&installation, &summary, owner_user_id)?;
        records.push(installation);
    }
    Ok(records)
}

async fn list_installation_summaries(
    host: &dyn PluginSettingsHost,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginInstallationSummary>, String> {
    let mut installations = Vec::new();
    let mut before_updated_at_unix_ms = None;
    let mut before_installation_id = None;
    let mut observed_cursors = BTreeSet::new();
    loop {
        let result = host
            .execute(HostCommand::ListPluginInstallations(
                ListPluginInstallationsCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    before_updated_at_unix_ms,
                    before_installation_id: before_installation_id.clone(),
                    limit: 200,
                },
            ))
            .await?;
        let HostResult::PluginInstallations { page } = result else {
            return Err(metadata_response_error());
        };
        if installations.len().saturating_add(page.installations.len()) > MAXIMUM_INSTALLATIONS {
            return Err("已安装 Plugin 数量超过本机目录上限。".into());
        }
        installations.extend(page.installations);
        match (
            page.next_before_updated_at_unix_ms,
            page.next_before_installation_id,
        ) {
            (Some(timestamp), Some(installation_id)) => {
                if timestamp < 0 || !observed_cursors.insert((timestamp, installation_id.clone())) {
                    return Err(metadata_response_error());
                }
                before_updated_at_unix_ms = Some(timestamp);
                before_installation_id = Some(installation_id);
            }
            (None, None) => return Ok(installations),
            _ => return Err(metadata_response_error()),
        }
    }
}

fn validate_record(
    record: &LocalPluginInstallationRecord,
    summary: &LocalPluginInstallationSummary,
    owner_user_id: &str,
) -> Result<(), String> {
    record
        .spec
        .validate()
        .map_err(|_| metadata_response_error())?;
    if record.spec.owner_user_id != owner_user_id
        || record.spec.installation_id != summary.installation_id
        || record.spec.plugin_id != summary.plugin_id
        || record.spec.release_id != summary.release_id
        || record.spec.component_id != summary.component_id
        || record.spec.component_revision != summary.component_revision
        || record.spec.server_id != summary.server_id
        || record.spec.enabled != summary.enabled
        || record.version != summary.version
        || record.created_at_unix_ms != summary.created_at_unix_ms
        || record.updated_at_unix_ms != summary.updated_at_unix_ms
    {
        return Err(metadata_response_error());
    }
    Ok(())
}

fn matching_records(
    records: Vec<LocalPluginInstallationRecord>,
    plugin_id: &str,
) -> Result<Vec<LocalPluginInstallationRecord>, String> {
    let records = records
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    if records.is_empty() {
        Err("Plugin 尚未安装。".into())
    } else {
        Ok(records)
    }
}

async fn set_records_enabled(
    host: &dyn PluginSettingsHost,
    records: Vec<LocalPluginInstallationRecord>,
    enabled: bool,
) -> Result<(), String> {
    let mut updated = Vec::<LocalPluginInstallationRecord>::new();
    for record in records {
        if record.spec.enabled == enabled {
            continue;
        }
        let mut next = record.spec.clone();
        next.enabled = enabled;
        let result = host
            .execute(HostCommand::PutPluginInstallation(
                PutPluginInstallationCommand {
                    installation: next,
                    expected_version: Some(record.version),
                },
            ))
            .await;
        match result {
            Ok(HostResult::PluginInstallation { installation }) => updated.push(installation),
            Ok(_) | Err(_) => {
                for changed in updated.into_iter().rev() {
                    let mut rollback = changed.spec.clone();
                    rollback.enabled = !enabled;
                    let _ = host
                        .execute(HostCommand::PutPluginInstallation(
                            PutPluginInstallationCommand {
                                installation: rollback,
                                expected_version: Some(changed.version),
                            },
                        ))
                        .await;
                }
                return Err("Plugin 状态更新失败，已尝试恢复原状态。".into());
            }
        }
    }
    Ok(())
}

fn load_installed_manifest(
    plugin_root: &Path,
    record: &LocalPluginInstallationRecord,
) -> Result<PluginManifest, String> {
    let root = confined_plugin_root(plugin_root)?;
    let manifest_path = installation_manifest_path(&root, record)?;
    let metadata = manifest_path
        .metadata()
        .map_err(|_| plugin_catalog_error())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let raw = fs::read_to_string(manifest_path).map_err(|_| plugin_catalog_error())?;
    parse_plugin_manifest(&raw).map_err(|_| plugin_catalog_error())
}

fn installation_manifest_path(
    plugin_root: &Path,
    record: &LocalPluginInstallationRecord,
) -> Result<PathBuf, String> {
    let executable = PathBuf::from(&record.spec.executable_path)
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !executable.is_file() || !executable.starts_with(plugin_root) {
        return Err(plugin_catalog_error());
    }
    let mut candidate = executable.parent().ok_or_else(plugin_catalog_error)?;
    loop {
        if !candidate.starts_with(plugin_root) {
            return Err(plugin_catalog_error());
        }
        let manifest = candidate.join("chatos.plugin.json");
        if manifest.is_file() {
            let manifest = manifest
                .canonicalize()
                .map_err(|_| plugin_catalog_error())?;
            return manifest
                .starts_with(candidate)
                .then_some(manifest)
                .ok_or_else(plugin_catalog_error);
        }
        if candidate == plugin_root {
            return Err(plugin_catalog_error());
        }
        candidate = candidate.parent().ok_or_else(plugin_catalog_error)?;
    }
}

fn confined_plugin_root(plugin_root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(plugin_root).map_err(|_| plugin_catalog_error())?;
    let root = plugin_root
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    root.is_dir()
        .then_some(root)
        .ok_or_else(plugin_catalog_error)
}

fn installation_roots(
    plugin_root: &Path,
    records: &[LocalPluginInstallationRecord],
) -> Result<BTreeSet<PathBuf>, String> {
    let root = confined_plugin_root(plugin_root)?;
    records
        .iter()
        .map(|record| {
            installation_manifest_path(&root, record).and_then(|path| {
                path.parent()
                    .filter(|directory| **directory != root && directory.starts_with(&root))
                    .map(Path::to_path_buf)
                    .ok_or_else(plugin_catalog_error)
            })
        })
        .collect()
}

async fn remove_installation_roots(
    plugin_root: &Path,
    installation_roots: BTreeSet<PathBuf>,
) -> Result<(), String> {
    let plugin_root = plugin_root.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let root = confined_plugin_root(&plugin_root)?;
        for installation_root in installation_roots {
            let canonical = installation_root
                .canonicalize()
                .map_err(|_| plugin_catalog_error())?;
            if canonical == root || !canonical.starts_with(&root) || !canonical.is_dir() {
                return Err(plugin_catalog_error());
            }
            fs::remove_dir_all(&canonical).map_err(|_| "Plugin 文件删除失败。".to_owned())?;
            if let Some(parent) = canonical.parent() {
                if parent != root
                    && parent.starts_with(&root)
                    && fs::read_dir(parent)
                        .map(|mut entries| entries.next().is_none())
                        .unwrap_or(false)
                {
                    let _ = fs::remove_dir(parent);
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "Plugin 文件删除任务异常退出。".to_owned())?
}

async fn load_device_id(app: &AppHandle, owner_user_id: &str) -> Result<String, String> {
    let data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法读取本机设备身份。".to_owned())?;
    let owner_user_id = owner_user_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        connector_state::ensure_owner_state(&data_directory, &owner_user_id)?;
        let state = connector_state::read_owner_state(&data_directory, &owner_user_id)?;
        let device_id = state
            .get("deviceID")
            .and_then(Value::as_str)
            .ok_or_else(|| "无法读取本机设备身份。".to_owned())?;
        required_identifier("device_id", device_id.to_owned())
    })
    .await
    .map_err(|_| "本机设备身份读取任务异常退出。".to_owned())?
}

fn required_identifier(field: &str, value: String) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 1_024
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err(format!("{field} is invalid"));
    }
    Ok(value.to_owned())
}

fn metadata_response_error() -> String {
    "Local Agent Host 返回了无效的 Plugin 安装记录。".into()
}

fn plugin_catalog_error() -> String {
    "本机 Plugin 清单未通过安全校验。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_gate_only_accepts_windows_or_universal_plugins() {
        assert!(windows_compatible(&[]));
        assert!(windows_compatible(&["windows".into()]));
        assert!(windows_compatible(&["windows-x86_64".into()]));
        assert!(!windows_compatible(&["macos-arm64".into(), "linux".into()]));
    }

    #[test]
    fn presented_text_and_skill_names_do_not_expose_paths() {
        assert_eq!(safe_text("  Diagram Studio  ", "Plugin"), "Diagram Studio");
        assert_eq!(safe_text("bad\0text", "Plugin"), "Plugin");
        let manifest = parse_plugin_manifest(
            r#"{
                "schemaVersion": 3,
                "name": "safe-plugin",
                "version": "1.0.0",
                "description": "Safe",
                "author": { "name": "ChatOS" },
                "skills": ["./skills/diagram-architecture", "./private/SKILL.md"],
                "mcpServers": {},
                "interface": {
                    "displayName": "Safe Plugin",
                    "shortDescription": "Safe",
                    "longDescription": "Safe plugin",
                    "developerName": "ChatOS",
                    "category": "Productivity"
                },
                "permissions": []
            }"#,
        )
        .unwrap();
        assert_eq!(
            presented_skills(&manifest),
            vec!["diagram-architecture".to_owned(), "SKILL".to_owned()]
        );
    }

    #[test]
    fn runtime_projection_never_claims_an_unchecked_plugin_is_ready() {
        assert_eq!(runtime_status(false, false), "not_installed");
        assert_eq!(runtime_status(true, false), "disabled");
        assert_eq!(runtime_status(true, true), "not_checked");
    }
}
