mod computer_use;
mod hooks;
mod installation;
mod manual_selection;
mod presentation;
mod source_installation;

use chatos_local_agent_protocol::{HostCommand, HostResult, RemovePluginInstallationCommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    authentication::AuthenticationManager,
    connector_state,
    host_lifecycle::HostManager,
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePermit,
        MachineResourcePriority,
    },
    plugin_application::PluginApplicationManager,
    plugin_installer::inspect_local_package_source,
    plugin_integrity::{recover_interrupted_installations, requires_installation_integrity},
};

use computer_use::{apply_computer_use_runtime_health, computer_use_runtime_health};
pub(crate) use computer_use::{resolve_computer_use_installation, ComputerUseInstallation};
use hooks::{
    deactivate_hook_bundles, list_all_hook_bundle_records, reconcile_active_hook_bundles,
    reconcile_current_hook_bundles,
};
use installation::{
    installation_roots, is_local_plugin_installation, list_installation_records,
    load_installed_manifest, matching_records, remove_installation_roots, set_records_enabled,
};
pub(crate) use installation::{installed_manifest_version, list_installation_records_for_owner};
use manual_selection::{
    manual_plugin_components, prepare_manual_plugin_directory, validated_manual_selection_input,
    ManualPluginSelection,
};
use presentation::{declared_secret_names, project_snapshot, safe_text, validate_secret_bindings};
pub(crate) use source_installation::{
    install_custom_npm_mcp, install_git_plugin_source, install_local_plugin_source,
    install_marketplace_for_owner, install_npm_plugin_source, install_program_mcp_source,
};

#[cfg(test)]
use crate::host_command_client::{HostCommandClient, HostCommandFuture};
#[cfg(test)]
use crate::plugin_credentials::environment_secret_references;
#[cfg(test)]
use chatos_local_agent_protocol::{
    LocalPluginHookBundleRecord, LocalPluginHookBundleSpec, LocalPluginInstallationRecord,
    LocalPluginInstallationSpec, SetActivePluginHookBundlesCommand,
};
#[cfg(test)]
use computer_use::{required_component_permission, PluginRuntimeHealthProjection};
#[cfg(test)]
use hooks::publish_inactive_hook_bundles;
#[cfg(test)]
use installation::local_installation_specs;
#[cfg(all(test, windows))]
use manual_selection::metadata_is_reparse;
#[cfg(test)]
use presentation::{presented_skills, runtime_status, windows_compatible};
#[cfg(test)]
use std::fs;

const MAXIMUM_MANIFEST_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_PRESENTED_TEXT_BYTES: usize = 16 * 1_024;

#[derive(Clone)]
pub(crate) struct PluginSettingsManager {
    plugin_root: PathBuf,
    manual_selections: Arc<Mutex<BTreeMap<String, ManualPluginSelection>>>,
    installation_transactions: Arc<AsyncMutex<()>>,
    resources: MachineResourceGovernor,
}

impl PluginSettingsManager {
    #[cfg(test)]
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self::new_with_resources(app_data_directory, MachineResourceGovernor::default())
    }

    pub(crate) fn new_with_resources(
        app_data_directory: PathBuf,
        resources: MachineResourceGovernor,
    ) -> Self {
        Self {
            plugin_root: app_data_directory.join("NativeConnector").join("Plugins"),
            manual_selections: Arc::new(Mutex::new(BTreeMap::new())),
            installation_transactions: Arc::new(AsyncMutex::new(())),
            resources,
        }
    }

    async fn acquire_install_resource(&self) -> Result<MachineResourcePermit, String> {
        self.resources
            .acquire(
                MachineResourceKind::CodeReadWorker,
                MachineResourcePriority::Interactive,
            )
            .await
    }

    pub(crate) fn plugin_root(&self) -> &Path {
        &self.plugin_root
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
    runtime_tool_count: Option<usize>,
    runtime_last_checked_at: Option<String>,
    runtime_error: Option<String>,
    mcp_server_count: usize,
    hook_count: usize,
    hook_runtime_status: Option<&'static str>,
    skills: Vec<String>,
    permissions: Vec<PresentedPluginPermission>,
    installation_issue: Option<&'static str>,
    windows_compatible: bool,
    install_action_available: bool,
    credentials: Vec<PresentedPluginCredential>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PresentedPluginCredential {
    name: String,
    configured: bool,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCredentialsInput {
    owner_user_id: String,
    plugin_id: String,
    credentials: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManualPluginDirectorySelection {
    selection_id: String,
    directory_name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManualPluginInspection {
    selection_id: String,
    directory_name: String,
    display_name: String,
    version: String,
    description: String,
    fingerprint: String,
    components: Vec<ManualPluginComponent>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ManualPluginComponent {
    kind: &'static str,
    display_name: String,
    hook_count: Option<usize>,
    events: Vec<String>,
    runtime_status: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualPluginSelectionInput {
    owner_user_id: String,
    presentation_id: String,
    selection_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualPluginInstallInput {
    owner_user_id: String,
    presentation_id: String,
    selection_id: String,
    expected_fingerprint: String,
}

#[tauri::command]
pub async fn load_plugin_settings(
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    manager: State<'_, PluginSettingsManager>,
    applications: State<'_, PluginApplicationManager>,
    owner_user_id: String,
) -> Result<PluginSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let _transaction = manager.installation_transactions.lock().await;
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
    if let Ok(records) = &records {
        let expected =
            records
                .iter()
                .filter(|record| requires_installation_integrity(&record.spec.component_revision))
                .filter_map(|record| {
                    record.spec.working_directory.as_ref().map(|directory| {
                        (PathBuf::from(directory), record.spec.release_digest.clone())
                    })
                })
                .collect::<Vec<_>>();
        let plugin_root = manager.plugin_root.clone();
        let recovered = tauri::async_runtime::spawn_blocking(move || {
            recover_interrupted_installations(&plugin_root, &expected)
        })
        .await
        .map_err(|_| plugin_catalog_error())??;
        if recovered > 0 {
            applications.reset().await;
        }
    }
    let runtime_health = computer_use_runtime_health(&host, &manager, &owner_user_id).await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let mut snapshot = project_snapshot(
        &manager.plugin_root,
        sources.map_err(|error| error.message),
        records,
        &owner_user_id,
        &applications,
    )?;
    apply_computer_use_runtime_health(&mut snapshot, runtime_health);
    Ok(snapshot)
}

#[tauri::command]
pub async fn pick_manual_plugin_directory(
    authentication: State<'_, AuthenticationManager>,
    settings: State<'_, PluginSettingsManager>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<Option<ManualPluginDirectorySelection>, String> {
    let owner_user_id = required_identifier("owner_user_id", owner_user_id)?;
    let presentation_id = required_identifier("presentation_id", presentation_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("选择 Plugin / MCP 根目录")
        .pick_folder()
        .await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let requested = selected.path().to_path_buf();
    let (canonical_root, directory_name) =
        tauri::async_runtime::spawn_blocking(move || prepare_manual_plugin_directory(&requested))
            .await
            .map_err(|_| "Plugin 目录选择任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    settings
        .stage_manual_selection(
            &owner_user_id,
            &presentation_id,
            generation,
            canonical_root,
            directory_name,
        )
        .map(Some)
}

#[tauri::command]
pub async fn inspect_manual_plugin_directory(
    authentication: State<'_, AuthenticationManager>,
    settings: State<'_, PluginSettingsManager>,
    input: ManualPluginSelectionInput,
) -> Result<ManualPluginInspection, String> {
    let input = validated_manual_selection_input(input)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let (selection, operation_id) = settings.begin_manual_operation(&input, generation, None)?;
    let plugin_root = settings.plugin_root.clone();
    let source_root = selection.canonical_root.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        inspect_local_package_source(&plugin_root, &source_root, None)
    })
    .await
    .map_err(|_| "Plugin 预检任务异常退出。".to_owned())
    .and_then(|result| result);
    if let Err(error) = authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await
    {
        settings.finish_manual_inspection(&input.selection_id, &operation_id, None);
        return Err(error);
    }
    match result {
        Ok(inspection) => {
            let components = manual_plugin_components(&inspection.manifest, &inspection.hooks);
            let presented = ManualPluginInspection {
                selection_id: input.selection_id.clone(),
                directory_name: selection.directory_name,
                display_name: safe_text(
                    &inspection.manifest.interface.display_name,
                    &inspection.manifest.name,
                ),
                version: inspection.manifest.version,
                description: inspection.manifest.description,
                fingerprint: inspection.fingerprint.clone(),
                components,
            };
            settings.finish_manual_inspection(
                &input.selection_id,
                &operation_id,
                Some(inspection.fingerprint),
            );
            Ok(presented)
        }
        Err(error) => {
            settings.finish_manual_inspection(&input.selection_id, &operation_id, None);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn install_manual_plugin_directory(
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    settings: State<'_, PluginSettingsManager>,
    applications: State<'_, PluginApplicationManager>,
    input: ManualPluginInstallInput,
) -> Result<PluginSettingsSnapshot, String> {
    let selection_input = validated_manual_selection_input(ManualPluginSelectionInput {
        owner_user_id: input.owner_user_id,
        presentation_id: input.presentation_id,
        selection_id: input.selection_id,
    })?;
    if input.expected_fingerprint.len() != 64
        || !input
            .expected_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("Plugin 预检指纹无效。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&selection_input.owner_user_id)
        .await?;
    let (selection, operation_id) = settings.begin_manual_operation(
        &selection_input,
        generation,
        Some(&input.expected_fingerprint),
    )?;
    let result = install_local_plugin_source(
        &authentication,
        &host,
        &settings,
        &applications,
        &selection_input.owner_user_id,
        selection.canonical_root.to_string_lossy().into_owned(),
        None,
        input.expected_fingerprint,
    )
    .await;
    settings.finish_manual_install(&selection_input.selection_id, &operation_id, result.is_ok());
    result?;
    load_snapshot(
        &authentication,
        &host,
        &settings,
        &selection_input.owner_user_id,
        &applications,
    )
    .await
}

#[tauri::command]
pub async fn discard_manual_plugin_directory(
    settings: State<'_, PluginSettingsManager>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<(), String> {
    let owner_user_id = required_identifier("owner_user_id", owner_user_id)?;
    let presentation_id = required_identifier("presentation_id", presentation_id)?;
    settings.discard_manual_selection(&owner_user_id, &presentation_id);
    Ok(())
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
    // Local installs are intentionally independent from the Marketplace. A
    // transient Marketplace outage must not prevent users from enabling or
    // disabling a package that is already installed and integrity-checked on
    // this machine. Keep the source gate for every Marketplace-looking
    // installation so a missing source cannot weaken signed package checks.
    let is_local = targets.iter().all(is_local_plugin_installation);
    if sources.is_err() && !is_local {
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
    if let Err(error) =
        reconcile_current_hook_bundles(&*host, &settings.plugin_root, &owner_user_id).await
    {
        let _ = deactivate_hook_bundles(&*host, &owner_user_id).await;
        eprintln!("Hook Bundle 状态同步失败，已停用 active set: {error}");
    }
    applications.reset().await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(
        &authentication,
        &host,
        &settings,
        &owner_user_id,
        &applications,
    )
    .await
}

#[tauri::command]
pub async fn update_plugin_credentials(
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    settings: State<'_, PluginSettingsManager>,
    applications: State<'_, PluginApplicationManager>,
    input: PluginCredentialsInput,
) -> Result<PluginSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    let plugin_id = required_identifier("plugin_id", input.plugin_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let records = matching_records(
        list_installation_records(&*host, &owner_user_id).await?,
        &plugin_id,
    )?;
    let manifest = records
        .iter()
        .max_by_key(|record| record.updated_at_unix_ms)
        .ok_or_else(|| "Plugin 尚未安装。".to_owned())
        .and_then(|record| load_installed_manifest(&settings.plugin_root, record))?;
    let declared = declared_secret_names(&manifest)?;
    if declared.is_empty() || input.credentials.keys().cloned().collect::<BTreeSet<_>>() != declared
    {
        return Err("提交的 Plugin 凭据名称与已安装清单不一致。".into());
    }
    validate_secret_bindings(&owner_user_id, &plugin_id, &manifest, &records)?;
    applications.update_credentials(&owner_user_id, &plugin_id, &input.credentials)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    applications.reset().await;
    load_snapshot(
        &authentication,
        &host,
        &settings,
        &owner_user_id,
        &applications,
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
    let manifest = targets
        .iter()
        .max_by_key(|record| record.updated_at_unix_ms)
        .ok_or_else(|| "Plugin 尚未安装。".to_owned())
        .and_then(|record| load_installed_manifest(&settings.plugin_root, record))?;
    let credential_names = declared_secret_names(&manifest)?;
    validate_secret_bindings(&owner_user_id, &plugin_id, &manifest, &targets)?;
    let installation_roots = installation_roots(&settings.plugin_root, &targets)?;
    let bundles_before = list_all_hook_bundle_records(&*host, &owner_user_id).await?;
    deactivate_hook_bundles(&*host, &owner_user_id).await?;
    let deleted_credentials =
        applications.delete_credentials(&owner_user_id, &plugin_id, &credential_names)?;
    for record in &targets {
        let result = match host
            .execute(HostCommand::RemovePluginInstallation(
                RemovePluginInstallationCommand {
                    owner_user_id: owner_user_id.clone(),
                    installation_id: record.spec.installation_id.clone(),
                    expected_version: record.version,
                },
            ))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let restored = applications
                    .update_credentials(&owner_user_id, &plugin_id, &deleted_credentials)
                    .is_ok();
                return Err(if restored {
                    error
                } else {
                    "Plugin 卸载失败，且系统凭据回滚未完全成功。".into()
                });
            }
        };
        let HostResult::PluginInstallation { installation } = result else {
            let restored = applications
                .update_credentials(&owner_user_id, &plugin_id, &deleted_credentials)
                .is_ok();
            return Err(if restored {
                metadata_response_error()
            } else {
                "Plugin 卸载响应无效，且系统凭据回滚未完全成功。".into()
            });
        };
        if installation.spec.installation_id != record.spec.installation_id
            || installation.spec.owner_user_id != owner_user_id
        {
            let restored = applications
                .update_credentials(&owner_user_id, &plugin_id, &deleted_credentials)
                .is_ok();
            return Err(if restored {
                metadata_response_error()
            } else {
                "Plugin 卸载响应无效，且系统凭据回滚未完全成功。".into()
            });
        }
    }
    applications.reset().await;
    remove_installation_roots(&settings.plugin_root, installation_roots).await?;
    let reconciliation = match list_installation_records(&*host, &owner_user_id).await {
        Ok(records) => {
            reconcile_active_hook_bundles(
                &*host,
                &owner_user_id,
                &settings.plugin_root,
                &records,
                &bundles_before,
            )
            .await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = reconciliation {
        let _ = deactivate_hook_bundles(&*host, &owner_user_id).await;
        eprintln!("卸载后 Hook Bundle 状态同步失败，active set 保持为空: {error}");
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(
        &authentication,
        &host,
        &settings,
        &owner_user_id,
        &applications,
    )
    .await
}

#[tauri::command]
pub async fn install_marketplace_plugin(
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    settings: State<'_, PluginSettingsManager>,
    applications: State<'_, PluginApplicationManager>,
    input: PluginIdentityInput,
) -> Result<PluginSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    let plugin_id = required_identifier("plugin_id", input.plugin_id)?;
    install_marketplace_for_owner(
        &authentication,
        &host,
        &settings,
        &applications,
        &owner_user_id,
        &plugin_id,
        None,
    )
    .await
}

async fn load_snapshot(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    owner_user_id: &str,
    applications: &PluginApplicationManager,
) -> Result<PluginSettingsSnapshot, String> {
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let (sources, records) = tokio::join!(
        authentication.plugin_install_sources_for_owner(owner_user_id),
        list_installation_records(host, owner_user_id),
    );
    let runtime_health = computer_use_runtime_health(host, settings, owner_user_id).await;
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;
    let mut snapshot = project_snapshot(
        &settings.plugin_root,
        sources.map_err(|error| error.message),
        records,
        owner_user_id,
        applications,
    )?;
    apply_computer_use_runtime_health(&mut snapshot, runtime_health);
    Ok(snapshot)
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
#[path = "plugin_settings/tests.rs"]
mod tests;
