use super::{
    plugin_catalog_error, plugin_unavailable, valid_health_path, validate_text,
    PresentedPluginApplication, ResolvedPluginApplication,
};
use crate::{
    platform_execution_context::resolve_project_root, terminal_runtime::process::ProcessTermination,
};
use reqwest::{redirect::Policy, Client};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsString,
    fs::{self, File},
    io::{self, Read},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::{Mutex, Notify},
    time::{sleep, timeout, Duration, Instant},
};
use uuid::Uuid;

const MAXIMUM_CONNECTOR_STATE_BYTES: u64 = 16 * 1_024 * 1_024;
const MAXIMUM_PACKAGE_JSON_BYTES: u64 = 1_024 * 1_024;
const MAXIMUM_PRESENTATIONS: usize = 1_024;
const MAXIMUM_RUNTIMES_PER_OWNER: usize = 8;
const DEFAULT_HEALTH_PATH: &str = "/api/health";
const DEFAULT_LAUNCH_TIMEOUT_MS: u64 = 15_000;
const HEALTH_REQUEST_TIMEOUT_MS: u64 = 700;
const HEALTH_RETRY_MS: u64 = 80;
const TERMINATION_GRACE_MS: u64 = 2_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PluginApplicationLaunchRequest {
    pub(super) owner_user_id: String,
    pub(super) presentation_id: String,
    pub(super) generation: u64,
    pub(super) plugin_id: String,
    pub(super) component_key: String,
    pub(super) context: PluginApplicationContextSelection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PluginApplicationContextSelection {
    None,
    Device,
    Project { project_id: String },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PresentedPluginApplicationLaunch {
    session_id: String,
    application: PresentedPluginApplication,
    url: String,
    storage_partition_id: String,
    context: Option<PresentedPluginApplicationContext>,
    release_id: String,
    release_digest: String,
    version: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PresentedPluginApplicationContext {
    project_id: String,
    project_name: String,
}

#[derive(Clone)]
pub(super) struct PluginApplicationRuntime {
    root: PathBuf,
    epoch: Arc<AtomicU64>,
    state: Arc<Mutex<RuntimeState>>,
    health_client: Client,
}

#[derive(Default)]
struct RuntimeState {
    presentations: HashMap<PresentationKey, PresentationState>,
    processes: HashMap<RuntimeKey, Arc<RunningPluginApplication>>,
    launch_locks: HashMap<RuntimeKey, Arc<Mutex<()>>>,
    reservations: HashSet<RuntimeKey>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PresentationKey {
    owner_user_id: String,
    presentation_id: String,
}

struct PresentationState {
    generation: u64,
    active: bool,
    runtime_key: Option<RuntimeKey>,
    session_id: Option<String>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RuntimeKey {
    owner_user_id: String,
    plugin_id: String,
    component_key: String,
    scope_key: String,
}

struct RunningPluginApplication {
    release_id: String,
    release_digest: String,
    version: String,
    base_url: String,
    health_path: String,
    termination: ProcessTermination,
    alive: AtomicBool,
    finished: Notify,
}

#[derive(Clone)]
struct ResolvedHostContext {
    owner_user_id: String,
    device_id: String,
    workspace_id: Option<String>,
    workspace_root: Option<PathBuf>,
    project_id: Option<String>,
    project_name: Option<String>,
}

struct ResolvedRuntimeContext {
    data_directory: PathBuf,
    cache_directory: PathBuf,
    environment: BTreeMap<String, String>,
    scope_key: String,
}

struct PreparedRuntimeProcess {
    executable: PathBuf,
    arguments: Vec<OsString>,
    environment: BTreeMap<String, String>,
    installation_root: PathBuf,
    base_url: String,
    health_path: String,
    launch_timeout_ms: u64,
}

#[derive(Deserialize)]
struct ConnectorState {
    user: Option<ConnectorUser>,
    #[serde(rename = "deviceID")]
    device_id: Option<String>,
}

#[derive(Deserialize)]
struct ConnectorUser {
    id: String,
}

#[derive(Deserialize)]
struct PackageManifest {
    name: String,
    bin: Value,
}

impl PluginApplicationRuntime {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            epoch: Arc::new(AtomicU64::new(1)),
            state: Arc::new(Mutex::new(RuntimeState::default())),
            health_client: Client::builder()
                .redirect(Policy::none())
                .no_proxy()
                .build()
                .expect("static loopback health client configuration"),
        }
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub(super) async fn launch(
        &self,
        expected_epoch: u64,
        request: PluginApplicationLaunchRequest,
        resolved: ResolvedPluginApplication,
    ) -> Result<PresentedPluginApplicationLaunch, String> {
        validate_launch_request(&request)?;
        self.begin_presentation(expected_epoch, &request).await?;
        let root = self.root.clone();
        let owner = request.owner_user_id.clone();
        let selection = request.context.clone();
        let application = resolved.application.clone();
        let host_context = tokio::task::spawn_blocking(move || {
            resolve_host_context(&root, &owner, &application, &selection)
        })
        .await
        .map_err(|_| plugin_runtime_error())??;
        if !self.accepts(expected_epoch, &request).await {
            return Err(stale_launch_error());
        }
        let runtime_context = resolve_runtime_context(&self.root, &resolved, &host_context)?;
        let key = RuntimeKey {
            owner_user_id: request.owner_user_id.clone(),
            plugin_id: request.plugin_id.clone(),
            component_key: request.component_key.clone(),
            scope_key: runtime_context.scope_key.clone(),
        };
        let launch_lock = self.launch_lock(&key).await;
        let _launch_guard = launch_lock.lock().await;
        if !self.accepts(expected_epoch, &request).await {
            return Err(stale_launch_error());
        }

        if let Some(process) = self.current_process(&key).await {
            if process.release_id == resolved.record.spec.release_id
                && process.release_digest == resolved.record.spec.release_digest
                && process.version == resolved.manifest.version
                && process.alive.load(Ordering::SeqCst)
                && self.is_healthy(&process).await
            {
                return self
                    .finish_launch(
                        expected_epoch,
                        &request,
                        key,
                        process,
                        resolved.application,
                        &host_context,
                    )
                    .await;
            }
            self.remove_process_if_same(&key, &process).await;
            process.stop().await;
        }

        self.reserve(&key).await?;
        if !self.accepts(expected_epoch, &request).await {
            self.release_reservation(&key).await;
            return Err(stale_launch_error());
        }
        let prepared = match prepare_runtime_process(&resolved, &runtime_context) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.release_reservation(&key).await;
                return Err(error);
            }
        };
        let launch_timeout = prepared.launch_timeout_ms;
        let process = match spawn_runtime_process(&resolved, prepared).await {
            Ok(process) => process,
            Err(error) => {
                self.release_reservation(&key).await;
                return Err(error);
            }
        };
        self.install_reserved_process(key.clone(), process.clone())
            .await;
        if let Err(error) = self.wait_until_healthy(&process, launch_timeout).await {
            self.remove_process_if_same(&key, &process).await;
            process.stop().await;
            return Err(error);
        }
        self.finish_launch(
            expected_epoch,
            &request,
            key,
            process,
            resolved.application,
            &host_context,
        )
        .await
    }

    pub(super) async fn discard_presentation(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        generation: u64,
    ) -> Result<(), String> {
        validate_text(owner_user_id, 512)?;
        validate_text(presentation_id, 512)?;
        if generation == 0 {
            return Err(plugin_runtime_error());
        }
        let key = PresentationKey {
            owner_user_id: owner_user_id.to_owned(),
            presentation_id: presentation_id.to_owned(),
        };
        let mut state = self.state.lock().await;
        if let Some(current) = state.presentations.get_mut(&key) {
            if current.generation <= generation {
                current.generation = generation;
                current.active = false;
                current.runtime_key = None;
                current.session_id = None;
            }
        } else if state.presentations.len() < MAXIMUM_PRESENTATIONS {
            state.presentations.insert(
                key,
                PresentationState {
                    generation,
                    active: false,
                    runtime_key: None,
                    session_id: None,
                },
            );
        } else {
            return Err("Plugin 应用展示会话过多。".into());
        }
        Ok(())
    }

    pub(crate) async fn reset(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let processes = {
            let mut state = self.state.lock().await;
            state.presentations.clear();
            state.launch_locks.clear();
            state.reservations.clear();
            state
                .processes
                .drain()
                .map(|(_, process)| process)
                .collect::<Vec<_>>()
        };
        for process in &processes {
            process.terminate_now();
        }
        for process in processes {
            process.stop().await;
        }
    }

    async fn begin_presentation(
        &self,
        expected_epoch: u64,
        request: &PluginApplicationLaunchRequest,
    ) -> Result<(), String> {
        if self.epoch() != expected_epoch {
            return Err(stale_launch_error());
        }
        let key = presentation_key(request);
        let mut state = self.state.lock().await;
        if self.epoch() != expected_epoch {
            return Err(stale_launch_error());
        }
        match state.presentations.get(&key) {
            Some(current) if current.generation >= request.generation => {
                return Err(stale_launch_error())
            }
            None if state.presentations.len() >= MAXIMUM_PRESENTATIONS => {
                return Err("Plugin 应用展示会话过多。".into())
            }
            _ => {}
        }
        state.presentations.insert(
            key,
            PresentationState {
                generation: request.generation,
                active: true,
                runtime_key: None,
                session_id: None,
            },
        );
        Ok(())
    }

    async fn accepts(&self, expected_epoch: u64, request: &PluginApplicationLaunchRequest) -> bool {
        if self.epoch() != expected_epoch {
            return false;
        }
        self.state
            .lock()
            .await
            .presentations
            .get(&presentation_key(request))
            .is_some_and(|state| state.active && state.generation == request.generation)
    }

    async fn finish_launch(
        &self,
        expected_epoch: u64,
        request: &PluginApplicationLaunchRequest,
        runtime_key: RuntimeKey,
        process: Arc<RunningPluginApplication>,
        application: PresentedPluginApplication,
        host_context: &ResolvedHostContext,
    ) -> Result<PresentedPluginApplicationLaunch, String> {
        if self.epoch() != expected_epoch || !process.alive.load(Ordering::SeqCst) {
            return Err(stale_launch_error());
        }
        let session_id = Uuid::new_v4().to_string();
        let mut state = self.state.lock().await;
        if self.epoch() != expected_epoch
            || !state
                .processes
                .get(&runtime_key)
                .is_some_and(|current| Arc::ptr_eq(current, &process))
        {
            return Err(stale_launch_error());
        }
        let presentation = state
            .presentations
            .get_mut(&presentation_key(request))
            .filter(|state| state.active && state.generation == request.generation)
            .ok_or_else(stale_launch_error)?;
        presentation.runtime_key = Some(runtime_key.clone());
        presentation.session_id = Some(session_id.clone());
        Ok(PresentedPluginApplicationLaunch {
            session_id,
            storage_partition_id: storage_partition_id(&application.id(), &runtime_key.scope_key),
            url: process.base_url.clone(),
            context: match (&host_context.project_id, &host_context.project_name) {
                (Some(project_id), Some(project_name)) => Some(PresentedPluginApplicationContext {
                    project_id: project_id.clone(),
                    project_name: project_name.clone(),
                }),
                _ => None,
            },
            release_id: process.release_id.clone(),
            release_digest: process.release_digest.clone(),
            version: process.version.clone(),
            application,
        })
    }

    async fn launch_lock(&self, key: &RuntimeKey) -> Arc<Mutex<()>> {
        self.state
            .lock()
            .await
            .launch_locks
            .entry(key.clone())
            .or_default()
            .clone()
    }

    async fn current_process(&self, key: &RuntimeKey) -> Option<Arc<RunningPluginApplication>> {
        self.state.lock().await.processes.get(key).cloned()
    }

    async fn remove_process_if_same(
        &self,
        key: &RuntimeKey,
        process: &Arc<RunningPluginApplication>,
    ) {
        let mut state = self.state.lock().await;
        if state
            .processes
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, process))
        {
            state.processes.remove(key);
        }
    }

    async fn reserve(&self, key: &RuntimeKey) -> Result<(), String> {
        let mut state = self.state.lock().await;
        state
            .processes
            .retain(|_, process| process.alive.load(Ordering::SeqCst));
        let count = state
            .processes
            .keys()
            .chain(state.reservations.iter())
            .filter(|candidate| candidate.owner_user_id == key.owner_user_id)
            .count();
        if count >= MAXIMUM_RUNTIMES_PER_OWNER {
            return Err("当前账号启动的 Plugin 应用过多。".into());
        }
        if !state.reservations.insert(key.clone()) {
            return Err(plugin_runtime_error());
        }
        Ok(())
    }

    async fn release_reservation(&self, key: &RuntimeKey) {
        self.state.lock().await.reservations.remove(key);
    }

    async fn install_reserved_process(
        &self,
        key: RuntimeKey,
        process: Arc<RunningPluginApplication>,
    ) {
        let mut state = self.state.lock().await;
        state.reservations.remove(&key);
        state.processes.insert(key, process);
    }

    async fn wait_until_healthy(
        &self,
        process: &Arc<RunningPluginApplication>,
        timeout_ms: u64,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            if !process.alive.load(Ordering::SeqCst) {
                return Err("Plugin 应用后端在启动阶段退出。".into());
            }
            if self.is_healthy(process).await {
                return Ok(());
            }
            sleep(Duration::from_millis(HEALTH_RETRY_MS)).await;
        }
        Err("等待 Plugin 应用后端就绪超时。".into())
    }

    async fn is_healthy(&self, process: &RunningPluginApplication) -> bool {
        if !process.alive.load(Ordering::SeqCst) {
            return false;
        }
        let url = format!(
            "{}{}",
            process.base_url.trim_end_matches('/'),
            process.health_path
        );
        let Ok(response) = self
            .health_client
            .get(url)
            .timeout(Duration::from_millis(HEALTH_REQUEST_TIMEOUT_MS))
            .send()
            .await
        else {
            return false;
        };
        response.status().is_success()
    }
}

impl RunningPluginApplication {
    fn terminate_now(&self) {
        let _ = self.termination.terminate(true);
    }

    async fn stop(&self) {
        if !self.alive.load(Ordering::SeqCst) {
            return;
        }
        let _ = self.termination.terminate(false);
        if timeout(
            Duration::from_millis(TERMINATION_GRACE_MS),
            self.wait_finished(),
        )
        .await
        .is_ok()
        {
            return;
        }
        let _ = self.termination.terminate(true);
        let _ = timeout(
            Duration::from_millis(TERMINATION_GRACE_MS),
            self.wait_finished(),
        )
        .await;
    }

    async fn wait_finished(&self) {
        loop {
            let notified = self.finished.notified();
            if !self.alive.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

fn validate_launch_request(request: &PluginApplicationLaunchRequest) -> Result<(), String> {
    validate_text(&request.owner_user_id, 512)?;
    validate_text(&request.presentation_id, 512)?;
    validate_text(&request.plugin_id, 1_024)?;
    validate_text(&request.component_key, 1_024)?;
    if request.generation == 0 {
        return Err(plugin_runtime_error());
    }
    if let PluginApplicationContextSelection::Project { project_id } = &request.context {
        validate_text(project_id, 1_024)?;
    }
    Ok(())
}

fn presentation_key(request: &PluginApplicationLaunchRequest) -> PresentationKey {
    PresentationKey {
        owner_user_id: request.owner_user_id.clone(),
        presentation_id: request.presentation_id.clone(),
    }
}

fn resolve_host_context(
    runtime_root: &Path,
    owner_user_id: &str,
    application: &PresentedPluginApplication,
    selection: &PluginApplicationContextSelection,
) -> Result<ResolvedHostContext, String> {
    let requires_project = matches!(
        application.context_scope.as_deref(),
        Some("project" | "workspace")
    );
    match selection {
        PluginApplicationContextSelection::None if requires_project => {
            return Err("请先选择 Plugin 应用使用的项目。".into())
        }
        PluginApplicationContextSelection::Device
            if !requires_project || application.missing_context.as_deref() != Some("device") =>
        {
            return Err("Plugin 应用不允许使用这个上下文。".into())
        }
        PluginApplicationContextSelection::Project { .. } if !requires_project => {
            return Err("Plugin 应用不允许使用这个上下文。".into())
        }
        _ => {}
    }
    let connector = runtime_root.parent().ok_or_else(plugin_runtime_error)?;
    let (device_id, connector_owner) = load_device_context(&connector.join("state.json"))?;
    if connector_owner != owner_user_id {
        return Err(plugin_unavailable());
    }
    match selection {
        PluginApplicationContextSelection::None | PluginApplicationContextSelection::Device => {
            Ok(ResolvedHostContext {
                owner_user_id: owner_user_id.to_owned(),
                device_id,
                workspace_id: None,
                workspace_root: None,
                project_id: None,
                project_name: None,
            })
        }
        PluginApplicationContextSelection::Project { project_id } => {
            let resolved = resolve_project_root(connector, owner_user_id, project_id)?;
            let project_name = load_project_name(
                &connector.join("Projects.sqlite3"),
                owner_user_id,
                project_id,
            )?;
            Ok(ResolvedHostContext {
                owner_user_id: owner_user_id.to_owned(),
                device_id: resolved.device_id,
                workspace_id: Some(resolved.workspace_scope_id),
                workspace_root: Some(resolved.root),
                project_id: Some(project_id.clone()),
                project_name: Some(project_name),
            })
        }
    }
}

fn load_device_context(path: &Path) -> Result<(String, String), String> {
    let bytes = bounded_file(path, MAXIMUM_CONNECTOR_STATE_BYTES)?;
    let state =
        serde_json::from_slice::<ConnectorState>(&bytes).map_err(|_| plugin_runtime_error())?;
    let owner = state
        .user
        .map(|user| user.id)
        .ok_or_else(plugin_unavailable)?;
    let device = state.device_id.ok_or_else(plugin_unavailable)?;
    validate_text(&owner, 512)?;
    validate_text(&device, 1_024)?;
    Ok((device, owner))
}

fn load_project_name(
    database: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<String, String> {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|_| plugin_unavailable())?;
    let name = connection
        .query_row(
            "SELECT name FROM local_project_records \
             WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
            params![owner_user_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| plugin_unavailable())?;
    validate_text(&name, 4_096)?;
    Ok(name)
}

fn resolve_runtime_context(
    runtime_root: &Path,
    resolved: &ResolvedPluginApplication,
    host: &ResolvedHostContext,
) -> Result<ResolvedRuntimeContext, String> {
    let plugin_hash = sha256(&resolved.record.spec.plugin_id);
    let user_hash = sha256(&host.owner_user_id);
    let user_data = PathBuf::from("data")
        .join("users")
        .join(&user_hash)
        .join(&plugin_hash);
    let user_cache = PathBuf::from("cache")
        .join("users")
        .join(&user_hash)
        .join(&plugin_hash);
    let Some(declaration) = resolved
        .manifest
        .runtime_context
        .as_ref()
        .filter(|context| {
            context
                .components
                .contains(&resolved.contribution.component_key)
        })
    else {
        return Ok(ResolvedRuntimeContext {
            data_directory: create_confined_directory(runtime_root, &user_data)?,
            cache_directory: create_confined_directory(runtime_root, &user_cache)?,
            environment: BTreeMap::new(),
            scope_key: format!("user:{user_hash}"),
        });
    };
    let requested = declaration
        .required
        .iter()
        .chain(&declaration.optional)
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let missing = declaration
        .required
        .iter()
        .filter(|field| match field.as_str() {
            "project.id" => host.project_id.is_none(),
            "workspace.id" => host.workspace_id.is_none(),
            "workspace.root" => host.workspace_root.is_none(),
            _ => true,
        });
    let missing = missing.cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("Plugin 运行缺少必需上下文：{}", missing.join(", ")));
    }
    let resolved_scope = match declaration.scope.as_str() {
        "device" => ("device", format!("device:{}", host.device_id)),
        "workspace" => host
            .workspace_id
            .as_ref()
            .map(|id| ("workspace", format!("workspace:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .ok_or_else(|| "Plugin 需要项目或工作区上下文。".to_owned())?,
        "project" => host
            .project_id
            .as_ref()
            .map(|id| ("project", format!("project:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .ok_or_else(|| "Plugin 需要项目或工作区上下文。".to_owned())?,
        _ => return Err(plugin_catalog_error()),
    };
    let storage_scope = match declaration.storage_isolation.as_str() {
        "plugin" => ("plugin", "plugin".to_owned()),
        "workspace" => host
            .workspace_id
            .as_ref()
            .map(|id| ("workspace", format!("workspace:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .unwrap_or_else(|| (resolved_scope.0, resolved_scope.1.clone())),
        "project" => host
            .project_id
            .as_ref()
            .map(|id| ("project", format!("project:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .unwrap_or_else(|| (resolved_scope.0, resolved_scope.1.clone())),
        _ => return Err(plugin_catalog_error()),
    };
    let scope_hash = sha256(&resolved_scope.1);
    let storage_hash = sha256(&storage_scope.1);
    let (data_relative, cache_relative) = if declaration.storage_isolation == "plugin" {
        (user_data, user_cache)
    } else {
        (
            user_data
                .join("scopes")
                .join(storage_scope.0)
                .join(&storage_hash),
            user_cache
                .join("scopes")
                .join(storage_scope.0)
                .join(&storage_hash),
        )
    };
    let mut environment = BTreeMap::from([
        ("CHATOS_CONTEXT_SCOPE".into(), resolved_scope.0.into()),
        ("CHATOS_CONTEXT_SCOPE_ID".into(), scope_hash.clone()),
    ]);
    if requested.contains("project.id") {
        if let Some(project_id) = &host.project_id {
            environment.insert("CHATOS_PROJECT_ID".into(), project_id.clone());
        }
    }
    if requested.contains("workspace.id") {
        if let Some(workspace_id) = &host.workspace_id {
            environment.insert("CHATOS_WORKSPACE_ID".into(), workspace_id.clone());
        }
    }
    if requested.contains("workspace.root") {
        if let Some(workspace_root) = &host.workspace_root {
            environment.insert(
                "CHATOS_WORKSPACE".into(),
                workspace_root.to_string_lossy().into_owned(),
            );
        }
    }
    if let Some(project_name) = &host.project_name {
        environment.insert("CHATOS_PROJECT_NAME".into(), project_name.clone());
    }
    Ok(ResolvedRuntimeContext {
        data_directory: create_confined_directory(runtime_root, &data_relative)?,
        cache_directory: create_confined_directory(runtime_root, &cache_relative)?,
        environment,
        scope_key: format!("user:{user_hash}:{}:{scope_hash}", resolved_scope.0),
    })
}

fn prepare_runtime_process(
    resolved: &ResolvedPluginApplication,
    context: &ResolvedRuntimeContext,
) -> Result<PreparedRuntimeProcess, String> {
    let runtime = resolved
        .contribution
        .runtime
        .as_ref()
        .ok_or_else(|| "Windows 尚未准入静态 Plugin 应用页面。".to_owned())?;
    let (executable, prefix_arguments) =
        resolve_package_executable(&resolved.installation_root, &runtime.bin)?;
    let port = available_loopback_port()?;
    let base_url = format!("http://127.0.0.1:{port}/");
    let health_path = runtime
        .health_path
        .clone()
        .unwrap_or_else(|| DEFAULT_HEALTH_PATH.into());
    if !valid_health_path(&health_path) {
        return Err(plugin_catalog_error());
    }
    let mut environment = safe_process_environment();
    environment.extend(context.environment.clone());
    environment.extend([
        (
            "CHATOS_PLUGIN_ROOT".into(),
            resolved.installation_root.to_string_lossy().into_owned(),
        ),
        (
            "CHATOS_PLUGIN_DATA_DIR".into(),
            context.data_directory.to_string_lossy().into_owned(),
        ),
        (
            "CHATOS_PLUGIN_CACHE_DIR".into(),
            context.cache_directory.to_string_lossy().into_owned(),
        ),
        ("CHATOS_PLUGIN_APP_HOST".into(), "127.0.0.1".into()),
        ("CHATOS_PLUGIN_APP_PORT".into(), port.to_string()),
        (
            "CHATOS_PLUGIN_ID".into(),
            resolved.record.spec.plugin_id.clone(),
        ),
        (
            "CHATOS_PLUGIN_COMPONENT_KEY".into(),
            resolved.contribution.component_key.clone(),
        ),
        (
            "CHATOS_PLUGIN_RELEASE_ID".into(),
            resolved.record.spec.release_id.clone(),
        ),
        (
            "CHATOS_PLUGIN_VERSION".into(),
            resolved.manifest.version.clone(),
        ),
        (
            "CHATOS_PLUGIN_ARTIFACT_SHA256".into(),
            resolved.record.spec.release_digest.clone(),
        ),
        ("CHATOS_PLUGIN_FILE_WATCH_MODE".into(), "polling".into()),
        (
            "CHATOS_PLUGIN_SANDBOX".into(),
            "windows-job-lifecycle".into(),
        ),
    ]);
    let arguments = prefix_arguments
        .into_iter()
        .chain(runtime.args.iter().map(OsString::from))
        .collect();
    Ok(PreparedRuntimeProcess {
        executable,
        arguments,
        environment,
        installation_root: resolved.installation_root.clone(),
        base_url,
        health_path,
        launch_timeout_ms: runtime
            .launch_timeout_ms
            .unwrap_or(DEFAULT_LAUNCH_TIMEOUT_MS),
    })
}

async fn spawn_runtime_process(
    resolved: &ResolvedPluginApplication,
    prepared: PreparedRuntimeProcess,
) -> Result<Arc<RunningPluginApplication>, String> {
    let mut command = Command::new(&prepared.executable);
    command
        .args(&prepared.arguments)
        .current_dir(&prepared.installation_root)
        .env_clear()
        .envs(&prepared.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Plugin 应用后端启动失败。".to_owned())?;
    let termination = match ProcessTermination::attach(&child) {
        Ok(termination) => termination,
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err("无法隔离 Plugin 应用进程树。".into());
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let process = Arc::new(RunningPluginApplication {
        release_id: resolved.record.spec.release_id.clone(),
        release_digest: resolved.record.spec.release_digest.clone(),
        version: resolved.manifest.version.clone(),
        base_url: prepared.base_url,
        health_path: prepared.health_path,
        termination,
        alive: AtomicBool::new(true),
        finished: Notify::new(),
    });
    let stdout_task = stdout.map(|pipe| tokio::spawn(drain_pipe(pipe)));
    let stderr_task = stderr.map(|pipe| tokio::spawn(drain_pipe(pipe)));
    tokio::spawn(wait_for_process(
        process.clone(),
        child,
        stdout_task,
        stderr_task,
    ));
    Ok(process)
}

async fn drain_pipe(mut pipe: impl AsyncRead + Unpin) {
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

async fn wait_for_process(
    process: Arc<RunningPluginApplication>,
    mut child: Child,
    stdout: Option<tokio::task::JoinHandle<()>>,
    stderr: Option<tokio::task::JoinHandle<()>>,
) {
    let _ = child.wait().await;
    process.alive.store(false, Ordering::SeqCst);
    process.finished.notify_waiters();
    if let Some(task) = stdout {
        let _ = timeout(Duration::from_secs(1), task).await;
    }
    if let Some(task) = stderr {
        let _ = timeout(Duration::from_secs(1), task).await;
    }
}

fn resolve_package_executable(
    installation_root: &Path,
    bin_name: &str,
) -> Result<(PathBuf, Vec<OsString>), String> {
    let package_path = resolve_non_linked_file(installation_root, "package.json")?;
    let package = serde_json::from_slice::<PackageManifest>(&bounded_file(
        &package_path,
        MAXIMUM_PACKAGE_JSON_BYTES,
    )?)
    .map_err(|_| "Plugin package.json 无效。".to_owned())?;
    let target = match &package.bin {
        Value::Object(bins) => bins.get(bin_name).and_then(Value::as_str),
        Value::String(target)
            if package
                .name
                .split('/')
                .next_back()
                .is_some_and(|name| name == bin_name) =>
        {
            Some(target.as_str())
        }
        _ => None,
    }
    .ok_or_else(|| "Plugin 应用入口未在 package.json.bin 中声明。".to_owned())?;
    let target = resolve_non_linked_file(installation_root, target)?;
    if is_node_script(&target)? {
        let node = find_system_executable("node")
            .ok_or_else(|| "Plugin 应用需要本机 Node.js 运行时。".to_owned())?;
        return Ok((node, vec![target.into_os_string()]));
    }
    #[cfg(windows)]
    if !target
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("Plugin 应用入口不是可执行文件。".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if target
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 == 0)
            .unwrap_or(true)
        {
            return Err("Plugin 应用入口不是可执行文件。".into());
        }
    }
    Ok((target, Vec::new()))
}

fn is_node_script(path: &Path) -> Result<bool, String> {
    let extension_is_node = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "js" | "mjs" | "cjs"
            )
        });
    if extension_is_node {
        return Ok(true);
    }
    let mut prefix = [0_u8; 256];
    let count = File::open(path)
        .and_then(|mut file| file.read(&mut prefix))
        .map_err(|_| plugin_runtime_error())?;
    let header = String::from_utf8_lossy(&prefix[..count]);
    Ok(header.starts_with("#!")
        && header
            .lines()
            .next()
            .is_some_and(|line| line.contains("node")))
}

fn find_system_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    for directory in std::env::split_paths(&path) {
        for extension in extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            let Ok(metadata) = fs::symlink_metadata(&candidate) else {
                continue;
            };
            if !metadata.is_file() || is_link_or_reparse_point(&metadata) {
                continue;
            }
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical
                .metadata()
                .ok()
                .is_some_and(|metadata| metadata.is_file())
            {
                return Some(canonical);
            }
        }
    }
    None
}

fn resolve_non_linked_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.len() > 4_096 || relative.chars().any(char::is_control) {
        return Err(plugin_catalog_error());
    }
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(plugin_catalog_error());
    }
    let canonical_root = root.canonicalize().map_err(|_| plugin_catalog_error())?;
    let mut candidate = canonical_root.clone();
    for component in relative.components() {
        if let Component::Normal(component) = component {
            candidate.push(component);
            let metadata = fs::symlink_metadata(&candidate).map_err(|_| plugin_catalog_error())?;
            if is_link_or_reparse_point(&metadata) {
                return Err(plugin_catalog_error());
            }
        }
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        return Err(plugin_catalog_error());
    }
    Ok(canonical)
}

fn create_confined_directory(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(plugin_runtime_error());
    }
    fs::create_dir_all(root).map_err(|_| plugin_runtime_error())?;
    let requested_root_metadata = fs::symlink_metadata(root).map_err(|_| plugin_runtime_error())?;
    if !requested_root_metadata.is_dir() || is_link_or_reparse_point(&requested_root_metadata) {
        return Err(plugin_runtime_error());
    }
    let root = root.canonicalize().map_err(|_| plugin_runtime_error())?;
    if is_link_or_reparse_point(&fs::symlink_metadata(&root).map_err(|_| plugin_runtime_error())?) {
        return Err(plugin_runtime_error());
    }
    let mut current = root.clone();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(plugin_runtime_error());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if !metadata.is_dir() || is_link_or_reparse_point(&metadata) {
                    return Err(plugin_runtime_error());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|_| plugin_runtime_error())?;
            }
            Err(_) => return Err(plugin_runtime_error()),
        }
    }
    let canonical = current.canonicalize().map_err(|_| plugin_runtime_error())?;
    if !canonical.starts_with(&root) {
        return Err(plugin_runtime_error());
    }
    Ok(canonical)
}

fn bounded_file(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| plugin_runtime_error())?;
    if !metadata.is_file() || is_link_or_reparse_point(&metadata) || metadata.len() > maximum {
        return Err(plugin_runtime_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(maximum + 1).read_to_end(&mut bytes))
        .map_err(|_| plugin_runtime_error())?;
    if bytes.len() as u64 > maximum {
        return Err(plugin_runtime_error());
    }
    Ok(bytes)
}

fn safe_process_environment() -> BTreeMap<String, String> {
    const ALLOWED: &[&str] = &[
        "APPDATA",
        "ComSpec",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "PATH",
        "PATHEXT",
        "PROGRAMDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "SHELL",
        "SystemRoot",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USER",
        "USERPROFILE",
        "WINDIR",
    ];
    ALLOWED
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).into(), value))
        })
        .collect()
}

fn available_loopback_port() -> Result<u16, String> {
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(|_| "无法分配 Plugin 应用端口。".to_owned())?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|_| "无法读取 Plugin 应用端口。".to_owned())
}

fn storage_partition_id(application_id: &str, scope_key: &str) -> String {
    let value = sha256(&format!("{application_id}\n{scope_key}"));
    format!(
        "{}-{}-{}-{}-{}",
        &value[0..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..32]
    )
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(windows)]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn stale_launch_error() -> String {
    "Plugin 应用启动请求已失效。".into()
}

fn plugin_runtime_error() -> String {
    "Plugin 应用运行环境无效。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_application::{load_manifest, project_application, PluginRuntimeContext};
    use chatos_local_agent_protocol::{LocalPluginInstallationRecord, LocalPluginInstallationSpec};
    use serde_json::json;
    use std::io::Write as _;

    struct RuntimeFixture {
        _directory: tempfile::TempDir,
        app_data: PathBuf,
        installation: PathBuf,
        resolved: ResolvedPluginApplication,
    }

    impl RuntimeFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let app_data = directory.path().join("app-data");
            let connector = app_data.join("NativeConnector");
            let installation = connector.join("Plugins").join("plugin-1").join("1.0.0");
            fs::create_dir_all(installation.join("ui")).unwrap();
            fs::write(installation.join("ui/index.html"), "<main>Plugin</main>").unwrap();
            let fixture_name = if cfg!(windows) {
                "plugin-runtime-fixture.exe"
            } else {
                "plugin-runtime-fixture"
            };
            let fixture_executable = installation.join(fixture_name);
            fs::copy(std::env::current_exe().unwrap(), &fixture_executable).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fixture_executable.metadata().unwrap().permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&fixture_executable, permissions).unwrap();
            }
            fs::write(
                installation.join("package.json"),
                serde_json::to_vec(&json!({
                    "name": "plugin-runtime-fixture",
                    "bin": {"fixture": fixture_name}
                }))
                .unwrap(),
            )
            .unwrap();
            fs::write(
                installation.join("chatos.plugin.json"),
                serde_json::to_vec(&json!({
                    "schemaVersion": 3,
                    "name": "plugin-runtime-fixture",
                    "version": "1.0.0",
                    "description": "Runtime fixture",
                    "ui": [{
                        "componentKey": "plugin-workbench",
                        "source": "./ui/index.html",
                        "title": "Plugin Runtime",
                        "surface": "workbench",
                        "bridgeCapabilities": ["host.context.read"],
                        "runtime": {
                            "type": "local_http",
                            "bin": "fixture",
                            "args": [
                                "--exact",
                                "plugin_application::runtime::tests::plugin_runtime_fixture_process",
                                "--nocapture"
                            ],
                            "healthPath": "/api/health",
                            "launchTimeoutMs": 15000
                        }
                    }],
                    "dependencies": {"supportedPlatforms": ["windows"]},
                    "permissions": [{
                        "permission": "process.spawn",
                        "required": true,
                        "components": ["plugin-workbench"]
                    }]
                }))
                .unwrap(),
            )
            .unwrap();
            fs::write(
                connector.join("state.json"),
                serde_json::to_vec(&json!({
                    "user": {"id": "owner-1"},
                    "deviceID": "device-1",
                    "workspaces": []
                }))
                .unwrap(),
            )
            .unwrap();
            let record = LocalPluginInstallationRecord {
                spec: LocalPluginInstallationSpec {
                    installation_id: "installation-1".into(),
                    owner_user_id: "owner-1".into(),
                    plugin_id: "plugin-1".into(),
                    release_id: "release-1".into(),
                    release_digest: format!("sha256:{}", "a".repeat(64)),
                    component_id: "component-1".into(),
                    component_revision: "revision-1".into(),
                    server_id: "server-1".into(),
                    executable_path: fixture_executable.to_string_lossy().into_owned(),
                    args: Vec::new(),
                    working_directory: None,
                    environment_secret_refs: BTreeMap::from([(
                        "API_TOKEN".into(),
                        "secret-reference".into(),
                    )]),
                    tool_prefix: None,
                    allowed_tools: None,
                    enabled: true,
                },
                version: 1,
                created_at_unix_ms: 10,
                updated_at_unix_ms: 20,
            };
            let plugin_root = connector.join("Plugins").canonicalize().unwrap();
            let (resolved_installation, manifest) = load_manifest(&plugin_root, &record).unwrap();
            let contribution = manifest.ui[0].clone();
            let application = project_application(
                &plugin_root,
                &resolved_installation,
                &record,
                &manifest,
                &contribution,
            )
            .unwrap();
            Self {
                _directory: directory,
                app_data,
                installation,
                resolved: ResolvedPluginApplication {
                    record,
                    installation_root: resolved_installation,
                    manifest,
                    contribution,
                    application,
                },
            }
        }

        fn runtime(&self) -> PluginApplicationRuntime {
            PluginApplicationRuntime::new(
                self.app_data.join("NativeConnector").join("PluginRuntime"),
            )
        }

        fn request(&self, generation: u64) -> PluginApplicationLaunchRequest {
            PluginApplicationLaunchRequest {
                owner_user_id: "owner-1".into(),
                presentation_id: "presentation-1".into(),
                generation,
                plugin_id: "plugin-1".into(),
                component_key: "plugin-workbench".into(),
                context: PluginApplicationContextSelection::None,
            }
        }
    }

    #[test]
    fn plugin_runtime_fixture_process() {
        let Ok(port) = std::env::var("CHATOS_PLUGIN_APP_PORT") else {
            return;
        };
        let listener = std::net::TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut request = [0_u8; 8 * 1_024];
            let count = stream.read(&mut request).unwrap_or(0);
            let success = request[..count].starts_with(b"GET /api/health ");
            let (status, body) = if success {
                ("200 OK", "ok")
            } else {
                ("404 Not Found", "missing")
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    }

    #[tokio::test]
    async fn runtime_launches_reuses_and_resets_a_healthy_owned_process() {
        let fixture = RuntimeFixture::new();
        let runtime = fixture.runtime();
        let epoch = runtime.epoch();

        let first = runtime
            .launch(epoch, fixture.request(1), fixture.resolved.clone())
            .await
            .unwrap();
        let second = runtime
            .launch(epoch, fixture.request(2), fixture.resolved.clone())
            .await
            .unwrap();

        assert_eq!(first.url, second.url);
        assert_ne!(first.session_id, second.session_id);
        assert!(first.url.starts_with("http://127.0.0.1:"));
        assert_eq!(runtime.state.lock().await.processes.len(), 1);
        let serialized = serde_json::to_string(&first).unwrap();
        assert!(!serialized.contains(&fixture.app_data.to_string_lossy().to_string()));
        assert!(!serialized.contains("API_TOKEN"));
        assert!(!serialized.contains("secret-reference"));

        runtime
            .discard_presentation("owner-1", "presentation-1", 2)
            .await
            .unwrap();
        assert!(runtime
            .launch(epoch, fixture.request(2), fixture.resolved.clone())
            .await
            .unwrap_err()
            .contains("失效"));
        runtime.reset().await;
        assert_ne!(runtime.epoch(), epoch);
        assert!(runtime.state.lock().await.processes.is_empty());
    }

    #[tokio::test]
    async fn reset_invalidates_an_in_flight_presentation_epoch() {
        let fixture = RuntimeFixture::new();
        let runtime = fixture.runtime();
        let epoch = runtime.epoch();
        let request = fixture.request(1);
        runtime.begin_presentation(epoch, &request).await.unwrap();

        runtime.reset().await;

        assert!(!runtime.accepts(epoch, &request).await);
        assert!(runtime
            .begin_presentation(epoch, &request)
            .await
            .unwrap_err()
            .contains("失效"));
    }

    #[tokio::test]
    async fn failed_health_start_releases_the_process_and_capacity_reservation() {
        let mut fixture = RuntimeFixture::new();
        fixture.resolved.contribution.runtime.as_mut().unwrap().args = vec![
            "--exact".into(),
            "plugin_application::runtime::tests::missing_fixture".into(),
            "--nocapture".into(),
        ];
        let runtime = fixture.runtime();

        let error = runtime
            .launch(
                runtime.epoch(),
                fixture.request(1),
                fixture.resolved.clone(),
            )
            .await
            .unwrap_err();

        assert!(error.contains("退出") || error.contains("超时"));
        let state = runtime.state.lock().await;
        assert!(state.processes.is_empty());
        assert!(state.reservations.is_empty());
    }

    #[test]
    fn context_selection_follows_the_macos_application_contract() {
        let mut fixture = RuntimeFixture::new();
        let runtime_root = fixture
            .app_data
            .join("NativeConnector")
            .join("PluginRuntime");

        assert!(resolve_host_context(
            &runtime_root,
            "owner-1",
            &fixture.resolved.application,
            &PluginApplicationContextSelection::Device,
        )
        .is_err());

        fixture.resolved.application.context_scope = Some("project".into());
        fixture.resolved.application.missing_context = Some("device".into());
        assert!(resolve_host_context(
            &runtime_root,
            "owner-1",
            &fixture.resolved.application,
            &PluginApplicationContextSelection::None,
        )
        .is_err());
        let device = resolve_host_context(
            &runtime_root,
            "owner-1",
            &fixture.resolved.application,
            &PluginApplicationContextSelection::Device,
        )
        .unwrap();
        assert_eq!(device.device_id, "device-1");
        assert!(device.project_id.is_none());
        assert!(resolve_host_context(
            &runtime_root,
            "owner-2",
            &fixture.resolved.application,
            &PluginApplicationContextSelection::Device,
        )
        .is_err());
    }

    #[test]
    fn context_storage_is_owner_plugin_and_project_scoped() {
        let mut fixture = RuntimeFixture::new();
        fixture.resolved.manifest.runtime_context = Some(PluginRuntimeContext {
            scope: "project".into(),
            components: vec!["plugin-workbench".into()],
            required: vec!["project.id".into(), "workspace.root".into()],
            optional: vec!["workspace.id".into()],
            storage_isolation: "project".into(),
            missing_context: "reject".into(),
        });
        let first_project = ResolvedHostContext {
            owner_user_id: "owner-1".into(),
            device_id: "device-1".into(),
            workspace_id: Some("workspace-1".into()),
            workspace_root: Some(fixture.installation.clone()),
            project_id: Some("project-1".into()),
            project_name: Some("First".into()),
        };
        let second_project = ResolvedHostContext {
            project_id: Some("project-2".into()),
            project_name: Some("Second".into()),
            ..first_project.clone()
        };

        let first = resolve_runtime_context(
            &fixture.app_data.join("NativeConnector/PluginRuntime"),
            &fixture.resolved,
            &first_project,
        )
        .unwrap();
        let second = resolve_runtime_context(
            &fixture.app_data.join("NativeConnector/PluginRuntime"),
            &fixture.resolved,
            &second_project,
        )
        .unwrap();

        assert_ne!(first.scope_key, second.scope_key);
        assert_ne!(first.data_directory, second.data_directory);
        assert_eq!(first.environment["CHATOS_PROJECT_ID"], "project-1");
        assert_eq!(first.environment["CHATOS_WORKSPACE_ID"], "workspace-1");
        assert_eq!(first.environment["CHATOS_PROJECT_NAME"], "First");
        assert!(first.data_directory.starts_with(
            fixture
                .app_data
                .join("NativeConnector/PluginRuntime")
                .canonicalize()
                .unwrap()
        ));
    }

    #[test]
    fn process_environment_never_inherits_plugin_secret_references() {
        let fixture = RuntimeFixture::new();
        let host = ResolvedHostContext {
            owner_user_id: "owner-1".into(),
            device_id: "device-1".into(),
            workspace_id: None,
            workspace_root: None,
            project_id: None,
            project_name: None,
        };
        let context = resolve_runtime_context(
            &fixture.app_data.join("NativeConnector/PluginRuntime"),
            &fixture.resolved,
            &host,
        )
        .unwrap();
        let prepared = prepare_runtime_process(&fixture.resolved, &context).unwrap();

        assert!(!prepared.environment.contains_key("API_TOKEN"));
        assert_eq!(
            prepared.environment["CHATOS_PLUGIN_SANDBOX"],
            "windows-job-lifecycle"
        );
        assert_eq!(prepared.environment["CHATOS_PLUGIN_APP_HOST"], "127.0.0.1");
    }

    #[test]
    fn package_bin_must_remain_a_non_linked_installed_file() {
        let fixture = RuntimeFixture::new();
        let outside = fixture.app_data.join("outside.exe");
        fs::write(&outside, "outside").unwrap();
        fs::write(
            fixture.installation.join("package.json"),
            serde_json::to_vec(&json!({
                "name": "plugin-runtime-fixture",
                "bin": {"fixture": "../../../../outside.exe"}
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(resolve_package_executable(&fixture.installation, "fixture").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn package_bin_symlink_is_rejected_even_when_its_target_is_inside_the_installation() {
        use std::os::unix::fs::symlink;

        let fixture = RuntimeFixture::new();
        let target = fixture.installation.join("real-bin");
        fs::write(&target, "#!/bin/sh\nexit 0\n").unwrap();
        symlink(&target, fixture.installation.join("linked-bin")).unwrap();
        fs::write(
            fixture.installation.join("package.json"),
            serde_json::to_vec(&json!({
                "name": "plugin-runtime-fixture",
                "bin": {"fixture": "linked-bin"}
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(resolve_package_executable(&fixture.installation, "fixture").is_err());
    }

    #[test]
    fn storage_partition_is_stable_without_exposing_scope_identity() {
        let first = storage_partition_id("plugin:app", "owner:secret-project");
        let second = storage_partition_id("plugin:app", "owner:secret-project");

        assert_eq!(first, second);
        assert_eq!(first.len(), 36);
        assert!(!first.contains("secret-project"));
    }
}
