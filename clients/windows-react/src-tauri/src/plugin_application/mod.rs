use chatos_local_agent_protocol::{
    GetPluginInstallationCommand, HostCommand, HostResult, ListPluginInstallationsCommand,
    LocalPluginInstallationRecord, LocalPluginInstallationSummary,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::File,
    future::Future,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};
use tauri::State;

mod runtime;
use crate::agent_plugin_runtime::{
    load_agent_plugin_skill_snapshot, AgentPluginRuntime, AgentPluginSkillSnapshot,
};
use crate::plugin_credentials::{
    environment_secret_references, platform_plugin_credential_store, secret_name, secret_reference,
    validate_secret, PluginCredentialStore,
};
use crate::plugin_integrity::{requires_installation_integrity, verify_installation_integrity};
#[cfg(target_os = "windows")]
use crate::windows_appcontainer::WorkspaceLeaseManager;
use runtime::{
    PluginApplicationBridgeRequest, PluginApplicationLaunchRequest, PluginApplicationRuntime,
    PresentedPluginApplicationBridgeResponse, PresentedPluginApplicationLaunch,
};

const MAXIMUM_INSTALLATIONS: usize = 1_000;
const MAXIMUM_MANIFEST_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_UI_CONTRIBUTIONS: usize = 64;
const MAXIMUM_UI_ASSETS: usize = 256;
const MAXIMUM_BRIDGE_CAPABILITIES: usize = 64;
const MAXIMUM_MCP_SERVERS: usize = 64;
const SKILLS_ONLY_COMPONENT_KEY: &str = "skills-only";

type MetadataFuture<'a> = Pin<Box<dyn Future<Output = Result<HostResult, String>> + Send + 'a>>;

pub(crate) trait PluginInstallationMetadataClient: Send + Sync {
    fn execute(&self, command: HostCommand) -> MetadataFuture<'_>;
}

impl PluginInstallationMetadataClient for crate::host_lifecycle::HostManager {
    fn execute(&self, command: HostCommand) -> MetadataFuture<'_> {
        Box::pin(crate::host_lifecycle::HostManager::execute(self, command))
    }
}

#[derive(Clone)]
pub(crate) struct PluginApplicationManager {
    plugin_root: PathBuf,
    credentials: Arc<dyn PluginCredentialStore>,
    runtime: PluginApplicationRuntime,
    pub(crate) agent_runtime: AgentPluginRuntime,
    #[cfg(target_os = "windows")]
    workspace_leases: Arc<WorkspaceLeaseManager>,
}

impl PluginApplicationManager {
    #[cfg(test)]
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self::try_new(app_data_directory)
            .unwrap_or_else(|error| panic!("Plugin runtime initialization failed: {error}"))
    }

    pub(crate) fn try_new(app_data_directory: PathBuf) -> Result<Self, String> {
        Self::try_with_credential_store(app_data_directory, platform_plugin_credential_store())
    }

    #[cfg(test)]
    fn with_credential_store(
        app_data_directory: PathBuf,
        credentials: Arc<dyn PluginCredentialStore>,
    ) -> Self {
        Self::try_with_credential_store(app_data_directory, credentials)
            .unwrap_or_else(|error| panic!("Plugin runtime initialization failed: {error}"))
    }

    fn try_with_credential_store(
        app_data_directory: PathBuf,
        credentials: Arc<dyn PluginCredentialStore>,
    ) -> Result<Self, String> {
        let plugin_root = app_data_directory.join("NativeConnector").join("Plugins");
        #[cfg(target_os = "windows")]
        let workspace_leases = Arc::new(WorkspaceLeaseManager::open(&app_data_directory)?);
        Ok(Self {
            plugin_root: plugin_root.clone(),
            credentials,
            runtime: PluginApplicationRuntime::new(
                app_data_directory
                    .join("NativeConnector")
                    .join("PluginRuntime"),
            ),
            agent_runtime: AgentPluginRuntime::new(
                plugin_root,
                app_data_directory
                    .join("NativeConnector")
                    .join("PluginRuntime")
                    .join("InvocationJournal"),
            ),
            #[cfg(target_os = "windows")]
            workspace_leases,
        })
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn workspace_leases(&self) -> &WorkspaceLeaseManager {
        &self.workspace_leases
    }

    pub(crate) async fn reset(&self) {
        self.runtime.reset().await;
        self.agent_runtime.reset().await;
    }

    pub(crate) async fn resolve_agent_plugin(
        &self,
        client: &dyn PluginInstallationMetadataClient,
        owner_user_id: &str,
        expected: &TrustedAgentPluginOption,
    ) -> Result<ResolvedAgentPluginExecution, String> {
        crate::host_lifecycle::validate_owner(owner_user_id)?;
        let record = get_installation(client, owner_user_id, &expected.installation_id).await?;
        record.spec.validate().map_err(|_| plugin_unavailable())?;
        if record.spec.owner_user_id != owner_user_id
            || !record.spec.enabled
            || record.spec.plugin_id != expected.plugin_id
            || record.spec.server_id != expected.component_key
            || record.spec.component_revision != expected.component_revision
            || record.spec.release_id != expected.release_id
            || record.spec.release_digest != expected.release_digest
        {
            return Err(plugin_unavailable());
        }
        let plugin_root = confined_plugin_root(&self.plugin_root)?;
        let (installation_root, manifest) = load_manifest(&plugin_root, &record)?;
        let skill_snapshot = load_agent_plugin_skill_snapshot(
            &installation_root,
            &manifest.name,
            manifest.skills.iter().map(PathReference::path),
        )?;
        let projected = project_agent_plugin(&record, &manifest)?;
        if projected.installation_id != expected.installation_id
            || projected.plugin_id != expected.plugin_id
            || projected.component_key != expected.component_key
            || projected.component_revision != expected.component_revision
            || projected.release_id != expected.release_id
            || projected.release_digest != expected.release_digest
            || projected.manifest_version != expected.manifest_version
            || projected.display_name != expected.display_name
        {
            return Err(plugin_unavailable());
        }
        let server = manifest.mcp_servers.get(&expected.component_key);
        if server.is_some_and(|server| record.spec.args != server.args)
            || (server.is_none()
                && (expected.component_key != SKILLS_ONLY_COMPONENT_KEY
                    || !record.spec.args.is_empty()))
        {
            return Err(plugin_unavailable());
        }
        let working_directory = match record.spec.working_directory.as_deref() {
            Some(path) => {
                let path = PathBuf::from(path)
                    .canonicalize()
                    .map_err(|_| plugin_unavailable())?;
                if !path.is_dir()
                    || !path.starts_with(&installation_root)
                    || !path.starts_with(&plugin_root)
                {
                    return Err(plugin_unavailable());
                }
                Some(path)
            }
            None => None,
        };
        let environment = match server {
            Some(server) => self.resolve_environment(owner_user_id, &record, &server.env)?,
            None => BTreeMap::new(),
        };
        Ok(ResolvedAgentPluginExecution {
            identity: projected,
            installation_root,
            executable: server.map(|_| PathBuf::from(&record.spec.executable_path)),
            args: record.spec.args,
            working_directory,
            environment,
            allowed_tools: record.spec.allowed_tools,
            skill_snapshot,
        })
    }

    pub(crate) async fn list_agent_plugins(
        &self,
        client: &dyn PluginInstallationMetadataClient,
        owner_user_id: &str,
    ) -> Result<Vec<TrustedAgentPluginOption>, String> {
        crate::host_lifecycle::validate_owner(owner_user_id)?;
        let summaries = list_installation_summaries(client, owner_user_id).await?;
        let mut latest_by_component =
            BTreeMap::<(String, String), LocalPluginInstallationSummary>::new();
        for summary in summaries {
            validate_summary(&summary, owner_user_id)?;
            if !summary.enabled {
                continue;
            }
            let key = (summary.plugin_id.clone(), summary.server_id.clone());
            match latest_by_component.get(&key) {
                Some(existing)
                    if (
                        existing.updated_at_unix_ms,
                        existing.installation_id.as_str(),
                    ) >= (summary.updated_at_unix_ms, summary.installation_id.as_str()) => {}
                _ => {
                    latest_by_component.insert(key, summary);
                }
            }
        }

        let plugin_root = confined_plugin_root(&self.plugin_root)?;
        let mut options = BTreeMap::new();
        for summary in latest_by_component.into_values() {
            let record = get_installation(client, owner_user_id, &summary.installation_id).await?;
            if validate_record(&record, &summary, owner_user_id).is_err() {
                continue;
            }
            let Ok((installation_root, manifest)) = load_manifest(&plugin_root, &record) else {
                continue;
            };
            if load_agent_plugin_skill_snapshot(
                &installation_root,
                &manifest.name,
                manifest.skills.iter().map(PathReference::path),
            )
            .is_err()
            {
                continue;
            }
            let Ok(option) = project_agent_plugin(&record, &manifest) else {
                continue;
            };
            let server = manifest.mcp_servers.get(&option.component_key);
            if server.is_some_and(|server| record.spec.args != server.args)
                || (server.is_none()
                    && (option.component_key != SKILLS_ONLY_COMPONENT_KEY
                        || !record.spec.args.is_empty()))
            {
                continue;
            }
            if server.is_some_and(|server| {
                self.resolve_environment(owner_user_id, &record, &server.env)
                    .is_err()
            }) {
                continue;
            }
            options.insert(option.selection_id(), option);
        }
        let mut options = options.into_values().collect::<Vec<_>>();
        options.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.selection_id().cmp(&right.selection_id()))
        });
        Ok(options)
    }

    fn resolve_environment(
        &self,
        owner_user_id: &str,
        record: &LocalPluginInstallationRecord,
        environment: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        let expected =
            environment_secret_references(owner_user_id, &record.spec.plugin_id, environment)
                .map_err(|_| plugin_unavailable())?;
        if record.spec.environment_secret_refs != expected {
            return Err(plugin_unavailable());
        }
        environment
            .iter()
            .map(|(variable, template)| {
                let reference = expected.get(variable).ok_or_else(plugin_unavailable)?;
                let value = self
                    .credentials
                    .load(reference)
                    .map_err(|_| plugin_unavailable())?
                    .ok_or_else(plugin_unavailable)?;
                validate_secret(&value).map_err(|_| plugin_unavailable())?;
                secret_name(template).map_err(|_| plugin_unavailable())?;
                Ok((variable.clone(), value))
            })
            .collect()
    }

    pub(crate) fn credential_is_configured(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        name: &str,
    ) -> Result<bool, String> {
        let reference = secret_reference(owner_user_id, plugin_id, name)?;
        Ok(self.credentials.load(&reference)?.is_some())
    }

    pub(crate) fn update_credentials(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        values: &BTreeMap<String, Option<String>>,
    ) -> Result<(), String> {
        let mut previous = Vec::new();
        for (name, value) in values {
            let reference = secret_reference(owner_user_id, plugin_id, name)?;
            let old = self.credentials.load(&reference)?;
            if value.is_none() && old.is_none() {
                return Err(format!("Plugin 凭据 {name} 尚未配置。"));
            }
            if let Some(value) = value {
                validate_secret(value)?;
            }
            previous.push((reference, old, value.as_deref()));
        }
        for (applied, (reference, _, value)) in previous.iter().enumerate() {
            let result = value.map_or(Ok(()), |value| self.credentials.save(reference, value));
            if let Err(error) = result {
                let mut rollback_ok = true;
                for (reference, old, _) in previous[..applied].iter().rev() {
                    rollback_ok &= match old {
                        Some(value) => self.credentials.save(reference, value).is_ok(),
                        None => self.credentials.delete(reference).is_ok(),
                    };
                }
                return Err(if rollback_ok {
                    error
                } else {
                    "Plugin 凭据保存失败，且系统凭据回滚未完全成功。".into()
                });
            }
        }
        Ok(())
    }

    pub(crate) fn delete_credentials(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        names: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, Option<String>>, String> {
        let mut previous = Vec::new();
        for name in names {
            let reference = secret_reference(owner_user_id, plugin_id, name)?;
            previous.push((
                name.clone(),
                reference.clone(),
                self.credentials.load(&reference)?,
            ));
        }
        for (index, (_, reference, _)) in previous.iter().enumerate() {
            if self.credentials.delete(reference).is_err() {
                let mut rollback_ok = true;
                for (_, reference, old) in previous[..index].iter().rev() {
                    if let Some(value) = old {
                        rollback_ok &= self.credentials.save(reference, value).is_ok();
                    }
                }
                return Err(if rollback_ok {
                    "无法从系统凭据存储删除 Plugin 凭据。".into()
                } else {
                    "Plugin 凭据删除失败，且系统凭据回滚未完全成功。".into()
                });
            }
        }
        Ok(previous
            .into_iter()
            .filter_map(|(name, _, value)| value.map(|value| (name, Some(value))))
            .collect())
    }

    async fn list(
        &self,
        client: &dyn PluginInstallationMetadataClient,
        owner_user_id: &str,
    ) -> Result<Vec<PresentedPluginApplication>, String> {
        crate::host_lifecycle::validate_owner(owner_user_id)?;
        let summaries = list_installation_summaries(client, owner_user_id).await?;
        let mut latest_by_plugin = BTreeMap::<String, LocalPluginInstallationSummary>::new();
        for summary in summaries {
            validate_summary(&summary, owner_user_id)?;
            if summary.enabled {
                latest_by_plugin
                    .entry(summary.plugin_id.clone())
                    .or_insert(summary);
            }
        }

        let plugin_root = confined_plugin_root(&self.plugin_root)?;
        let mut applications = BTreeMap::new();
        for summary in latest_by_plugin.into_values() {
            let record = get_installation(client, owner_user_id, &summary.installation_id).await?;
            if validate_record(&record, &summary, owner_user_id).is_err() {
                continue;
            }
            let Ok((installation_root, manifest)) = load_manifest(&plugin_root, &record) else {
                continue;
            };
            for application in
                project_applications(&plugin_root, &installation_root, &record, &manifest)
            {
                applications.insert(application.id(), application);
            }
        }
        let mut applications = applications.into_values().collect::<Vec<_>>();
        applications.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.id().cmp(&right.id()))
        });
        Ok(applications)
    }

    async fn resolve(
        &self,
        client: &dyn PluginInstallationMetadataClient,
        owner_user_id: &str,
        plugin_id: &str,
        component_key: &str,
    ) -> Result<ResolvedPluginApplication, String> {
        crate::host_lifecycle::validate_owner(owner_user_id)?;
        validate_text(plugin_id, 1_024)?;
        validate_text(component_key, 1_024)?;
        let summaries = list_installation_summaries(client, owner_user_id).await?;
        let summary = summaries
            .into_iter()
            .filter(|summary| summary.enabled && summary.plugin_id == plugin_id)
            .map(|summary| {
                validate_summary(&summary, owner_user_id)?;
                Ok(summary)
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .max_by(|left, right| {
                left.updated_at_unix_ms
                    .cmp(&right.updated_at_unix_ms)
                    .then_with(|| left.installation_id.cmp(&right.installation_id))
            })
            .ok_or_else(plugin_unavailable)?;
        let record = get_installation(client, owner_user_id, &summary.installation_id).await?;
        validate_record(&record, &summary, owner_user_id)?;
        let plugin_root = confined_plugin_root(&self.plugin_root)?;
        let (installation_root, manifest) = load_manifest(&plugin_root, &record)?;
        let contribution = manifest
            .ui
            .iter()
            .find(|contribution| {
                contribution.surface.as_deref() == Some("workbench")
                    && contribution.component_key == component_key
            })
            .cloned()
            .ok_or_else(plugin_unavailable)?;
        let application = project_application(
            &plugin_root,
            &installation_root,
            &record,
            &manifest,
            &contribution,
        )?;
        Ok(ResolvedPluginApplication {
            record,
            installation_root,
            manifest,
            contribution,
            application,
        })
    }
}

#[derive(Clone)]
struct ResolvedPluginApplication {
    record: LocalPluginInstallationRecord,
    installation_root: PathBuf,
    manifest: PluginManifest,
    contribution: PluginUiContribution,
    application: PresentedPluginApplication,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedPluginApplication {
    plugin_id: String,
    component_key: String,
    display_name: String,
    description: String,
    brand_color: Option<String>,
    requires_local_runtime: bool,
    context_scope: Option<String>,
    missing_context: Option<String>,
    bridge_capabilities: Vec<String>,
    release_id: String,
    release_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrustedAgentPluginOption {
    pub(crate) installation_id: String,
    pub(crate) plugin_id: String,
    pub(crate) component_key: String,
    pub(crate) component_revision: String,
    pub(crate) release_id: String,
    pub(crate) release_digest: String,
    pub(crate) manifest_version: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
}

#[derive(Clone)]
pub(crate) struct ResolvedAgentPluginExecution {
    pub(crate) identity: TrustedAgentPluginOption,
    pub(crate) installation_root: PathBuf,
    pub(crate) executable: Option<PathBuf>,
    pub(crate) args: Vec<String>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) allowed_tools: Option<Vec<String>>,
    pub(crate) skill_snapshot: AgentPluginSkillSnapshot,
}

impl fmt::Debug for ResolvedAgentPluginExecution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedAgentPluginExecution")
            .field("identity", &self.identity)
            .field("installation_root", &self.installation_root)
            .field("executable", &self.executable)
            .field("args", &self.args)
            .field("working_directory", &self.working_directory)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("allowed_tools", &self.allowed_tools)
            .field("skill_snapshot", &self.skill_snapshot)
            .finish()
    }
}

impl TrustedAgentPluginOption {
    pub(crate) fn selection_id(&self) -> String {
        format!("{}\0{}", self.plugin_id, self.component_key)
    }

    pub(crate) fn frozen_identity(&self) -> String {
        [
            self.installation_id.as_str(),
            self.plugin_id.as_str(),
            self.component_key.as_str(),
            self.component_revision.as_str(),
            self.release_id.as_str(),
            self.release_digest.as_str(),
            self.manifest_version.as_str(),
        ]
        .join("\0")
    }
}

impl PresentedPluginApplication {
    fn id(&self) -> String {
        format!("{}:{}", self.plugin_id, self.component_key)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    schema_version: u32,
    name: String,
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    skills: Vec<PathReference>,
    #[serde(default)]
    mcp_servers: BTreeMap<String, PluginMcpServer>,
    #[serde(default)]
    ui: Vec<PluginUiContribution>,
    interface: Option<PluginInterface>,
    #[serde(default)]
    permissions: Vec<PluginPermission>,
    runtime_context: Option<PluginRuntimeContext>,
    dependencies: Option<PluginDependencies>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginMcpServer {
    #[serde(rename = "type")]
    kind: String,
    bin: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    agent: Option<PluginAgentPresentation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginAgentPresentation {
    display_name: String,
    description: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginUiContribution {
    component_key: String,
    source: PathReference,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    bridge_capabilities: Vec<String>,
    #[serde(default)]
    assets: Vec<PathReference>,
    runtime: Option<PluginUiRuntime>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginUiRuntime {
    #[serde(rename = "type")]
    kind: String,
    bin: String,
    #[serde(default)]
    args: Vec<String>,
    health_path: Option<String>,
    launch_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum PathReference {
    Text(String),
    Object { path: String },
}

impl PathReference {
    fn path(&self) -> &str {
        match self {
            Self::Text(path) | Self::Object { path } => path,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginInterface {
    display_name: Option<String>,
    brand_color: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PluginPermission {
    permission: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    components: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginRuntimeContext {
    scope: String,
    #[serde(default)]
    components: Vec<String>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    optional: Vec<String>,
    #[serde(default = "default_storage_isolation")]
    storage_isolation: String,
    #[serde(default = "default_missing_context")]
    missing_context: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginDependencies {
    #[serde(default)]
    supported_platforms: Vec<String>,
}

#[tauri::command]
pub async fn list_plugin_applications(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    host: State<'_, crate::host_lifecycle::HostManager>,
    manager: State<'_, PluginApplicationManager>,
    owner_user_id: String,
) -> Result<Vec<PresentedPluginApplication>, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager.list(&*host, &owner_user_id).await
}

#[tauri::command]
pub async fn launch_plugin_application(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    host: State<'_, crate::host_lifecycle::HostManager>,
    manager: State<'_, PluginApplicationManager>,
    request: PluginApplicationLaunchRequest,
) -> Result<PresentedPluginApplicationLaunch, String> {
    let epoch = manager.runtime.epoch();
    authentication
        .access_token_for_owner(&request.owner_user_id)
        .await?;
    let resolved = manager
        .resolve(
            &*host,
            &request.owner_user_id,
            &request.plugin_id,
            &request.component_key,
        )
        .await?;
    manager.runtime.launch(epoch, request, resolved).await
}

#[tauri::command]
pub async fn discard_plugin_application_presentation(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, PluginApplicationManager>,
    owner_user_id: String,
    presentation_id: String,
    generation: u64,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .runtime
        .discard_presentation(&owner_user_id, &presentation_id, generation)
        .await
}

#[tauri::command]
pub async fn handle_plugin_application_bridge_request(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, PluginApplicationManager>,
    request: PluginApplicationBridgeRequest,
) -> Result<PresentedPluginApplicationBridgeResponse, String> {
    authentication
        .access_token_for_owner(&request.owner_user_id)
        .await?;
    manager.runtime.bridge_request(request).await
}

async fn list_installation_summaries(
    client: &dyn PluginInstallationMetadataClient,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginInstallationSummary>, String> {
    let mut installations = Vec::new();
    let mut before_updated_at_unix_ms = None;
    let mut before_installation_id = None;
    let mut observed_cursors = BTreeSet::new();
    loop {
        let result = client
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
            return Err("已安装 Plugin 数量超过本机应用目录上限。".into());
        }
        installations.extend(page.installations);
        match (
            page.next_before_updated_at_unix_ms,
            page.next_before_installation_id,
        ) {
            (Some(timestamp), Some(installation_id)) => {
                if !observed_cursors.insert((timestamp, installation_id.clone())) {
                    return Err("Host 返回了重复的 Plugin 安装分页游标。".into());
                }
                before_updated_at_unix_ms = Some(timestamp);
                before_installation_id = Some(installation_id);
            }
            (None, None) => return Ok(installations),
            _ => return Err("Host 返回了无效的 Plugin 安装分页游标。".into()),
        }
    }
}

async fn get_installation(
    client: &dyn PluginInstallationMetadataClient,
    owner_user_id: &str,
    installation_id: &str,
) -> Result<LocalPluginInstallationRecord, String> {
    let result = client
        .execute(HostCommand::GetPluginInstallation(
            GetPluginInstallationCommand {
                owner_user_id: owner_user_id.to_owned(),
                installation_id: installation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::PluginInstallation { installation } = result else {
        return Err(metadata_response_error());
    };
    Ok(installation)
}

fn validate_summary(
    summary: &LocalPluginInstallationSummary,
    owner_user_id: &str,
) -> Result<(), String> {
    for value in [
        &summary.installation_id,
        &summary.plugin_id,
        &summary.release_id,
        &summary.component_id,
        &summary.component_revision,
        &summary.server_id,
    ] {
        validate_text(value, 1_024)?;
    }
    if summary.owner_user_id != owner_user_id
        || summary.version == 0
        || summary.created_at_unix_ms < 0
        || summary.updated_at_unix_ms < summary.created_at_unix_ms
    {
        return Err(metadata_response_error());
    }
    Ok(())
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
        || record.version != summary.version
        || record.created_at_unix_ms != summary.created_at_unix_ms
        || record.updated_at_unix_ms != summary.updated_at_unix_ms
        || !record.spec.enabled
        || !valid_release_digest(&record.spec.release_digest)
    {
        return Err(metadata_response_error());
    }
    Ok(())
}

fn confined_plugin_root(requested: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(requested).map_err(|_| plugin_catalog_error())?;
    let root = requested
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !root.is_dir() {
        return Err(plugin_catalog_error());
    }
    Ok(root)
}

fn load_manifest(
    plugin_root: &Path,
    record: &LocalPluginInstallationRecord,
) -> Result<(PathBuf, PluginManifest), String> {
    let executable = PathBuf::from(&record.spec.executable_path)
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !executable.is_file() || !executable.starts_with(plugin_root) {
        return Err(plugin_catalog_error());
    }
    let mut candidate = executable.parent().ok_or_else(plugin_catalog_error)?;
    let installation_root = loop {
        if !candidate.starts_with(plugin_root) {
            return Err(plugin_catalog_error());
        }
        let manifest = candidate.join("chatos.plugin.json");
        if manifest.is_file() {
            break candidate.to_path_buf();
        }
        if candidate == plugin_root {
            return Err(plugin_catalog_error());
        }
        candidate = candidate.parent().ok_or_else(plugin_catalog_error)?;
    };
    let manifest_path = installation_root.join("chatos.plugin.json");
    if requires_installation_integrity(&record.spec.component_revision) {
        verify_installation_integrity(plugin_root, &installation_root, &record.spec.release_digest)
            .map_err(|_| plugin_catalog_error())?;
    }
    let canonical_manifest = manifest_path
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !canonical_manifest.starts_with(&installation_root) {
        return Err(plugin_catalog_error());
    }
    let metadata = canonical_manifest
        .metadata()
        .map_err(|_| plugin_catalog_error())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(&canonical_manifest)
        .and_then(|file| {
            file.take(MAXIMUM_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| plugin_catalog_error())?;
    if bytes.len() as u64 > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let manifest =
        serde_json::from_slice::<PluginManifest>(&bytes).map_err(|_| plugin_catalog_error())?;
    validate_manifest(&manifest)?;
    Ok((installation_root, manifest))
}

fn validate_manifest(manifest: &PluginManifest) -> Result<(), String> {
    if manifest.schema_version != 3
        || manifest.ui.len() > MAXIMUM_UI_CONTRIBUTIONS
        || manifest.mcp_servers.len() > MAXIMUM_MCP_SERVERS
        || !valid_text(&manifest.name, 512)
        || !valid_text(&manifest.version, 128)
        || !valid_optional_text(&manifest.description, 16 * 1_024)
    {
        return Err(plugin_catalog_error());
    }
    if let Some(platforms) = manifest
        .dependencies
        .as_ref()
        .map(|dependencies| &dependencies.supported_platforms)
    {
        if platforms.len() > 32
            || (!platforms.is_empty()
                && !platforms
                    .iter()
                    .any(|platform| platform.eq_ignore_ascii_case("windows")))
        {
            return Err(plugin_catalog_error());
        }
    }
    Ok(())
}

fn project_agent_plugin(
    record: &LocalPluginInstallationRecord,
    manifest: &PluginManifest,
) -> Result<TrustedAgentPluginOption, String> {
    let component_key = &record.spec.server_id;
    if component_key == SKILLS_ONLY_COMPONENT_KEY {
        if manifest.skills.is_empty()
            || !manifest.mcp_servers.is_empty()
            || !record.spec.args.is_empty()
            || !record.spec.environment_secret_refs.is_empty()
            || record.spec.tool_prefix.is_some()
            || record.spec.allowed_tools.is_some()
            || Path::new(&record.spec.executable_path)
                .file_name()
                .and_then(|value| value.to_str())
                != Some("chatos.plugin.json")
        {
            return Err(plugin_catalog_error());
        }
        let display_name = manifest
            .interface
            .as_ref()
            .and_then(|interface| interface.display_name.as_deref())
            .unwrap_or(&manifest.name)
            .trim()
            .to_owned();
        let description = manifest.description.trim().to_owned();
        validate_text(&display_name, 240)?;
        if !valid_optional_text(&description, 16 * 1_024) {
            return Err(plugin_catalog_error());
        }
        return Ok(TrustedAgentPluginOption {
            installation_id: record.spec.installation_id.clone(),
            plugin_id: record.spec.plugin_id.clone(),
            component_key: component_key.clone(),
            component_revision: record.spec.component_revision.clone(),
            release_id: record.spec.release_id.clone(),
            release_digest: record.spec.release_digest.clone(),
            manifest_version: manifest.version.clone(),
            display_name,
            description,
        });
    }
    let server = manifest
        .mcp_servers
        .get(component_key)
        .ok_or_else(plugin_catalog_error)?;
    if server.kind != "stdio"
        || !valid_package_executable(&server.bin)
        || !executable_matches_manifest_bin(&record.spec.executable_path, &server.bin)
        || server.args.len() > 256
        || server
            .args
            .iter()
            .any(|argument| !valid_optional_text(argument, 8_192))
        || server.env.len() > 256
        || server
            .env
            .iter()
            .any(|(key, value)| !valid_environment_name(key) || !valid_optional_text(value, 8_192))
    {
        return Err(plugin_catalog_error());
    }
    let has_spawn_permission = manifest.permissions.iter().any(|permission| {
        permission.permission == "process.spawn"
            && permission.required
            && (permission.components.is_empty() || permission.components.contains(component_key))
    });
    if !has_spawn_permission {
        return Err(plugin_catalog_error());
    }
    let display_name = server
        .agent
        .as_ref()
        .map(|presentation| presentation.display_name.as_str())
        .or_else(|| {
            manifest
                .interface
                .as_ref()
                .and_then(|interface| interface.display_name.as_deref())
        })
        .unwrap_or(&manifest.name)
        .trim()
        .to_owned();
    let description = server
        .agent
        .as_ref()
        .map(|presentation| presentation.description.as_str())
        .unwrap_or(&manifest.description)
        .trim()
        .to_owned();
    validate_text(&display_name, 240)?;
    if !valid_optional_text(&description, 16 * 1_024) {
        return Err(plugin_catalog_error());
    }
    Ok(TrustedAgentPluginOption {
        installation_id: record.spec.installation_id.clone(),
        plugin_id: record.spec.plugin_id.clone(),
        component_key: component_key.clone(),
        component_revision: record.spec.component_revision.clone(),
        release_id: record.spec.release_id.clone(),
        release_digest: record.spec.release_digest.clone(),
        manifest_version: manifest.version.clone(),
        display_name,
        description,
    })
}

fn executable_matches_manifest_bin(executable_path: &str, manifest_bin: &str) -> bool {
    let Some(file_name) = Path::new(executable_path)
        .file_name()
        .and_then(|value| value.to_str())
    else {
        return false;
    };
    file_name == manifest_bin
        || file_name
            .strip_suffix(".exe")
            .is_some_and(|stem| stem == manifest_bin)
}

fn project_applications(
    plugin_root: &Path,
    installation_root: &Path,
    record: &LocalPluginInstallationRecord,
    manifest: &PluginManifest,
) -> Vec<PresentedPluginApplication> {
    manifest
        .ui
        .iter()
        .filter(|contribution| contribution.surface.as_deref() == Some("workbench"))
        .filter_map(|contribution| {
            project_application(
                plugin_root,
                installation_root,
                record,
                manifest,
                contribution,
            )
            .ok()
        })
        .collect()
}

fn project_application(
    plugin_root: &Path,
    installation_root: &Path,
    record: &LocalPluginInstallationRecord,
    manifest: &PluginManifest,
    contribution: &PluginUiContribution,
) -> Result<PresentedPluginApplication, String> {
    validate_text(&record.spec.plugin_id, 1_024)?;
    validate_text(&contribution.component_key, 1_024)?;
    let source =
        resolve_installed_file(plugin_root, installation_root, contribution.source.path())?;
    if !source.is_file() {
        return Err(plugin_catalog_error());
    }
    validate_ui_files(plugin_root, installation_root, contribution)?;
    if contribution.bridge_capabilities.len() > MAXIMUM_BRIDGE_CAPABILITIES {
        return Err(plugin_catalog_error());
    }
    let mut bridge_capabilities = BTreeSet::new();
    for capability in &contribution.bridge_capabilities {
        if !valid_capability(capability) || !bridge_capabilities.insert(capability.clone()) {
            return Err(plugin_catalog_error());
        }
    }
    if let Some(runtime) = &contribution.runtime {
        validate_runtime(runtime)?;
        let has_spawn_permission = manifest.permissions.iter().any(|permission| {
            permission.permission == "process.spawn"
                && permission.required
                && (permission.components.is_empty()
                    || permission.components.contains(&contribution.component_key))
        });
        if !has_spawn_permission {
            return Err(plugin_catalog_error());
        }
    }
    let runtime_context = manifest
        .runtime_context
        .as_ref()
        .filter(|context| context.components.contains(&contribution.component_key));
    if let Some(context) = runtime_context {
        validate_runtime_context(context)?;
    }
    let contribution_title = contribution
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let interface_title = manifest
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let display_name = contribution_title
        .or(interface_title)
        .unwrap_or(&manifest.name)
        .to_owned();
    validate_text(&display_name, 1_024)?;
    let description = manifest.description.trim().to_owned();
    if !valid_optional_text(&description, 16 * 1_024) {
        return Err(plugin_catalog_error());
    }
    let brand_color = manifest
        .interface
        .as_ref()
        .and_then(|interface| interface.brand_color.as_deref())
        .filter(|value| valid_brand_color(value))
        .map(str::to_owned);
    Ok(PresentedPluginApplication {
        plugin_id: record.spec.plugin_id.clone(),
        component_key: contribution.component_key.clone(),
        display_name,
        description,
        brand_color,
        requires_local_runtime: contribution.runtime.is_some(),
        context_scope: runtime_context.map(|context| context.scope.clone()),
        missing_context: runtime_context.map(|context| context.missing_context.clone()),
        bridge_capabilities: bridge_capabilities.into_iter().collect(),
        release_id: record.spec.release_id.clone(),
        release_digest: record.spec.release_digest.clone(),
    })
}

fn validate_ui_files(
    plugin_root: &Path,
    installation_root: &Path,
    contribution: &PluginUiContribution,
) -> Result<(), String> {
    let source = contribution.source.path();
    if !source.starts_with("./ui/")
        || !source
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("html"))
        || contribution.assets.len() > MAXIMUM_UI_ASSETS
    {
        return Err(plugin_catalog_error());
    }
    let mut observed = BTreeSet::new();
    for asset in &contribution.assets {
        let path = asset.path();
        let extension = path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let supported = extension.as_deref().is_some_and(|extension| {
            matches!(
                extension,
                "js" | "mjs"
                    | "css"
                    | "json"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "woff"
                    | "woff2"
            )
        });
        if !path.starts_with("./ui/")
            || !supported
            || path == source
            || !observed.insert(path)
            || !resolve_installed_file(plugin_root, installation_root, path)?.is_file()
        {
            return Err(plugin_catalog_error());
        }
    }
    Ok(())
}

fn resolve_installed_file(
    plugin_root: &Path,
    installation_root: &Path,
    relative: &str,
) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.len() > 4_096
        || relative.chars().any(char::is_control)
        || Path::new(relative).is_absolute()
    {
        return Err(plugin_catalog_error());
    }
    let canonical = installation_root
        .join(relative)
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !canonical.starts_with(installation_root) || !canonical.starts_with(plugin_root) {
        return Err(plugin_catalog_error());
    }
    Ok(canonical)
}

fn validate_runtime(runtime: &PluginUiRuntime) -> Result<(), String> {
    if runtime.kind != "local_http"
        || !valid_package_executable(&runtime.bin)
        || runtime.args.len() > 256
        || runtime
            .args
            .iter()
            .any(|argument| !valid_optional_text(argument, 8_192))
        || runtime
            .health_path
            .as_deref()
            .is_some_and(|path| !valid_health_path(path))
        || runtime
            .launch_timeout_ms
            .is_some_and(|timeout| !(100..=120_000).contains(&timeout))
    {
        return Err(plugin_catalog_error());
    }
    Ok(())
}

fn validate_runtime_context(context: &PluginRuntimeContext) -> Result<(), String> {
    let valid_scope = matches!(context.scope.as_str(), "device" | "workspace" | "project");
    let valid_storage = matches!(
        context.storage_isolation.as_str(),
        "plugin" | "workspace" | "project"
    );
    let valid_missing = matches!(context.missing_context.as_str(), "device" | "reject");
    let valid_fields = context
        .required
        .iter()
        .chain(&context.optional)
        .all(|field| {
            matches!(
                field.as_str(),
                "project.id" | "workspace.id" | "workspace.root"
            )
        });
    if !valid_scope
        || !valid_storage
        || !valid_missing
        || context.components.len() > 256
        || context.required.len() > 16
        || context.optional.len() > 16
        || !valid_fields
    {
        return Err(plugin_catalog_error());
    }
    Ok(())
}

fn valid_release_digest(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_package_executable(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

fn valid_health_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 2_048
        && !value.starts_with("//")
        && !value.contains("..")
        && !value.contains('?')
        && !value.contains('#')
        && !value.chars().any(char::is_control)
}

fn valid_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_environment_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains('=')
        && !value.chars().any(char::is_control)
}

fn valid_brand_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_text(value: &str, maximum: usize) -> Result<(), String> {
    if valid_text(value, maximum) {
        Ok(())
    } else {
        Err(plugin_catalog_error())
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
}

fn valid_optional_text(value: &str, maximum: usize) -> bool {
    value.len() <= maximum && !value.chars().any(char::is_control)
}

fn default_storage_isolation() -> String {
    "plugin".into()
}

fn default_missing_context() -> String {
    "reject".into()
}

fn metadata_response_error() -> String {
    "Host 返回了无效的 Plugin 安装记录。".into()
}

fn plugin_catalog_error() -> String {
    "已安装 Plugin 的应用清单未通过 Windows 安全校验。".into()
}

fn plugin_unavailable() -> String {
    "Plugin 应用不可用，请刷新后重试。".into()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
