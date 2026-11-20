use super::{
    plugin_catalog_error, plugin_unavailable, valid_health_path, validate_text,
    PresentedPluginApplication, ResolvedPluginApplication,
};
#[cfg(windows)]
use crate::windows_appcontainer::WorkspaceLeaseManager;
use crate::{
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePermit,
        MachineResourcePriority,
    },
    terminal_runtime::process::ProcessTermination,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::{redirect::Policy, Client};
use ring::hmac;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex as StandardMutex,
    },
};
use tokio::{
    sync::{Mutex, Notify},
    time::{sleep, timeout, Duration, Instant},
};
use uuid::Uuid;

mod context;
mod filesystem;
mod loopback_proxy;
mod process_lifecycle;
mod process_preparation;
mod security;
mod static_server;
use context::{
    resolve_host_context, resolve_runtime_context, ResolvedHostContext, ResolvedRuntimeContext,
};
use loopback_proxy::LoopbackProxyShutdown;
use process_lifecycle::spawn_application_runtime;
use security::{
    authenticated_launch_url, secure_random_token, stale_bridge_error, stale_launch_error,
    storage_partition_id, validate_bridge_request, validate_launch_request,
    validated_loopback_origin,
};
use static_server::StaticServerShutdown;

#[cfg(test)]
use loopback_proxy::{
    authorize_loopback_request, inject_authentication_header, read_proxy_headers, secure_equal,
    start_loopback_proxy, unique_header_value, valid_authentication_value, valid_proxy_request,
    valid_proxy_response, LoopbackRequestAuthorization,
};
#[cfg(test)]
use process_preparation::{
    available_loopback_port, prepare_runtime_process, resolve_package_executable,
};
#[cfg(test)]
use security::{health_proof, MAXIMUM_BRIDGE_REQUEST_BYTES};
#[cfg(test)]
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};
#[cfg(test)]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufStream},
    net::{TcpListener as TokioTcpListener, TcpStream},
};

const MAXIMUM_PRESENTATIONS: usize = 1_024;
const MAXIMUM_RUNTIMES_PER_OWNER: usize = 8;
const HEALTH_REQUEST_TIMEOUT_MS: u64 = 700;
const HEALTH_RETRY_MS: u64 = 80;
const TERMINATION_GRACE_MS: u64 = 2_000;
const HOST_CONTEXT_READ_CAPABILITY: &str = "host.context.read";
const PLUGIN_AUTHENTICATION_HEADER: &str = "x-chatos-plugin-token";
const PLUGIN_HEALTH_CHALLENGE_HEADER: &str = "x-chatos-plugin-challenge";
const PLUGIN_HEALTH_PROOF_HEADER: &str = "x-chatos-plugin-proof";
const PLUGIN_SESSION_COOKIE: &str = "CHATOS_PLUGIN_APP_SESSION";
const PLUGIN_SESSION_BOOTSTRAP_PATH: &str = "/__chatos_plugin_session";

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
    host_session_nonce: String,
    application: PresentedPluginApplication,
    url: String,
    bridge_origin: String,
    bridge_capabilities: Vec<String>,
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
    resources: MachineResourceGovernor,
    #[cfg(all(windows, not(test)))]
    sandbox: Arc<Mutex<Option<Arc<WorkspaceLeaseManager>>>>,
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
    bridge: Option<BridgeSession>,
}

#[derive(Clone)]
struct BridgeSession {
    runtime_key: RuntimeKey,
    session_id: String,
    host_session_nonce: String,
    origin: String,
    capabilities: Vec<String>,
    context: Option<PresentedPluginApplicationContext>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PluginApplicationBridgeRequest {
    pub(crate) owner_user_id: String,
    pub(crate) presentation_id: String,
    pub(crate) generation: u64,
    pub(crate) session_id: String,
    pub(crate) host_session_nonce: String,
    pub(crate) origin: String,
    pub(crate) request_id: String,
    pub(crate) method: String,
    pub(crate) payload: Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PresentedPluginApplicationBridgeResponse {
    request_id: String,
    result: Value,
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
    authentication_token: String,
    termination: RuntimeTermination,
    alive: AtomicBool,
    finished: Notify,
    resource_permit: StandardMutex<Option<MachineResourcePermit>>,
}

enum RuntimeTermination {
    Process {
        process: ProcessTermination,
        proxy: Arc<LoopbackProxyShutdown>,
    },
    Static(Arc<StaticServerShutdown>),
}

impl PluginApplicationRuntime {
    #[cfg(test)]
    pub(super) fn new(root: PathBuf) -> Self {
        Self::new_with_resources(root, MachineResourceGovernor::default())
    }

    pub(super) fn new_with_resources(root: PathBuf, resources: MachineResourceGovernor) -> Self {
        Self {
            root,
            epoch: Arc::new(AtomicU64::new(1)),
            state: Arc::new(Mutex::new(RuntimeState::default())),
            health_client: Client::builder()
                .redirect(Policy::none())
                .no_proxy()
                .build()
                .expect("static loopback health client configuration"),
            resources,
            #[cfg(all(windows, not(test)))]
            sandbox: Arc::new(Mutex::new(None)),
        }
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    #[cfg(all(windows, not(test)))]
    async fn ensure_sandbox(&self) -> Result<Arc<WorkspaceLeaseManager>, String> {
        if let Some(manager) = self.sandbox.lock().await.clone() {
            return Ok(manager);
        }
        let manager = Arc::new(WorkspaceLeaseManager::open(&self.root)?);
        let mut sandbox = self.sandbox.lock().await;
        if sandbox.is_none() {
            *sandbox = Some(manager.clone());
        }
        Ok(sandbox.clone().unwrap_or(manager))
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
        let resource_permit = if resolved.contribution.runtime.is_some() {
            match self
                .resources
                .acquire(
                    MachineResourceKind::CodeReadWorker,
                    MachineResourcePriority::Interactive,
                )
                .await
            {
                Ok(permit) => Some(permit),
                Err(error) => {
                    self.release_reservation(&key).await;
                    return Err(error);
                }
            }
        } else {
            None
        };
        if !self.accepts(expected_epoch, &request).await {
            self.release_reservation(&key).await;
            return Err(stale_launch_error());
        }
        #[cfg(all(windows, not(test)))]
        let sandbox = match self.ensure_sandbox().await {
            Ok(sandbox) => Some(sandbox),
            Err(error) => {
                self.release_reservation(&key).await;
                return Err(error);
            }
        };
        #[cfg(all(windows, test))]
        let sandbox = None;
        let (process, launch_timeout) = match spawn_application_runtime(
            &resolved,
            &runtime_context,
            resource_permit,
            #[cfg(windows)]
            sandbox,
        )
        .await
        {
            Ok(result) => result,
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
                current.bridge = None;
            }
        } else if state.presentations.len() < MAXIMUM_PRESENTATIONS {
            state.presentations.insert(
                key,
                PresentationState {
                    generation,
                    active: false,
                    bridge: None,
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
                bridge: None,
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
        let host_session_nonce = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let bridge_origin = validated_loopback_origin(&process.base_url)?;
        let bridge_capabilities = application
            .bridge_capabilities
            .iter()
            .filter(|capability| capability.as_str() == HOST_CONTEXT_READ_CAPABILITY)
            .cloned()
            .collect::<Vec<_>>();
        let presented_context = match (&host_context.project_id, &host_context.project_name) {
            (Some(project_id), Some(project_name)) => Some(PresentedPluginApplicationContext {
                project_id: project_id.clone(),
                project_name: project_name.clone(),
            }),
            _ => None,
        };
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
        presentation.bridge = Some(BridgeSession {
            runtime_key: runtime_key.clone(),
            session_id: session_id.clone(),
            host_session_nonce: host_session_nonce.clone(),
            origin: bridge_origin.clone(),
            capabilities: bridge_capabilities.clone(),
            context: presented_context.clone(),
        });
        Ok(PresentedPluginApplicationLaunch {
            session_id,
            host_session_nonce,
            storage_partition_id: storage_partition_id(&application.id(), &runtime_key.scope_key),
            url: authenticated_launch_url(&process),
            bridge_origin,
            bridge_capabilities,
            context: presented_context,
            release_id: process.release_id.clone(),
            release_digest: process.release_digest.clone(),
            version: process.version.clone(),
            application,
        })
    }

    pub(super) async fn bridge_request(
        &self,
        request: PluginApplicationBridgeRequest,
    ) -> Result<PresentedPluginApplicationBridgeResponse, String> {
        validate_bridge_request(&request)?;
        let state = self.state.lock().await;
        let presentation = state
            .presentations
            .get(&PresentationKey {
                owner_user_id: request.owner_user_id.clone(),
                presentation_id: request.presentation_id.clone(),
            })
            .filter(|presentation| {
                presentation.active && presentation.generation == request.generation
            })
            .ok_or_else(stale_bridge_error)?;
        let bridge = presentation
            .bridge
            .as_ref()
            .filter(|bridge| {
                bridge.session_id == request.session_id
                    && bridge.host_session_nonce == request.host_session_nonce
                    && bridge.origin == request.origin
                    && bridge.capabilities.contains(&request.method)
            })
            .ok_or_else(stale_bridge_error)?;
        let process = state
            .processes
            .get(&bridge.runtime_key)
            .filter(|process| process.alive.load(Ordering::SeqCst))
            .ok_or_else(stale_bridge_error)?;
        if validated_loopback_origin(&process.base_url)? != bridge.origin {
            return Err(stale_bridge_error());
        }
        let result = match request.method.as_str() {
            HOST_CONTEXT_READ_CAPABILITY => serde_json::json!({
                "projectId": bridge.context.as_ref().map(|context| &context.project_id),
                "projectName": bridge.context.as_ref().map(|context| &context.project_name),
                "capabilities": bridge.capabilities,
            }),
            _ => return Err("Plugin 宿主能力尚未实现。".into()),
        };
        Ok(PresentedPluginApplicationBridgeResponse {
            request_id: request.request_id,
            result,
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
        let Ok(challenge) = secure_random_token() else {
            return false;
        };
        let Ok(response) = self
            .health_client
            .get(url)
            .header(PLUGIN_HEALTH_CHALLENGE_HEADER, &challenge)
            .timeout(Duration::from_millis(HEALTH_REQUEST_TIMEOUT_MS))
            .send()
            .await
        else {
            return false;
        };
        let Some(proof) = response
            .headers()
            .get(PLUGIN_HEALTH_PROOF_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        else {
            return false;
        };
        response.status().is_success()
            && hmac::verify(
                &hmac::Key::new(hmac::HMAC_SHA256, process.authentication_token.as_bytes()),
                challenge.as_bytes(),
                &proof,
            )
            .is_ok()
    }
}

impl RunningPluginApplication {
    fn release_resource(&self) {
        if let Ok(mut permit) = self.resource_permit.lock() {
            permit.take();
        }
    }

    fn terminate_now(&self) {
        match &self.termination {
            RuntimeTermination::Process { process, proxy } => {
                let _ = process.terminate(true);
                proxy.request();
            }
            RuntimeTermination::Static(shutdown) => shutdown.request(),
        }
    }

    async fn stop(&self) {
        if !self.alive.load(Ordering::SeqCst) {
            return;
        }
        match &self.termination {
            RuntimeTermination::Process { process, proxy } => {
                let _ = process.terminate(false);
                proxy.request();
            }
            RuntimeTermination::Static(shutdown) => shutdown.request(),
        }
        if timeout(
            Duration::from_millis(TERMINATION_GRACE_MS),
            self.wait_finished(),
        )
        .await
        .is_ok()
        {
            return;
        }
        match &self.termination {
            RuntimeTermination::Process { process, proxy } => {
                let _ = process.terminate(true);
                proxy.request();
            }
            RuntimeTermination::Static(shutdown) => shutdown.request(),
        }
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

fn presentation_key(request: &PluginApplicationLaunchRequest) -> PresentationKey {
    PresentationKey {
        owner_user_id: request.owner_user_id.clone(),
        presentation_id: request.presentation_id.clone(),
    }
}

fn plugin_runtime_error() -> String {
    "Plugin 应用运行环境无效。".into()
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
