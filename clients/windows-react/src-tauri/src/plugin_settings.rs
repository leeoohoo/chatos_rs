mod hooks;
mod installation;
mod presentation;

use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalPluginHookBundleRecord, LocalPluginInstallationRecord,
    LocalPluginInstallationSpec, PutPluginInstallationCommand, RemovePluginInstallationCommand,
};
use chatos_plugin_management_sdk::{PluginInstallSource, PluginManifest, PluginMcpServer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    authentication::AuthenticationManager,
    computer_use_worker::{diagnose as diagnose_computer_use, TrustedComputerUseLaunch},
    connector_state,
    host_command_client::HostCommandClient,
    host_lifecycle::HostManager,
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePermit,
        MachineResourcePriority,
    },
    plugin_application::PluginApplicationManager,
    plugin_credentials::{environment_secret_references, secret_name},
    plugin_installer::{
        inspect_local_package_source, install_git_package, install_local_package,
        install_marketplace_package, install_npm_mcp, install_npm_package_source,
        install_program_mcp as install_program_mcp_package, resolve_executable,
        trusted_local_hook_bundles, CommittedPluginPackage, LocalPluginHookSummary, NpmMcpDraft,
        NpmMcpInspection, ProgramMcpDraft, ProgramMcpInspection,
    },
    plugin_integrity::{
        recover_interrupted_installations, requires_installation_integrity,
        secure_component_revision,
    },
};

use hooks::{
    deactivate_hook_bundles, list_all_hook_bundle_records, list_hook_bundle_records,
    publish_inactive_hook_bundles, reconcile_active_hook_bundles, reconcile_current_hook_bundles,
};
use installation::{
    confined_plugin_root, installation_manifest_path, installation_roots,
    is_local_plugin_installation, list_installation_records, load_installed_manifest,
    matching_records, remove_installation_roots, set_records_enabled,
};
pub(crate) use installation::{installed_manifest_version, list_installation_records_for_owner};
use presentation::{project_snapshot, safe_text, windows_compatible};

#[cfg(test)]
use crate::host_command_client::HostCommandFuture;
#[cfg(test)]
use chatos_local_agent_protocol::{LocalPluginHookBundleSpec, SetActivePluginHookBundlesCommand};
#[cfg(test)]
use presentation::{presented_skills, runtime_status};

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

    fn stage_manual_selection(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        generation: u64,
        canonical_root: PathBuf,
        directory_name: String,
    ) -> Result<ManualPluginDirectorySelection, String> {
        let selection_id = format!("plugin-source-{}", uuid::Uuid::new_v4().simple());
        let mut selections = self
            .manual_selections
            .lock()
            .map_err(|_| "Plugin 手动安装状态不可用。".to_owned())?;
        for selection in selections.values_mut().filter(|selection| {
            selection.owner_user_id == owner_user_id
                && selection.presentation_id == presentation_id
                && selection.operation_id.is_some()
        }) {
            selection.discard_after_operation = true;
        }
        selections.retain(|_, selection| {
            selection.owner_user_id != owner_user_id
                || selection.presentation_id != presentation_id
                || selection.operation_id.is_some()
        });
        while selections.len() >= 64 {
            let removable = selections
                .iter()
                .find_map(|(key, selection)| selection.operation_id.is_none().then(|| key.clone()))
                .ok_or_else(|| "Plugin 手动安装操作过多，请稍后重试。".to_owned())?;
            selections.remove(&removable);
        }
        selections.insert(
            selection_id.clone(),
            ManualPluginSelection {
                owner_user_id: owner_user_id.to_owned(),
                presentation_id: presentation_id.to_owned(),
                generation,
                canonical_root,
                directory_name: directory_name.clone(),
                inspected_fingerprint: None,
                operation_id: None,
                discard_after_operation: false,
            },
        );
        Ok(ManualPluginDirectorySelection {
            selection_id,
            directory_name,
        })
    }

    fn begin_manual_operation(
        &self,
        input: &ManualPluginSelectionInput,
        generation: u64,
        expected_fingerprint: Option<&str>,
    ) -> Result<(ManualPluginSelection, String), String> {
        let mut selections = self
            .manual_selections
            .lock()
            .map_err(|_| "Plugin 手动安装状态不可用。".to_owned())?;
        let selection = selections
            .get_mut(&input.selection_id)
            .filter(|selection| {
                selection.owner_user_id == input.owner_user_id
                    && selection.presentation_id == input.presentation_id
                    && selection.generation == generation
            })
            .ok_or_else(manual_selection_expired)?;
        if selection.operation_id.is_some() {
            return Err("Plugin 来源正在处理，请等待当前操作完成。".into());
        }
        if let Some(expected) = expected_fingerprint {
            if selection.inspected_fingerprint.as_deref() != Some(expected) {
                return Err("Plugin 预检结果已失效，请重新预检。".into());
            }
        }
        let operation_id = uuid::Uuid::new_v4().to_string();
        selection.operation_id = Some(operation_id.clone());
        Ok((selection.clone(), operation_id))
    }

    fn finish_manual_inspection(
        &self,
        selection_id: &str,
        operation_id: &str,
        fingerprint: Option<String>,
    ) {
        let Ok(mut selections) = self.manual_selections.lock() else {
            return;
        };
        let Some(selection) = selections.get_mut(selection_id) else {
            return;
        };
        if selection.operation_id.as_deref() != Some(operation_id) {
            return;
        }
        if selection.discard_after_operation {
            selections.remove(selection_id);
            return;
        }
        selection.operation_id = None;
        selection.inspected_fingerprint = fingerprint;
    }

    fn finish_manual_install(&self, selection_id: &str, operation_id: &str, installed: bool) {
        let Ok(mut selections) = self.manual_selections.lock() else {
            return;
        };
        if selections
            .get(selection_id)
            .and_then(|selection| selection.operation_id.as_deref())
            != Some(operation_id)
        {
            return;
        }
        if installed
            || selections
                .get(selection_id)
                .is_some_and(|selection| selection.discard_after_operation)
        {
            selections.remove(selection_id);
        } else if let Some(selection) = selections.get_mut(selection_id) {
            selection.operation_id = None;
        }
    }

    fn discard_manual_selection(&self, owner_user_id: &str, presentation_id: &str) {
        let Ok(mut selections) = self.manual_selections.lock() else {
            return;
        };
        for selection in selections.values_mut().filter(|selection| {
            selection.owner_user_id == owner_user_id
                && selection.presentation_id == presentation_id
                && selection.operation_id.is_some()
        }) {
            selection.discard_after_operation = true;
        }
        selections.retain(|_, selection| {
            selection.owner_user_id != owner_user_id
                || selection.presentation_id != presentation_id
                || selection.operation_id.is_some()
        });
    }
}

#[derive(Clone, Debug)]
struct ManualPluginSelection {
    owner_user_id: String,
    presentation_id: String,
    generation: u64,
    canonical_root: PathBuf,
    directory_name: String,
    inspected_fingerprint: Option<String>,
    operation_id: Option<String>,
    discard_after_operation: bool,
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
        if manifest.name != "open-computer-use"
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

fn required_component_permission(
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginRuntimeHealthProjection {
    status: &'static str,
    checked_at: String,
    error: Option<String>,
    process_spawn_status: &'static str,
    computer_control_status: &'static str,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct LocalPluginInstallResult {
    pub(crate) plugin_id: String,
    pub(crate) display_name: String,
    pub(crate) version: String,
    pub(crate) component_count: usize,
    pub(crate) fingerprint: String,
}

pub(crate) async fn install_local_plugin_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    source_path: String,
    subdirectory: Option<String>,
    expected_fingerprint: String,
) -> Result<LocalPluginInstallResult, String> {
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    if source_path.is_empty() || source_path.len() > 4_096 || source_path.contains('\0') {
        return Err("本地 Plugin 来源路径无效。".into());
    }
    let plugin_root = settings.plugin_root.clone();
    let package = tauri::async_runtime::spawn_blocking(move || {
        install_local_package(
            &plugin_root,
            Path::new(&source_path),
            subdirectory.as_deref(),
            &expected_fingerprint,
        )
    })
    .await
    .map_err(|_| "本地 Plugin 安装任务异常退出。".to_owned())??;
    finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await
}

pub(crate) async fn install_git_plugin_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    repository_url: String,
    git_reference: Option<String>,
    subdirectory: Option<String>,
    expected_fingerprint: String,
) -> Result<LocalPluginInstallResult, String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let package = install_git_package(
        &settings.plugin_root,
        &repository_url,
        git_reference.as_deref(),
        subdirectory.as_deref(),
        &expected_fingerprint,
    )
    .await?;
    finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await
}

pub(crate) async fn install_npm_plugin_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    package_name: String,
    exact_version: String,
    subdirectory: Option<String>,
    expected_fingerprint: String,
) -> Result<LocalPluginInstallResult, String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let package = install_npm_package_source(
        &settings.plugin_root,
        &package_name,
        &exact_version,
        subdirectory.as_deref(),
        &expected_fingerprint,
    )
    .await?;
    finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await
}

pub(crate) async fn install_custom_npm_mcp(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    draft: NpmMcpDraft,
    expected_fingerprint: String,
) -> Result<(LocalPluginInstallResult, NpmMcpInspection), String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let (package, inspection) =
        install_npm_mcp(&settings.plugin_root, &draft, &expected_fingerprint).await?;
    let result = finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await?;
    Ok((result, inspection))
}

pub(crate) async fn install_program_mcp_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    draft: ProgramMcpDraft,
    expected_fingerprint: String,
) -> Result<(LocalPluginInstallResult, ProgramMcpInspection), String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let (package, inspection) =
        install_program_mcp_package(&settings.plugin_root, &draft, &expected_fingerprint).await?;
    let result = finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await?;
    Ok((result, inspection))
}

async fn finish_local_plugin_install(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    generation: u64,
    package: CommittedPluginPackage,
) -> Result<LocalPluginInstallResult, String> {
    if let Err(error) = authentication
        .require_session_generation(owner_user_id, generation)
        .await
    {
        package.rollback();
        return Err(error);
    }
    let plugin_id = format!(
        "local-plugin-{:x}",
        Sha256::digest(package.manifest.name.as_bytes())
    );
    let release_id = format!("local-release-{}", package.artifact_sha256);
    let hook_bundles =
        match trusted_local_hook_bundles(&package, owner_user_id, &plugin_id, &release_id) {
            Ok(bundles) => bundles,
            Err(error) => {
                package.rollback();
                return Err(error);
            }
        };
    let records = match list_installation_records(host, owner_user_id).await {
        Ok(records) => records,
        Err(error) => {
            package.rollback();
            return Err(error);
        }
    };
    let previous = records
        .clone()
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    let bundles_before = match list_hook_bundle_records(host, owner_user_id).await {
        Ok(bundles) => bundles,
        Err(error) => {
            package.rollback();
            return Err(error);
        }
    };
    let previous_roots = installation_roots(&settings.plugin_root, &previous).unwrap_or_default();
    if let Err(error) = publish_inactive_hook_bundles(host, owner_user_id, &hook_bundles).await {
        package.rollback();
        return Err(error);
    }
    if let Err(error) = authentication
        .require_session_generation(owner_user_id, generation)
        .await
    {
        package.rollback();
        return Err(error);
    }
    let specs =
        match local_installation_specs(owner_user_id, &plugin_id, &release_id, &package, &previous)
        {
            Ok(specs) => specs,
            Err(error) => {
                package.rollback();
                return Err(error);
            }
        };
    let component_count = specs.len();
    applications.reset().await;
    if let Err(error) = replace_installation_records(host, previous, specs).await {
        package.rollback();
        return Err(error);
    }
    let records_after = match list_installation_records(host, owner_user_id).await {
        Ok(records) => records,
        Err(error) => {
            let _ = deactivate_hook_bundles(host, owner_user_id).await;
            eprintln!("Hook Bundle reconciliation skipped: {error}");
            Vec::new()
        }
    };
    if !records_after.is_empty() {
        let mut bundles_to_reconcile = bundles_before;
        bundles_to_reconcile.extend(hook_bundles.iter().cloned().map(|bundle| {
            LocalPluginHookBundleRecord {
                spec: bundle,
                created_at_unix_ms: 0,
            }
        }));
        if let Err(error) = reconcile_active_hook_bundles(
            host,
            owner_user_id,
            &settings.plugin_root,
            &records_after,
            &bundles_to_reconcile,
        )
        .await
        {
            let _ = deactivate_hook_bundles(host, owner_user_id).await;
            eprintln!("Hook Bundle activation skipped: {error}");
        }
    }
    let result = LocalPluginInstallResult {
        plugin_id,
        display_name: safe_text(
            &package.manifest.interface.display_name,
            &package.manifest.name,
        ),
        version: package.manifest.version.clone(),
        component_count,
        fingerprint: package.artifact_sha256.clone(),
    };
    let installed_root = package.installation_root.clone();
    let _ = package.finish();
    let stale_roots = previous_roots
        .into_iter()
        .filter(|path| path != &installed_root)
        .collect();
    let _ = remove_installation_roots(&settings.plugin_root, stale_roots).await;
    applications.reset().await;
    Ok(result)
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

pub(crate) async fn install_marketplace_for_owner(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    plugin_id: &str,
    expected_release_id: Option<&str>,
) -> Result<PluginSettingsSnapshot, String> {
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let sources = authentication
        .plugin_install_sources_for_owner(owner_user_id)
        .await
        .map_err(|error| error.message)?;
    let source = sources
        .items
        .iter()
        .find(|source| source.catalog.id == plugin_id)
        .cloned()
        .ok_or_else(|| "Marketplace 中没有找到这个 Plugin。".to_owned())?;
    if expected_release_id.is_some_and(|expected| expected != source.release.id) {
        return Err("Marketplace Release 已变化，请重新搜索并确认后再安装。".into());
    }
    let records = list_installation_records(host, owner_user_id).await?;
    let previous = records
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    // A reinstall is also the repair path for a missing or corrupted legacy installation.
    // Do not let an unreadable old directory prevent a signed replacement from being staged.
    let previous_roots = installation_roots(&settings.plugin_root, &previous).unwrap_or_default();
    let response = authentication
        .download_plugin_artifact_for_owner(owner_user_id, &source.catalog.id, &source.release.id)
        .await
        .map_err(|error| error.message)?;
    let package =
        install_marketplace_package(settings.plugin_root.clone(), source.clone(), response).await?;
    let specs = match installation_specs(owner_user_id, &source, &package, &previous) {
        Ok(specs) => specs,
        Err(error) => {
            package.rollback();
            return Err(error);
        }
    };
    applications.reset().await;
    if let Err(error) = replace_installation_records(host, previous, specs).await {
        package.rollback();
        return Err(error);
    }
    let installed_root = package.installation_root.clone();
    let _ = package.finish();
    let stale_roots = previous_roots
        .into_iter()
        .filter(|path| path != &installed_root)
        .collect();
    let _ = remove_installation_roots(&settings.plugin_root, stale_roots).await;
    applications.reset().await;
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;
    load_snapshot(authentication, host, settings, owner_user_id, applications).await
}

fn installation_specs(
    owner_user_id: &str,
    source: &PluginInstallSource,
    package: &crate::plugin_installer::CommittedPluginPackage,
    previous: &[LocalPluginInstallationRecord],
) -> Result<Vec<LocalPluginInstallationSpec>, String> {
    let mut previous_by_component = BTreeMap::<String, &LocalPluginInstallationRecord>::new();
    for record in previous {
        match previous_by_component.get(&record.spec.server_id) {
            Some(current)
                if (current.updated_at_unix_ms, current.version)
                    >= (record.updated_at_unix_ms, record.version) => {}
            _ => {
                previous_by_component.insert(record.spec.server_id.clone(), record);
            }
        }
    }
    let enabled = source
        .preference
        .as_ref()
        .is_none_or(|preference| preference.enabled);
    let mut specs = Vec::new();
    for server in &package.manifest.mcp_servers {
        let PluginMcpServer::Stdio {
            component_key,
            bin,
            args,
            env,
            ..
        } = server
        else {
            continue;
        };
        let executable = resolve_executable(&package.installation_root, bin)?;
        let installation_id = previous_by_component
            .get(component_key)
            .map(|record| record.spec.installation_id.clone())
            .unwrap_or_else(|| format!("plugin-install-{}", uuid::Uuid::new_v4().simple()));
        let spec = LocalPluginInstallationSpec {
            installation_id,
            owner_user_id: owner_user_id.to_owned(),
            plugin_id: source.catalog.id.clone(),
            release_id: source.release.id.clone(),
            release_digest: format!("sha256:{}", source.release.artifact_sha256),
            component_id: component_key.clone(),
            component_revision: secure_component_revision(&source.release.artifact_sha256)?,
            server_id: component_key.clone(),
            executable_path: executable.to_string_lossy().into_owned(),
            args: args.clone(),
            working_directory: Some(package.installation_root.to_string_lossy().into_owned()),
            environment_secret_refs: environment_secret_references(
                owner_user_id,
                &source.catalog.id,
                env,
            )?,
            tool_prefix: None,
            allowed_tools: None,
            enabled,
        };
        spec.validate()
            .map_err(|_| "Plugin 安装记录无效。".to_owned())?;
        specs.push(spec);
    }
    if specs.is_empty()
        && package.manifest.mcp_servers.is_empty()
        && (!package.manifest.skills.is_empty() || !package.manifest.hooks.is_empty())
    {
        let component_key = if package.manifest.skills.is_empty() {
            "hooks-only"
        } else {
            "skills-only"
        };
        let manifest_path = package.installation_root.join("chatos.plugin.json");
        let installation_id = previous_by_component
            .get(component_key)
            .map(|record| record.spec.installation_id.clone())
            .unwrap_or_else(|| format!("plugin-install-{}", uuid::Uuid::new_v4().simple()));
        let spec = LocalPluginInstallationSpec {
            installation_id,
            owner_user_id: owner_user_id.to_owned(),
            plugin_id: source.catalog.id.clone(),
            release_id: source.release.id.clone(),
            release_digest: format!("sha256:{}", source.release.artifact_sha256),
            component_id: component_key.into(),
            component_revision: secure_component_revision(&source.release.artifact_sha256)?,
            server_id: component_key.into(),
            executable_path: manifest_path.to_string_lossy().into_owned(),
            args: Vec::new(),
            working_directory: Some(package.installation_root.to_string_lossy().into_owned()),
            environment_secret_refs: BTreeMap::new(),
            tool_prefix: None,
            allowed_tools: None,
            enabled,
        };
        spec.validate()
            .map_err(|_| "Plugin Skill 安装记录无效。".to_owned())?;
        specs.push(spec);
    }
    if specs.is_empty() {
        return Err("Plugin 没有 Windows 可运行的 stdio MCP 或有效 Skill。".into());
    }
    Ok(specs)
}

fn local_installation_specs(
    owner_user_id: &str,
    plugin_id: &str,
    release_id: &str,
    package: &crate::plugin_installer::CommittedPluginPackage,
    previous: &[LocalPluginInstallationRecord],
) -> Result<Vec<LocalPluginInstallationSpec>, String> {
    let mut previous_by_component = BTreeMap::<String, &LocalPluginInstallationRecord>::new();
    for record in previous {
        previous_by_component.insert(record.spec.server_id.clone(), record);
    }
    let mut specs = Vec::new();
    for server in &package.manifest.mcp_servers {
        let PluginMcpServer::Stdio {
            component_key,
            bin,
            args,
            env,
            ..
        } = server
        else {
            continue;
        };
        let executable = resolve_executable(&package.installation_root, bin)?;
        let spec = LocalPluginInstallationSpec {
            installation_id: previous_by_component
                .get(component_key)
                .map(|record| record.spec.installation_id.clone())
                .unwrap_or_else(|| format!("plugin-install-{}", uuid::Uuid::new_v4().simple())),
            owner_user_id: owner_user_id.to_owned(),
            plugin_id: plugin_id.to_owned(),
            release_id: release_id.to_owned(),
            release_digest: format!("sha256:{}", package.artifact_sha256),
            component_id: component_key.clone(),
            component_revision: secure_component_revision(&package.artifact_sha256)?,
            server_id: component_key.clone(),
            executable_path: executable.to_string_lossy().into_owned(),
            args: args.clone(),
            working_directory: Some(package.installation_root.to_string_lossy().into_owned()),
            environment_secret_refs: environment_secret_references(owner_user_id, plugin_id, env)?,
            tool_prefix: None,
            allowed_tools: None,
            enabled: true,
        };
        spec.validate()
            .map_err(|_| "Plugin 安装记录无效。".to_owned())?;
        specs.push(spec);
    }
    if specs.is_empty()
        && package.manifest.mcp_servers.is_empty()
        && (!package.manifest.skills.is_empty() || !package.manifest.hooks.is_empty())
    {
        let component_key = if package.manifest.skills.is_empty() {
            "hooks-only"
        } else {
            "skills-only"
        };
        let manifest_path = package.installation_root.join("chatos.plugin.json");
        let spec = LocalPluginInstallationSpec {
            installation_id: previous_by_component
                .get(component_key)
                .map(|record| record.spec.installation_id.clone())
                .unwrap_or_else(|| format!("plugin-install-{}", uuid::Uuid::new_v4().simple())),
            owner_user_id: owner_user_id.to_owned(),
            plugin_id: plugin_id.to_owned(),
            release_id: release_id.to_owned(),
            release_digest: format!("sha256:{}", package.artifact_sha256),
            component_id: component_key.into(),
            component_revision: secure_component_revision(&package.artifact_sha256)?,
            server_id: component_key.into(),
            executable_path: manifest_path.to_string_lossy().into_owned(),
            args: Vec::new(),
            working_directory: Some(package.installation_root.to_string_lossy().into_owned()),
            environment_secret_refs: BTreeMap::new(),
            tool_prefix: None,
            allowed_tools: None,
            enabled: true,
        };
        spec.validate()
            .map_err(|_| "Plugin Skill 安装记录无效。".to_owned())?;
        specs.push(spec);
    }
    if specs.is_empty() {
        return Err("Plugin 没有 Windows 可运行的 stdio MCP 或有效 Skill。".into());
    }
    Ok(specs)
}

async fn replace_installation_records(
    host: &dyn HostCommandClient,
    previous: Vec<LocalPluginInstallationRecord>,
    specs: Vec<LocalPluginInstallationSpec>,
) -> Result<(), String> {
    let mut previous_by_id = previous
        .into_iter()
        .map(|record| (record.spec.installation_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut applied = Vec::<(
        Option<LocalPluginInstallationRecord>,
        LocalPluginInstallationRecord,
    )>::new();
    let mut removed = Vec::<LocalPluginInstallationRecord>::new();
    for spec in specs {
        let before = previous_by_id.remove(&spec.installation_id);
        let expected_version = before.as_ref().map(|record| record.version);
        let result = host
            .execute(HostCommand::PutPluginInstallation(
                PutPluginInstallationCommand {
                    installation: spec.clone(),
                    expected_version,
                },
            ))
            .await;
        match result {
            Ok(HostResult::PluginInstallation { installation })
                if installation.spec == spec && installation.version > 0 =>
            {
                applied.push((before, installation));
            }
            _ => {
                rollback_installation_records(host, &applied, &removed).await;
                return Err("Plugin 安装记录写入失败，已尝试恢复原状态。".into());
            }
        }
    }
    for record in previous_by_id.into_values() {
        let result = host
            .execute(HostCommand::RemovePluginInstallation(
                RemovePluginInstallationCommand {
                    owner_user_id: record.spec.owner_user_id.clone(),
                    installation_id: record.spec.installation_id.clone(),
                    expected_version: record.version,
                },
            ))
            .await;
        match result {
            Ok(HostResult::PluginInstallation { .. }) => removed.push(record),
            _ => {
                rollback_installation_records(host, &applied, &removed).await;
                return Err("Plugin 旧版本记录清理失败，已尝试恢复原状态。".into());
            }
        }
    }
    Ok(())
}

async fn rollback_installation_records(
    host: &dyn HostCommandClient,
    applied: &[(
        Option<LocalPluginInstallationRecord>,
        LocalPluginInstallationRecord,
    )],
    removed: &[LocalPluginInstallationRecord],
) {
    for record in removed.iter().rev() {
        let _ = host
            .execute(HostCommand::PutPluginInstallation(
                PutPluginInstallationCommand {
                    installation: record.spec.clone(),
                    expected_version: None,
                },
            ))
            .await;
    }
    for (before, after) in applied.iter().rev() {
        match before {
            Some(before) => {
                let _ = host
                    .execute(HostCommand::PutPluginInstallation(
                        PutPluginInstallationCommand {
                            installation: before.spec.clone(),
                            expected_version: Some(after.version),
                        },
                    ))
                    .await;
            }
            None => {
                let _ = host
                    .execute(HostCommand::RemovePluginInstallation(
                        RemovePluginInstallationCommand {
                            owner_user_id: after.spec.owner_user_id.clone(),
                            installation_id: after.spec.installation_id.clone(),
                            expected_version: after.version,
                        },
                    ))
                    .await;
            }
        }
    }
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

fn presented_credentials(
    manifest: &PluginManifest,
    owner_user_id: &str,
    plugin_id: &str,
    applications: &PluginApplicationManager,
) -> Result<Vec<PresentedPluginCredential>, String> {
    declared_secret_names(manifest)?
        .into_iter()
        .map(|name| {
            Ok(PresentedPluginCredential {
                configured: applications.credential_is_configured(
                    owner_user_id,
                    plugin_id,
                    &name,
                )?,
                name,
            })
        })
        .collect()
}

fn declared_secret_names(manifest: &PluginManifest) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for server in &manifest.mcp_servers {
        let PluginMcpServer::Stdio { env, .. } = server else {
            continue;
        };
        for template in env.values() {
            names.insert(secret_name(template)?.to_owned());
        }
    }
    Ok(names)
}

fn validate_secret_bindings(
    owner_user_id: &str,
    plugin_id: &str,
    manifest: &PluginManifest,
    records: &[LocalPluginInstallationRecord],
) -> Result<(), String> {
    for record in records {
        let environment = manifest.mcp_servers.iter().find_map(|server| match server {
            PluginMcpServer::Stdio {
                component_key, env, ..
            } if component_key == &record.spec.server_id => Some(env),
            _ => None,
        });
        let expected = match environment {
            Some(environment) => {
                environment_secret_references(owner_user_id, plugin_id, environment)?
            }
            None if record.spec.server_id == "skills-only" => BTreeMap::new(),
            None => return Err("Plugin 凭据绑定与已安装清单不一致。".into()),
        };
        if record.spec.owner_user_id != owner_user_id
            || record.spec.plugin_id != plugin_id
            || record.spec.environment_secret_refs != expected
        {
            return Err("Plugin 凭据绑定与已安装清单不一致。".into());
        }
    }
    Ok(())
}

struct InstalledProjection {
    manifest: Option<PluginManifest>,
    enabled: bool,
    issue: Option<&'static str>,
}

async fn computer_use_runtime_health(
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
            match diagnose_computer_use(&launch, &settings.resources).await {
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

fn apply_computer_use_runtime_health(
    snapshot: &mut PluginSettingsSnapshot,
    health: Option<PluginRuntimeHealthProjection>,
) {
    let Some(plugin) = snapshot.plugins.iter_mut().find(|plugin| {
        plugin.installed && plugin.package_name.as_deref() == Some("open-computer-use")
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

fn validated_manual_selection_input(
    input: ManualPluginSelectionInput,
) -> Result<ManualPluginSelectionInput, String> {
    Ok(ManualPluginSelectionInput {
        owner_user_id: required_identifier("owner_user_id", input.owner_user_id)?,
        presentation_id: required_identifier("presentation_id", input.presentation_id)?,
        selection_id: required_identifier("selection_id", input.selection_id)?,
    })
}

fn prepare_manual_plugin_directory(path: &Path) -> Result<(PathBuf, String), String> {
    if !path.is_absolute() {
        return Err("Plugin 来源必须是绝对目录。".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| "Plugin 来源目录不存在。".to_owned())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata_is_reparse(&metadata) {
        return Err("Plugin 来源必须是普通目录，不能是链接或重解析点。".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "Plugin 来源目录不可用。".to_owned())?;
    let directory_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty() && value.len() <= 240 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "Plugin 来源目录名称无效。".to_owned())?
        .to_owned();
    Ok((canonical, directory_name))
}

fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn manual_plugin_components(
    manifest: &PluginManifest,
    hook_summaries: &[LocalPluginHookSummary],
) -> Vec<ManualPluginComponent> {
    let mut components = manifest
        .mcp_servers
        .iter()
        .map(|server| ManualPluginComponent {
            kind: "mcp",
            display_name: server.component_key().to_owned(),
            hook_count: None,
            events: Vec::new(),
            runtime_status: None,
        })
        .chain(manifest.skills.iter().map(|skill| {
            ManualPluginComponent {
                kind: "skill",
                display_name: Path::new(&skill.path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Skill")
                    .to_owned(),
                hook_count: None,
                events: Vec::new(),
                runtime_status: None,
            }
        }))
        .chain(hook_summaries.iter().map(|hook| ManualPluginComponent {
            kind: "hook",
            display_name: hook.component_key.clone(),
            hook_count: Some(hook.hook_count),
            events: hook.events.clone(),
            runtime_status: Some("ready"),
        }))
        .collect::<Vec<_>>();
    components.sort_by(|left, right| {
        left.kind
            .cmp(right.kind)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    components
}

fn manual_selection_expired() -> String {
    "Plugin 目录选择已失效，请重新选择。".into()
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
