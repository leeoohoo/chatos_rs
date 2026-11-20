use chatos_local_agent_protocol::{LocalPluginInstallationRecord, LocalPluginInstallationSummary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    sync::Arc,
};

#[cfg(test)]
use std::io::Cursor;
#[cfg(test)]
use tauri::http;

mod commands;
mod icons;
mod manifest;
mod projection;
mod runtime;
use crate::agent_plugin_runtime::{
    load_agent_plugin_skill_snapshot, AgentPluginRuntime, AgentPluginSkillSnapshot,
};
use crate::host_command_client::HostCommandClient;
#[cfg(test)]
use crate::host_command_client::HostCommandFuture;
use crate::machine_resource_governor::MachineResourceGovernor;
use crate::plugin_credentials::{
    environment_secret_references, platform_plugin_credential_store, secret_name, secret_reference,
    validate_secret, PluginCredentialStore,
};
use crate::plugin_installation_metadata::{get_installation, list_installation_summaries};
#[cfg(target_os = "windows")]
use crate::windows_appcontainer::WorkspaceLeaseManager;
#[cfg(test)]
use chatos_local_agent_protocol::{HostCommand, HostResult};
pub use commands::{
    discard_plugin_application_presentation, handle_plugin_application_bridge_request,
    launch_plugin_application, list_plugin_applications,
};
#[cfg(test)]
use icons::{normalize_plugin_application_svg, MAXIMUM_ICON_PNG_BYTES};
pub(crate) use icons::{plugin_application_icon_protocol, PluginApplicationIconStore};
use manifest::{
    confined_plugin_root, load_manifest, valid_health_path, validate_record, validate_summary,
    validate_text,
};
use projection::{
    is_non_executable_component, project_agent_plugin, project_application, project_applications,
    resolve_installed_file,
};
use runtime::PluginApplicationRuntime;

const MAXIMUM_MANIFEST_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_UI_CONTRIBUTIONS: usize = 64;
const MAXIMUM_UI_ASSETS: usize = 256;
const MAXIMUM_BRIDGE_CAPABILITIES: usize = 64;
const MAXIMUM_MCP_SERVERS: usize = 64;
const SKILLS_ONLY_COMPONENT_KEY: &str = "skills-only";
const HOOKS_ONLY_COMPONENT_KEY: &str = "hooks-only";

#[derive(Clone)]
pub(crate) struct PluginApplicationManager {
    plugin_root: PathBuf,
    credentials: Arc<dyn PluginCredentialStore>,
    icons: PluginApplicationIconStore,
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

    #[cfg(test)]
    pub(crate) fn try_new(app_data_directory: PathBuf) -> Result<Self, String> {
        Self::try_with_credential_store_and_resources(
            app_data_directory,
            platform_plugin_credential_store(),
            MachineResourceGovernor::default(),
            PluginApplicationIconStore::default(),
        )
    }

    pub(crate) fn try_new_with_resources_and_icons(
        app_data_directory: PathBuf,
        resources: MachineResourceGovernor,
        icons: PluginApplicationIconStore,
    ) -> Result<Self, String> {
        Self::try_with_credential_store_and_resources(
            app_data_directory,
            platform_plugin_credential_store(),
            resources,
            icons,
        )
    }

    #[cfg(test)]
    fn with_credential_store(
        app_data_directory: PathBuf,
        credentials: Arc<dyn PluginCredentialStore>,
    ) -> Self {
        Self::try_with_credential_store(app_data_directory, credentials)
            .unwrap_or_else(|error| panic!("Plugin runtime initialization failed: {error}"))
    }

    #[cfg(test)]
    fn try_with_credential_store(
        app_data_directory: PathBuf,
        credentials: Arc<dyn PluginCredentialStore>,
    ) -> Result<Self, String> {
        Self::try_with_credential_store_and_resources(
            app_data_directory,
            credentials,
            MachineResourceGovernor::default(),
            PluginApplicationIconStore::default(),
        )
    }

    fn try_with_credential_store_and_resources(
        app_data_directory: PathBuf,
        credentials: Arc<dyn PluginCredentialStore>,
        resources: MachineResourceGovernor,
        icons: PluginApplicationIconStore,
    ) -> Result<Self, String> {
        let plugin_root = app_data_directory.join("NativeConnector").join("Plugins");
        #[cfg(target_os = "windows")]
        let workspace_leases = Arc::new(WorkspaceLeaseManager::open(&app_data_directory)?);
        Ok(Self {
            plugin_root: plugin_root.clone(),
            credentials,
            icons,
            runtime: PluginApplicationRuntime::new_with_resources(
                app_data_directory
                    .join("NativeConnector")
                    .join("PluginRuntime"),
                resources.clone(),
            ),
            agent_runtime: AgentPluginRuntime::new_with_resources(
                plugin_root,
                app_data_directory
                    .join("NativeConnector")
                    .join("PluginRuntime")
                    .join("InvocationJournal"),
                resources,
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
        self.icons.reset();
    }

    pub(crate) async fn resolve_agent_plugin(
        &self,
        client: &dyn HostCommandClient,
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
                && !is_non_executable_component(&expected.component_key)
                && !record.spec.args.is_empty())
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
        let network_policy = if manifest.permissions.iter().any(|permission| {
            permission.permission == "network.client"
                && permission.required
                && (permission.components.is_empty()
                    || permission.components.contains(&expected.component_key))
        }) {
            "controlled"
        } else {
            "disabled"
        };
        Ok(ResolvedAgentPluginExecution {
            identity: projected,
            installation_root,
            executable: server.map(|_| PathBuf::from(&record.spec.executable_path)),
            args: record.spec.args,
            working_directory,
            environment,
            network_policy,
            allowed_tools: record.spec.allowed_tools,
            skill_snapshot,
        })
    }

    pub(crate) async fn list_agent_plugins(
        &self,
        client: &dyn HostCommandClient,
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
                    && !is_non_executable_component(&option.component_key)
                    && !record.spec.args.is_empty())
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
        client: &dyn HostCommandClient,
        owner_user_id: &str,
    ) -> Result<Vec<PresentedPluginApplication>, String> {
        crate::host_lifecycle::validate_owner(owner_user_id)?;
        let icon_generation = self.icons.generation();
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
        let icon_session_token = uuid::Uuid::new_v4().to_string().to_lowercase();
        let mut icons = BTreeMap::new();
        let mut applications = BTreeMap::new();
        for summary in latest_by_plugin.into_values() {
            let record = get_installation(client, owner_user_id, &summary.installation_id).await?;
            if validate_record(&record, &summary, owner_user_id).is_err() {
                continue;
            }
            let Ok((installation_root, manifest)) = load_manifest(&plugin_root, &record) else {
                continue;
            };
            for application in project_applications(
                &plugin_root,
                &installation_root,
                &record,
                &manifest,
                Some(&icon_session_token),
                &mut icons,
            ) {
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
        self.icons
            .publish(icon_generation, icon_session_token, icons)?;
        Ok(applications)
    }

    async fn resolve(
        &self,
        client: &dyn HostCommandClient,
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
            None,
            &mut BTreeMap::new(),
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
    icon_preview_url: Option<String>,
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
    pub(crate) network_policy: &'static str,
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
    hooks: Vec<Value>,
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
    logo: Option<PathReference>,
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
