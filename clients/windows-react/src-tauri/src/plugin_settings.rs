use chatos_local_agent_protocol::{
    GetPluginInstallationCommand, HostCommand, HostResult, ListPluginInstallationsCommand,
    LocalPluginHookBundleSpec, LocalPluginInstallationRecord, LocalPluginInstallationSpec,
    LocalPluginInstallationSummary, PutPluginHookBundleCommand, PutPluginInstallationCommand,
    RemovePluginInstallationCommand, SetActivePluginHookBundlesCommand,
};
use chatos_plugin_management_sdk::{
    parse_plugin_manifest, validate_plugin_manifest, PluginInstallSource, PluginInstallSourceList,
    PluginManifest, PluginMcpServer, PluginPermissionRequirement,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    authentication::AuthenticationManager,
    computer_use_worker::TrustedComputerUseLaunch,
    connector_state,
    host_lifecycle::HostManager,
    plugin_application::PluginApplicationManager,
    plugin_credentials::{environment_secret_references, secret_name},
    plugin_installer::{
        inspect_local_package_source, install_git_package, install_local_package,
        install_marketplace_package, install_npm_mcp, install_npm_package_source,
        resolve_executable, trusted_local_hook_bundles, CommittedPluginPackage,
        LocalPluginHookSummary, NpmMcpDraft, NpmMcpInspection,
    },
    plugin_integrity::{
        recover_interrupted_installations, requires_installation_integrity,
        secure_component_revision, verify_installation_integrity,
    },
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
    manual_selections: Arc<Mutex<BTreeMap<String, ManualPluginSelection>>>,
    installation_transactions: Arc<AsyncMutex<()>>,
}

impl PluginSettingsManager {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            plugin_root: app_data_directory.join("NativeConnector").join("Plugins"),
            manual_selections: Arc::new(Mutex::new(BTreeMap::new())),
            installation_transactions: Arc::new(AsyncMutex::new(())),
        }
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
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
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

async fn publish_inactive_hook_bundles(
    host: &dyn PluginSettingsHost,
    owner_user_id: &str,
    bundles: &[LocalPluginHookBundleSpec],
) -> Result<(), String> {
    if bundles.is_empty() {
        return Ok(());
    }
    let result = host
        .execute(HostCommand::SetActivePluginHookBundles(
            SetActivePluginHookBundlesCommand {
                owner_user_id: owner_user_id.to_owned(),
                bundle_ids: Vec::new(),
            },
        ))
        .await?;
    if !matches!(
        result,
        HostResult::ActivePluginHookBundlesUpdated { active_bundle_ids }
            if active_bundle_ids.is_empty()
    ) {
        return Err("Local Agent Host 未确认 Hook Bundle 安全停用。".into());
    }
    for bundle in bundles {
        let result = host
            .execute(HostCommand::PutPluginHookBundle(
                PutPluginHookBundleCommand {
                    bundle: bundle.clone(),
                },
            ))
            .await?;
        if !matches!(
            result,
            HostResult::PluginHookBundle { bundle: record }
                if record.spec == *bundle
        ) {
            return Err("Local Agent Host 返回了无效的 Hook Bundle 记录。".into());
        }
    }
    Ok(())
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
    project_snapshot(
        &manager.plugin_root,
        sources.map_err(|error| error.message),
        records,
        &owner_user_id,
        &applications,
    )
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
        &*host,
        &settings.plugin_root,
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
        &*host,
        &settings.plugin_root,
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
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(
        &authentication,
        &*host,
        &settings.plugin_root,
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
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let sources = authentication
        .plugin_install_sources_for_owner(&owner_user_id)
        .await
        .map_err(|error| error.message)?;
    let source = sources
        .items
        .iter()
        .find(|source| source.catalog.id == plugin_id)
        .cloned()
        .ok_or_else(|| "Marketplace 中没有找到这个 Plugin。".to_owned())?;
    let records = list_installation_records(&*host, &owner_user_id).await?;
    let previous = records
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    // A reinstall is also the repair path for a missing or corrupted legacy installation.
    // Do not let an unreadable old directory prevent a signed replacement from being staged.
    let previous_roots = installation_roots(&settings.plugin_root, &previous).unwrap_or_default();
    let response = authentication
        .download_plugin_artifact_for_owner(&owner_user_id, &source.catalog.id, &source.release.id)
        .await
        .map_err(|error| error.message)?;
    let package =
        install_marketplace_package(settings.plugin_root.clone(), source.clone(), response).await?;
    let specs = match installation_specs(&owner_user_id, &source, &package, &previous) {
        Ok(specs) => specs,
        Err(error) => {
            package.rollback();
            return Err(error);
        }
    };
    applications.reset().await;
    if let Err(error) = replace_installation_records(&*host, previous, specs).await {
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
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(
        &authentication,
        &*host,
        &settings.plugin_root,
        &owner_user_id,
        &applications,
    )
    .await
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
        && !package.manifest.skills.is_empty()
    {
        let component_key = "skills-only";
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
        && !package.manifest.skills.is_empty()
    {
        let component_key = "skills-only";
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
    host: &dyn PluginSettingsHost,
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
    host: &dyn PluginSettingsHost,
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
    host: &dyn PluginSettingsHost,
    plugin_root: &Path,
    owner_user_id: &str,
    applications: &PluginApplicationManager,
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
        applications,
    )
}

fn project_snapshot(
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
        mcp_server_count: manifest.mcp_servers.len(),
        hook_count: manifest.hooks.len(),
        hook_runtime_status: (!manifest.hooks.is_empty()).then_some("runner_unavailable"),
        skills: presented_skills(manifest),
        permissions: presented_permissions(&manifest.permissions),
        installation_issue: installed_projection.and_then(|projection| projection.issue),
        windows_compatible: compatible,
        install_action_available: compatible
            && manifest
                .mcp_servers
                .iter()
                .any(|server| matches!(server, PluginMcpServer::Stdio { .. })),
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
        mcp_server_count: manifest.map_or(0, |manifest| manifest.mcp_servers.len()),
        hook_count: manifest.map_or(0, |manifest| manifest.hooks.len()),
        hook_runtime_status: manifest
            .is_some_and(|manifest| !manifest.hooks.is_empty())
            .then_some("runner_unavailable"),
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
    let installation_root = manifest_path.parent().ok_or_else(plugin_catalog_error)?;
    if requires_installation_integrity(&record.spec.component_revision) {
        verify_installation_integrity(&root, installation_root, &record.spec.release_digest)
            .map_err(|_| plugin_catalog_error())?;
    }
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
            runtime_status: Some("runner_unavailable"),
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
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    enum HookHostBehavior {
        Success,
        Error,
        WrongActiveReply,
        WrongBundleReply,
    }

    struct HookHost {
        behavior: HookHostBehavior,
        commands: Mutex<Vec<HostCommand>>,
    }

    impl HookHost {
        fn new(behavior: HookHostBehavior) -> Self {
            Self {
                behavior,
                commands: Mutex::new(Vec::new()),
            }
        }
    }

    impl PluginSettingsHost for HookHost {
        fn execute(&self, command: HostCommand) -> PluginHostFuture<'_> {
            self.commands.lock().unwrap().push(command.clone());
            let behavior = self.behavior;
            Box::pin(async move {
                match (behavior, command) {
                    (HookHostBehavior::Error, _) => Err("host unavailable".into()),
                    (
                        HookHostBehavior::WrongActiveReply,
                        HostCommand::SetActivePluginHookBundles(_),
                    ) => Ok(HostResult::ActivePluginHookBundlesUpdated {
                        active_bundle_ids: vec!["unexpected-active-bundle".into()],
                    }),
                    (
                        _,
                        HostCommand::SetActivePluginHookBundles(
                            SetActivePluginHookBundlesCommand { bundle_ids, .. },
                        ),
                    ) => Ok(HostResult::ActivePluginHookBundlesUpdated {
                        active_bundle_ids: bundle_ids,
                    }),
                    (HookHostBehavior::WrongBundleReply, HostCommand::PutPluginHookBundle(_)) => {
                        Ok(HostResult::ActivePluginHookBundlesUpdated {
                            active_bundle_ids: Vec::new(),
                        })
                    }
                    (_, HostCommand::PutPluginHookBundle(command)) => {
                        Ok(HostResult::PluginHookBundle {
                            bundle: chatos_local_agent_protocol::LocalPluginHookBundleRecord {
                                spec: command.bundle,
                                created_at_unix_ms: 1,
                            },
                        })
                    }
                    _ => panic!("unexpected host command"),
                }
            })
        }
    }

    fn hook_bundle_fixture() -> LocalPluginHookBundleSpec {
        LocalPluginHookBundleSpec {
            bundle_id: "bundle-1".into(),
            owner_user_id: "owner-1".into(),
            plugin_id: "plugin-1".into(),
            release_id: "release-1".into(),
            component_key: "lifecycle".into(),
            artifact_sha256: "a".repeat(64),
            content_sha256: "b".repeat(64),
            normalized_hook_sha256: "c".repeat(64),
            command_sha256: "d".repeat(64),
            permission_snapshot_sha256: "e".repeat(64),
            runner_policy_version: 1,
            installation_root: r"C:\ChatOS\Plugins\plugin-1".into(),
            permissions: vec!["process.spawn".into()],
            hooks: vec![chatos_local_agent_protocol::LocalPluginHookDefinition {
                hook_id: "SessionStart-0-0".into(),
                event: "SessionStart".into(),
                matcher: None,
                command: "node hook.js".into(),
                timeout_ms: 5_000,
                additional_context_limit: 65_536,
            }],
            enabled: true,
            trusted_at_unix_ms: 1,
        }
    }

    #[tokio::test]
    async fn hook_bundles_are_deactivated_before_they_are_published() {
        let host = HookHost::new(HookHostBehavior::Success);
        let bundle = hook_bundle_fixture();

        publish_inactive_hook_bundles(&host, "owner-1", std::slice::from_ref(&bundle))
            .await
            .unwrap();

        let commands = host.commands.lock().unwrap();
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            &commands[0],
            HostCommand::SetActivePluginHookBundles(command)
                if command.owner_user_id == "owner-1" && command.bundle_ids.is_empty()
        ));
        assert!(matches!(
            &commands[1],
            HostCommand::PutPluginHookBundle(command) if command.bundle == bundle
        ));
    }

    #[tokio::test]
    async fn hook_bundle_publication_rejects_host_errors_and_wrong_replies() {
        let bundle = hook_bundle_fixture();
        for behavior in [
            HookHostBehavior::Error,
            HookHostBehavior::WrongActiveReply,
            HookHostBehavior::WrongBundleReply,
        ] {
            let host = HookHost::new(behavior);
            assert!(
                publish_inactive_hook_bundles(&host, "owner-1", std::slice::from_ref(&bundle))
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn package_without_hooks_does_not_call_hook_host_api() {
        let host = HookHost::new(HookHostBehavior::Success);
        publish_inactive_hook_bundles(&host, "owner-1", &[])
            .await
            .unwrap();
        assert!(host.commands.lock().unwrap().is_empty());
    }

    fn manual_manager() -> (tempfile::TempDir, PluginSettingsManager) {
        let directory = tempfile::tempdir().unwrap();
        let manager = PluginSettingsManager::new(directory.path().join("app-data"));
        (directory, manager)
    }

    fn stage_manual(
        manager: &PluginSettingsManager,
        source_root: &Path,
    ) -> ManualPluginDirectorySelection {
        manager
            .stage_manual_selection(
                "owner-1",
                "presentation-1",
                7,
                source_root.to_path_buf(),
                "source-plugin".into(),
            )
            .unwrap()
    }

    fn manual_input(selection_id: &str) -> ManualPluginSelectionInput {
        ManualPluginSelectionInput {
            owner_user_id: "owner-1".into(),
            presentation_id: "presentation-1".into(),
            selection_id: selection_id.into(),
        }
    }

    #[test]
    fn manual_selection_is_bound_to_owner_presentation_and_session_generation() {
        let (directory, manager) = manual_manager();
        let selection = stage_manual(&manager, directory.path());
        let mut input = manual_input(&selection.selection_id);

        input.owner_user_id = "owner-2".into();
        assert!(manager.begin_manual_operation(&input, 7, None).is_err());
        input.owner_user_id = "owner-1".into();
        input.presentation_id = "presentation-2".into();
        assert!(manager.begin_manual_operation(&input, 7, None).is_err());
        input.presentation_id = "presentation-1".into();
        assert!(manager.begin_manual_operation(&input, 8, None).is_err());

        let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
        manager.finish_manual_inspection(&selection.selection_id, &operation_id, None);
    }

    #[test]
    fn manual_install_requires_the_exact_preflight_fingerprint() {
        let (directory, manager) = manual_manager();
        let selection = stage_manual(&manager, directory.path());
        let input = manual_input(&selection.selection_id);
        let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
        manager.finish_manual_inspection(
            &selection.selection_id,
            &operation_id,
            Some("a".repeat(64)),
        );

        assert!(manager
            .begin_manual_operation(&input, 7, Some(&"b".repeat(64)))
            .is_err());
        assert!(manager
            .begin_manual_operation(&input, 7, Some(&"a".repeat(64)))
            .is_ok());
    }

    #[test]
    fn manual_selection_rejects_concurrent_operations() {
        let (directory, manager) = manual_manager();
        let selection = stage_manual(&manager, directory.path());
        let input = manual_input(&selection.selection_id);
        let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();

        assert_eq!(
            manager.begin_manual_operation(&input, 7, None).unwrap_err(),
            "Plugin 来源正在处理，请等待当前操作完成。"
        );
        manager.finish_manual_inspection(&selection.selection_id, &operation_id, None);
    }

    #[test]
    fn successful_manual_install_consumes_the_selection() {
        let (directory, manager) = manual_manager();
        let selection = stage_manual(&manager, directory.path());
        let input = manual_input(&selection.selection_id);
        let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
        manager.finish_manual_install(&selection.selection_id, &operation_id, true);

        assert!(manager.begin_manual_operation(&input, 7, None).is_err());
    }

    #[test]
    fn discard_does_not_remove_a_leased_manual_operation() {
        let (directory, manager) = manual_manager();
        let selection = stage_manual(&manager, directory.path());
        let input = manual_input(&selection.selection_id);
        let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
        manager.discard_manual_selection("owner-1", "presentation-1");
        let replacement = stage_manual(&manager, directory.path());

        assert!(manager
            .manual_selections
            .lock()
            .unwrap()
            .contains_key(&selection.selection_id));
        assert_ne!(replacement.selection_id, selection.selection_id);
        manager.finish_manual_install(&selection.selection_id, &operation_id, false);
        assert!(!manager
            .manual_selections
            .lock()
            .unwrap()
            .contains_key(&selection.selection_id));
    }

    #[test]
    fn manual_directory_preflight_rejects_non_directories_and_links() {
        let directory = tempfile::tempdir().unwrap();
        let ordinary = directory.path().join("ordinary-plugin");
        fs::create_dir(&ordinary).unwrap();
        let (canonical, name) = prepare_manual_plugin_directory(&ordinary).unwrap();
        assert_eq!(canonical, ordinary.canonicalize().unwrap());
        assert_eq!(name, "ordinary-plugin");

        let file = directory.path().join("not-a-directory");
        fs::write(&file, b"plugin").unwrap();
        assert!(prepare_manual_plugin_directory(&file).is_err());
        assert!(prepare_manual_plugin_directory(Path::new("relative-plugin")).is_err());

        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_dir;
            let link = directory.path().join("linked-plugin");
            if symlink_dir(&ordinary, &link).is_ok() {
                assert!(prepare_manual_plugin_directory(&link).is_err());
                assert!(metadata_is_reparse(&fs::symlink_metadata(&link).unwrap()));
            }
        }
    }

    #[test]
    fn plugin_secret_references_are_opaque_owner_and_plugin_scoped() {
        let environment = BTreeMap::from([
            ("API_TOKEN".into(), "${credential:primary-token}".into()),
            (
                "SECONDARY_TOKEN".into(),
                "${credential:primary-token}".into(),
            ),
        ]);
        let first = environment_secret_references("owner-1", "plugin-1", &environment).unwrap();
        let same = environment_secret_references("owner-1", "plugin-1", &environment).unwrap();
        let other_owner =
            environment_secret_references("owner-2", "plugin-1", &environment).unwrap();
        let other_plugin =
            environment_secret_references("owner-1", "plugin-2", &environment).unwrap();

        assert_eq!(first, same);
        assert_eq!(first["API_TOKEN"], first["SECONDARY_TOKEN"]);
        assert_ne!(first["API_TOKEN"], other_owner["API_TOKEN"]);
        assert_ne!(first["API_TOKEN"], other_plugin["API_TOKEN"]);
        assert!(!first["API_TOKEN"].contains("owner-1"));
        assert!(!first["API_TOKEN"].contains("plugin-1"));
        assert!(!first["API_TOKEN"].contains("primary-token"));
        assert!(environment_secret_references(
            "owner-1",
            "plugin-1",
            &BTreeMap::from([("API_TOKEN".into(), "plaintext".into())]),
        )
        .is_err());
    }

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

    #[test]
    fn skill_only_package_receives_a_non_executable_host_record() {
        let directory = tempfile::tempdir().unwrap();
        let installation_root = directory.path().join("installed");
        fs::create_dir_all(&installation_root).unwrap();
        fs::write(installation_root.join("chatos.plugin.json"), "{}").unwrap();
        let manifest = parse_plugin_manifest(
            r#"{
                "schemaVersion": 3,
                "name": "skill-only",
                "version": "1.0.0",
                "description": "Skill only",
                "author": { "name": "Local" },
                "skills": ["./skills/helper"],
                "mcpServers": {},
                "interface": {
                    "displayName": "Skill Only",
                    "shortDescription": "Skill only",
                    "longDescription": "Skill only",
                    "developerName": "Local",
                    "category": "Developer Tools"
                },
                "permissions": []
            }"#,
        )
        .unwrap();
        let package = crate::plugin_installer::CommittedPluginPackage {
            plugin_root: directory.path().to_path_buf(),
            installation_root: installation_root.clone(),
            manifest,
            artifact_sha256: "a".repeat(64),
            backup_root: None,
        };

        let specs =
            local_installation_specs("owner-1", "plugin-1", "release-1", &package, &[]).unwrap();

        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].server_id, "skills-only");
        assert_eq!(specs[0].component_id, "skills-only");
        assert!(specs[0].args.is_empty());
        assert!(specs[0].allowed_tools.is_none());
        assert_eq!(
            Path::new(&specs[0].executable_path).file_name(),
            Some(std::ffi::OsStr::new("chatos.plugin.json"))
        );
    }

    #[test]
    fn computer_use_worker_requires_spawn_and_control_for_the_exact_component() {
        let complete = parse_plugin_manifest(
            r#"{
                "schemaVersion": 3,
                "name": "open-computer-use",
                "version": "1.0.0",
                "description": "Computer Use",
                "author": { "name": "ChatOS" },
                "interface": {
                    "displayName": "Computer Use",
                    "shortDescription": "Control Windows",
                    "longDescription": "Control Windows from screenshots",
                    "developerName": "ChatOS",
                    "category": "Productivity"
                },
                "mcpServers": {
                    "computer-use": { "type": "stdio", "bin": "computer-use" }
                },
                "permissions": [
                    { "permission": "process.spawn", "required": true, "components": ["computer-use"] },
                    { "permission": "computer.control", "required": true, "components": ["computer-use"] }
                ]
            }"#,
        )
        .unwrap();
        assert!(required_component_permission(
            &complete,
            "process.spawn",
            "computer-use"
        ));
        assert!(required_component_permission(
            &complete,
            "computer.control",
            "computer-use"
        ));
        assert!(!required_component_permission(
            &complete,
            "computer.control",
            "other-component"
        ));
    }
}
