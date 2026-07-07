// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use axum::extract::{Path as AxumPath, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::engine::general_purpose;
use base64::Engine as _;
use chatos_builtin_tools::{
    BrowserToolsOptions, BrowserToolsService, CodeMaintainerOptions, CodeMaintainerService,
    TerminalControllerContext, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStore, TerminalControllerStoreRef,
};
use chrono::{Duration as ChronoDuration, Utc};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

const DEFAULT_CLOUD_BASE_URL: &str = "http://127.0.0.1:39230";
const DEFAULT_USER_SERVICE_BASE_URL: &str = "http://127.0.0.1:39190";
const DEFAULT_LOCAL_API_PORT: u16 = 39232;
const DEFAULT_LOCAL_SANDBOX_IMAGE: &str = "chatos-sandbox-agent:latest";
const DEFAULT_LOCAL_SANDBOX_AGENT_PORT: u16 = 49_888;
const DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX: &str = "chatos-sandbox-agent";
const LOCAL_SANDBOX_BACKEND: &str = "docker";
const LOCAL_SANDBOX_STATUS_READY: &str = "ready";
const LOCAL_SANDBOX_STATUS_DESTROYED: &str = "destroyed";
const HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
const DEFAULT_TERMINAL_EXEC_TIMEOUT_MS: u64 = 30_000;
const MAX_TERMINAL_EXEC_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const MAX_TERMINAL_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_LOCAL_MCP_READ_BYTES: u64 = 256 * 1024;
const MAX_LOCAL_MCP_WRITE_BYTES: usize = 1024 * 1024;
const MAX_LOCAL_MCP_SEARCH_RESULTS: usize = 500;
const MAX_COMMAND_HISTORY_ENTRIES: usize = 1_000;
const DEFAULT_COMMAND_HISTORY_LIMIT: usize = 200;
const MAX_COMMAND_HISTORY_OUTPUT_PREVIEW_BYTES: usize = 16 * 1024;
const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";
const LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER: &str =
    "x-local-connector-enabled-builtin-kinds";

#[derive(Debug, Clone)]
struct ClientConfig {
    cloud_base_url: String,
    access_token: String,
    device_name: String,
    public_key: Option<String>,
    workspace_path: Option<PathBuf>,
    workspace_alias: Option<String>,
    state_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LocalState {
    #[serde(default)]
    auth: Option<AuthState>,
    #[serde(default)]
    paired_cloud_base_url: Option<String>,
    #[serde(default)]
    paired_user_id: Option<String>,
    device_id: Option<String>,
    device_public_key: Option<String>,
    #[serde(default)]
    workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    sandbox: LocalSandboxState,
    #[serde(default)]
    command_history: Vec<CommandHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CommandHistoryEntry {
    id: String,
    source: String,
    workspace_id: Option<String>,
    workspace_alias: Option<String>,
    cwd: Option<String>,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    display: String,
    status: String,
    exit_code: Option<i32>,
    stdout_preview: Option<String>,
    stderr_preview: Option<String>,
    error: Option<String>,
    started_at: String,
    finished_at: Option<String>,
    request_id: Option<String>,
    terminal_session_id: Option<String>,
    sandbox_id: Option<String>,
    tool_name: Option<String>,
}

#[derive(Clone)]
struct CommandHistoryRecorder {
    state_path: PathBuf,
    state: Arc<RwLock<LocalState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthState {
    cloud_base_url: String,
    user_service_base_url: String,
    access_token: String,
    device_name: String,
    user: Option<AuthUserState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthUserState {
    id: String,
    username: String,
    display_name: String,
    role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalSandboxState {
    enabled: bool,
    selected_image_ref: Option<String>,
}

impl Default for LocalSandboxState {
    fn default() -> Self {
        Self {
            enabled: false,
            selected_image_ref: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct LocalSandboxRuntime {
    jobs: Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    leases: Arc<RwLock<HashMap<String, LocalSandboxLease>>>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalSandboxImageJob {
    id: String,
    image_id: String,
    image_name: String,
    image_ref: String,
    features: Vec<String>,
    backend: String,
    status: String,
    created_at: String,
    updated_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    output: String,
    error: Option<String>,
    #[serde(skip_serializing)]
    custom_build_script: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalSandboxLease {
    id: String,
    sandbox_id: String,
    tenant_id: String,
    user_id: String,
    project_id: String,
    run_id: String,
    workspace_root: String,
    run_workspace: String,
    backend: String,
    backend_id: Option<String>,
    image_id: Option<String>,
    image_ref: Option<String>,
    status: String,
    agent_endpoint: Option<String>,
    agent_token: String,
    resource_limits: LocalSandboxResourceLimits,
    network: LocalSandboxNetworkPolicy,
    tools: Vec<String>,
    created_at: String,
    updated_at: String,
    expires_at: String,
    destroyed_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalSandboxResourceLimits {
    cpu: f32,
    memory_mb: u64,
    disk_mb: u64,
    max_processes: u32,
}

impl Default for LocalSandboxResourceLimits {
    fn default() -> Self {
        Self {
            cpu: 2.0,
            memory_mb: 4096,
            disk_mb: 10240,
            max_processes: 128,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalSandboxNetworkPolicy {
    mode: String,
}

impl Default for LocalSandboxNetworkPolicy {
    fn default() -> Self {
        Self {
            mode: "bridge".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceState {
    id: String,
    absolute_root: PathBuf,
    alias: String,
    fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct DeviceResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct WorkspaceResponse {
    id: String,
    local_path_alias: String,
    local_path_fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
    user: AuthUserState,
}

#[derive(Debug, Clone)]
struct LocalRuntime {
    state_path: PathBuf,
    state: Arc<RwLock<LocalState>>,
    http_client: reqwest::Client,
    connector_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    sandbox_runtime: LocalSandboxRuntime,
}

#[derive(Debug, Deserialize)]
struct LocalAuthRequest {
    cloud_base_url: String,
    user_service_base_url: Option<String>,
    username: String,
    password: String,
    display_name: Option<String>,
    device_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddWorkspaceRequest {
    path: String,
    alias: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FsListQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommandHistoryQuery {
    limit: Option<usize>,
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct FsListResponse {
    path: String,
    parent: Option<String>,
    entries: Vec<FsEntry>,
}

#[derive(Debug, Serialize)]
struct FsEntry {
    name: String,
    path: String,
    is_dir: bool,
}

#[derive(Debug, Deserialize)]
struct ToggleSandboxRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct InitializeImageRequest {
    features: Vec<String>,
    custom_build_script: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalTerminalExecRequest {
    workspace_id: String,
    command: String,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RelayRequest {
    #[serde(rename = "type")]
    _message_type: String,
    request_id: String,
    #[allow(dead_code)]
    owner_user_id: Option<String>,
    #[allow(dead_code)]
    device_id: Option<String>,
    workspace_id: String,
    method: Option<String>,
    path: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Value,
}

#[derive(Debug, Serialize)]
struct RelayResponse {
    #[serde(rename = "type")]
    message_type: String,
    request_id: String,
    status: u16,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers: BTreeMap<String, String>,
    body: Value,
}

#[derive(Debug, Deserialize)]
struct TerminalExecRequest {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default, alias = "working_dir")]
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
}

#[derive(Debug)]
struct CommandExecutionContext {
    source: String,
    request_id: Option<String>,
    tool_name: Option<String>,
    terminal_session_id: Option<String>,
    sandbox_id: Option<String>,
}

#[derive(Debug)]
struct InteractiveCommandSubmission {
    command: String,
    cwd: PathBuf,
    blocked_reason: Option<String>,
}

#[derive(Debug)]
struct SandboxToolCallDetails {
    tool_name: String,
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
    display: String,
}

#[derive(Debug, Deserialize)]
struct CreateLocalSandboxLeaseRequest {
    tenant_id: String,
    user_id: String,
    project_id: String,
    run_id: String,
    workspace_root: String,
    image_id: Option<String>,
    #[serde(default)]
    tools: Vec<String>,
    ttl_seconds: Option<u64>,
    resource_limits: Option<LocalSandboxResourceLimits>,
    network: Option<LocalSandboxNetworkPolicy>,
}

#[derive(Debug, Deserialize)]
struct ReleaseLocalSandboxRequest {
    lease_id: String,
    #[serde(default)]
    export_result: bool,
    #[serde(default = "default_true")]
    destroy: bool,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionCreateRequest {
    terminal_session_id: String,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionInputRequest {
    terminal_session_id: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionResizeRequest {
    terminal_session_id: String,
    cols: u16,
    rows: u16,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionSnapshotRequest {
    terminal_session_id: String,
    lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionCloseRequest {
    terminal_session_id: String,
}

#[derive(Clone, Default)]
struct LocalTerminalManager {
    sessions: Arc<Mutex<BTreeMap<String, Arc<LocalPtySession>>>>,
}

struct LocalPtySession {
    id: String,
    root_cwd: PathBuf,
    current_cwd: StdMutex<PathBuf>,
    input_line: StdMutex<String>,
    writer: StdMutex<Box<dyn Write + Send>>,
    master: StdMutex<Box<dyn MasterPty + Send>>,
    child_killer: StdMutex<Box<dyn ChildKiller + Send + Sync>>,
    outbound: mpsc::UnboundedSender<Value>,
    output_history: StdMutex<String>,
    busy: AtomicBool,
    exited: AtomicBool,
}

impl LocalTerminalManager {
    async fn ensure_session(
        &self,
        session_id: String,
        root_cwd: PathBuf,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        outbound: mpsc::UnboundedSender<Value>,
    ) -> Result<Arc<LocalPtySession>> {
        {
            let sessions = self.sessions.lock().await;
            if let Some(existing) = sessions.get(session_id.as_str()) {
                if !existing.exited.load(Ordering::SeqCst) {
                    let _ = existing.resize(cols, rows);
                    return Ok(existing.clone());
                }
            }
        }

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| anyhow!("open pty failed: {err}"))?;
        let shell = select_local_shell();
        let mut command = CommandBuilder::new(shell);
        command.cwd(cwd.as_path());
        command.env("PWD", cwd.display().to_string());
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| anyhow!("spawn shell failed: {err}"))?;
        let child_killer = child.clone_killer();
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| anyhow!("clone pty reader failed: {err}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| anyhow!("take pty writer failed: {err}"))?;

        let session = Arc::new(LocalPtySession {
            id: session_id.clone(),
            root_cwd,
            current_cwd: StdMutex::new(cwd.clone()),
            input_line: StdMutex::new(String::new()),
            writer: StdMutex::new(writer),
            master: StdMutex::new(pair.master),
            child_killer: StdMutex::new(child_killer),
            outbound: outbound.clone(),
            output_history: StdMutex::new(String::new()),
            busy: AtomicBool::new(false),
            exited: AtomicBool::new(false),
        });

        let read_session = session.clone();
        let read_outbound = outbound.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        read_session.append_output(data.as_str());
                        let _ = read_outbound.send(terminal_event(
                            "terminal_output",
                            read_session.id.as_str(),
                            json!({ "data": data }),
                        ));
                    }
                    Err(_) => break,
                }
            }
        });

        let wait_session = session.clone();
        let wait_outbound = outbound;
        std::thread::spawn(move || {
            let code = child
                .wait()
                .ok()
                .map(|status| status.exit_code())
                .unwrap_or(0) as i32;
            wait_session.exited.store(true, Ordering::SeqCst);
            wait_session.busy.store(false, Ordering::SeqCst);
            let _ = wait_outbound.send(terminal_event(
                "terminal_exit",
                wait_session.id.as_str(),
                json!({ "code": code }),
            ));
        });

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    async fn get(&self, session_id: &str) -> Option<Arc<LocalPtySession>> {
        self.sessions.lock().await.get(session_id).cloned()
    }

    async fn close(&self, session_id: &str) {
        let session = self.sessions.lock().await.remove(session_id);
        if let Some(session) = session {
            session.close();
        }
    }
}

impl LocalPtySession {
    fn write_input(&self, data: &str) -> Result<Vec<InteractiveCommandSubmission>> {
        let (forward_data, blocked_messages, submissions) = self.apply_directory_guard(data);
        if forward_data.contains('\r') || forward_data.contains('\n') {
            self.busy.store(true, Ordering::SeqCst);
        }
        if !forward_data.is_empty() {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| anyhow!("terminal writer lock failed"))?;
            writer
                .write_all(forward_data.as_bytes())
                .map_err(|err| anyhow!("write terminal input failed: {err}"))?;
            writer
                .flush()
                .map_err(|err| anyhow!("flush terminal input failed: {err}"))?;
        }
        for message in blocked_messages {
            let data = format!("\r\n{message}\r\n");
            self.append_output(data.as_str());
            let _ = self.outbound.send(terminal_event(
                "terminal_output",
                self.id.as_str(),
                json!({ "data": data }),
            ));
        }
        Ok(submissions)
    }

    fn apply_directory_guard(
        &self,
        data: &str,
    ) -> (String, Vec<String>, Vec<InteractiveCommandSubmission>) {
        if data.is_empty() {
            return (String::new(), Vec::new(), Vec::new());
        }
        let mut line = match self.input_line.lock() {
            Ok(line) => line,
            Err(_) => return (data.to_string(), Vec::new(), Vec::new()),
        };
        let mut current_cwd = match self.current_cwd.lock() {
            Ok(current_cwd) => current_cwd,
            Err(_) => return (data.to_string(), Vec::new(), Vec::new()),
        };
        let mut forward = String::with_capacity(data.len());
        let mut blocked = Vec::new();
        let mut submissions = Vec::new();
        let mut skip_following_lf = false;

        for ch in normalize_terminal_input(data).chars() {
            if skip_following_lf && ch != '\n' {
                skip_following_lf = false;
            }
            match ch {
                '\r' | '\n' => {
                    if skip_following_lf && ch == '\n' {
                        skip_following_lf = false;
                        continue;
                    }
                    let command_line = line.clone();
                    let sanitized = sanitize_terminal_command_line(command_line.as_str());
                    let cwd_before = current_cwd.clone();
                    line.clear();
                    if let Some(reason) = validate_local_terminal_command(
                        sanitized.as_str(),
                        self.root_cwd.as_path(),
                        &mut current_cwd,
                    ) {
                        if !sanitized.trim().is_empty() {
                            submissions.push(InteractiveCommandSubmission {
                                command: sanitized.clone(),
                                cwd: cwd_before,
                                blocked_reason: Some(reason.clone()),
                            });
                        }
                        forward.push_str(clear_terminal_input_line(sanitized.as_str()).as_str());
                        skip_following_lf = ch == '\r';
                        blocked.push(reason);
                        continue;
                    }
                    if !sanitized.trim().is_empty() {
                        submissions.push(InteractiveCommandSubmission {
                            command: sanitized,
                            cwd: cwd_before,
                            blocked_reason: None,
                        });
                    }
                    forward.push(ch);
                }
                '\u{8}' | '\u{7f}' => {
                    line.pop();
                    forward.push(ch);
                }
                '\u{3}' => {
                    line.clear();
                    forward.push(ch);
                }
                _ => {
                    line.push(ch);
                    forward.push(ch);
                }
            }
        }
        (forward, blocked, submissions)
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self
            .master
            .lock()
            .map_err(|_| anyhow!("terminal pty lock failed"))?;
        master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| anyhow!("resize terminal failed: {err}"))
    }

    fn snapshot(&self, lines: usize) -> String {
        let history = match self.output_history.lock() {
            Ok(history) => history.clone(),
            Err(_) => return String::new(),
        };
        let normalized = lines.clamp(1, 10_000);
        let mut items = history.lines().rev().take(normalized).collect::<Vec<_>>();
        items.reverse();
        items.join("\n")
    }

    fn append_output(&self, data: &str) {
        const MAX_HISTORY_BYTES: usize = 1024 * 1024;
        let Ok(mut history) = self.output_history.lock() else {
            return;
        };
        history.push_str(data);
        if history.len() > MAX_HISTORY_BYTES {
            let trim_to = history.len().saturating_sub(MAX_HISTORY_BYTES);
            let mut boundary = trim_to;
            while boundary < history.len() && !history.is_char_boundary(boundary) {
                boundary += 1;
            }
            history.drain(..boundary);
        }
    }

    fn close(&self) {
        self.exited.store(true, Ordering::SeqCst);
        if let Ok(mut killer) = self.child_killer.lock() {
            let _ = killer.kill();
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    let state_path = optional_env("LOCAL_CONNECTOR_STATE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let state = Arc::new(RwLock::new(LocalState::load(state_path.as_path())?));
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build HTTP client")?;

    if let Ok(config) = ClientConfig::from_env() {
        bootstrap_env_config(&http_client, &config, &state).await?;
    }

    let runtime = LocalRuntime {
        state_path,
        state,
        http_client,
        connector_task: Arc::new(Mutex::new(None)),
        sandbox_runtime: LocalSandboxRuntime::default(),
    };
    runtime.start_connector_if_configured().await?;

    serve_local_api(runtime).await
}

impl ClientConfig {
    fn from_env() -> Result<Self> {
        let access_token = required_env("LOCAL_CONNECTOR_ACCESS_TOKEN")?;
        let cloud_base_url = optional_env("LOCAL_CONNECTOR_CLOUD_BASE_URL")
            .unwrap_or_else(|| DEFAULT_CLOUD_BASE_URL.to_string());
        let device_name =
            optional_env("LOCAL_CONNECTOR_DEVICE_NAME").unwrap_or_else(default_device_name);
        let public_key = optional_env("LOCAL_CONNECTOR_PUBLIC_KEY");
        let workspace_path = optional_env("LOCAL_CONNECTOR_WORKSPACE_PATH").map(PathBuf::from);
        let workspace_alias = optional_env("LOCAL_CONNECTOR_WORKSPACE_ALIAS");
        let state_path = optional_env("LOCAL_CONNECTOR_STATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(default_state_path);
        Ok(Self {
            cloud_base_url,
            access_token,
            device_name,
            public_key,
            workspace_path,
            workspace_alias,
            state_path,
        })
    }

    fn from_state(state: &LocalState, state_path: PathBuf) -> Option<Self> {
        let auth = state.auth.as_ref()?;
        Some(Self {
            cloud_base_url: auth.cloud_base_url.clone(),
            access_token: auth.access_token.clone(),
            device_name: auth.device_name.clone(),
            public_key: state.device_public_key.clone(),
            workspace_path: None,
            workspace_alias: None,
            state_path,
        })
    }
}

impl LocalRuntime {
    async fn sync_saved_workspaces_if_needed(&self) -> Result<()> {
        let config = {
            let state = self.state.read().await;
            ClientConfig::from_state(&state, self.state_path.clone())
        };
        let Some(config) = config else {
            return Ok(());
        };

        let mut state = self.state.write().await;
        let previous_device_id = state.device_id.clone();
        let saved_workspaces = state.workspaces.clone();
        let device_id = ensure_device_registered(&self.http_client, &config, &mut state).await?;
        let device_changed = previous_device_id.as_deref() != Some(device_id.as_str());
        if device_changed {
            for workspace in saved_workspaces {
                let workspace_config = ClientConfig {
                    workspace_alias: Some(workspace.alias.clone()),
                    ..config.clone()
                };
                if let Err(err) = ensure_workspace_registered(
                    &self.http_client,
                    &workspace_config,
                    &mut state,
                    device_id.as_str(),
                    workspace.absolute_root.clone(),
                    true,
                )
                .await
                {
                    tracing_stdout(
                        format!(
                            "sync saved workspace {} failed: {err}",
                            workspace.absolute_root.display()
                        )
                        .as_str(),
                    );
                }
            }
        }
        state.save(self.state_path.as_path())?;
        Ok(())
    }

    async fn start_connector_if_configured(&self) -> Result<()> {
        let config = {
            let state = self.state.read().await;
            ClientConfig::from_state(&state, self.state_path.clone())
        };
        let Some(config) = config else {
            return Ok(());
        };
        let device_id = {
            let mut state = self.state.write().await;
            let device_id =
                ensure_device_registered(&self.http_client, &config, &mut state).await?;
            state.save(self.state_path.as_path())?;
            device_id
        };

        let mut current = self.connector_task.lock().await;
        if let Some(handle) = current.take() {
            handle.abort();
        }
        let runtime = self.clone();
        *current = Some(tokio::spawn(async move {
            loop {
                let maybe_config = {
                    let state = runtime.state.read().await;
                    ClientConfig::from_state(&state, runtime.state_path.clone())
                };
                let Some(config) = maybe_config else {
                    break;
                };
                let device_id = {
                    let state = runtime.state.read().await;
                    state.device_id.clone().unwrap_or_else(|| device_id.clone())
                };
                if let Err(err) = connect_loop(
                    config,
                    runtime.state.clone(),
                    runtime.sandbox_runtime.clone(),
                    device_id,
                )
                .await
                {
                    tracing_stdout(format!("connector loop stopped: {err}").as_str());
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }));
        Ok(())
    }
}

impl CommandHistoryRecorder {
    async fn append(&self, entry: CommandHistoryEntry) {
        let mut state = self.state.write().await;
        state.command_history.push(entry);
        let overflow = state
            .command_history
            .len()
            .saturating_sub(MAX_COMMAND_HISTORY_ENTRIES);
        if overflow > 0 {
            state.command_history.drain(0..overflow);
        }
        if let Err(err) = state.save(self.state_path.as_path()) {
            tracing_stdout(format!("save command history failed: {err}").as_str());
        }
    }
}

impl CommandExecutionContext {
    fn terminal_exec(request: &RelayRequest) -> Self {
        Self {
            source: "chatos_terminal_exec".to_string(),
            request_id: Some(request.request_id.clone()),
            tool_name: None,
            terminal_session_id: None,
            sandbox_id: None,
        }
    }

    fn local_mcp(request: &RelayRequest, tool_name: &str) -> Self {
        Self {
            source: "local_mcp".to_string(),
            request_id: Some(request.request_id.clone()),
            tool_name: Some(tool_name.to_string()),
            terminal_session_id: None,
            sandbox_id: None,
        }
    }

    fn task_runner_sandbox(request: &RelayRequest, sandbox_id: &str, tool_name: &str) -> Self {
        Self {
            source: "task_runner_sandbox".to_string(),
            request_id: Some(request.request_id.clone()),
            tool_name: Some(tool_name.to_string()),
            terminal_session_id: None,
            sandbox_id: Some(sandbox_id.to_string()),
        }
    }
}

#[derive(Debug)]
struct LocalApiError {
    status: axum::http::StatusCode,
    message: String,
}

impl LocalApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for LocalApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<anyhow::Error> for LocalApiError {
    fn from(value: anyhow::Error) -> Self {
        Self::internal(value.to_string())
    }
}

async fn serve_local_api(runtime: LocalRuntime) -> Result<()> {
    let bind_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        optional_env("LOCAL_CONNECTOR_CORE_API_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(DEFAULT_LOCAL_API_PORT),
    );
    let app = Router::new()
        .route("/api/local/status", get(local_status))
        .route("/api/local/auth/login", post(local_login))
        .route("/api/local/auth/register", post(local_register))
        .route("/api/local/auth/logout", post(local_logout))
        .route("/api/local/fs/list", get(local_fs_list_handler))
        .route("/api/local/workspaces", post(local_add_workspace))
        .route(
            "/api/local/workspaces/{workspace_id}",
            delete(local_remove_workspace),
        )
        .route(
            "/api/local/commands",
            get(local_command_history).delete(local_clear_command_history),
        )
        .route("/api/local/docker/status", get(local_docker_status))
        .route("/api/local/sandbox/toggle", post(local_toggle_sandbox))
        .route("/api/local/sandbox/images", get(local_sandbox_images))
        .route(
            "/api/local/sandbox/images/jobs",
            get(local_sandbox_image_jobs),
        )
        .route("/api/local/sandbox/leases", get(local_sandbox_leases))
        .route(
            "/api/local/sandbox/images/initialize",
            post(local_initialize_sandbox_image),
        )
        .route("/api/local/terminal/exec", post(local_terminal_exec))
        .with_state(runtime)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing_stdout(format!("local connector core API listening on http://{bind_addr}").as_str());
    axum::serve(listener, app).await?;
    Ok(())
}

async fn local_status(State(runtime): State<LocalRuntime>) -> Result<Json<Value>, LocalApiError> {
    Ok(Json(status_payload(&runtime).await))
}

async fn local_command_history(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<CommandHistoryQuery>,
) -> Result<Json<Value>, LocalApiError> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_COMMAND_HISTORY_LIMIT)
        .clamp(1, MAX_COMMAND_HISTORY_ENTRIES);
    let source = query
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let state = runtime.state.read().await;
    let entries = state
        .command_history
        .iter()
        .rev()
        .filter(|entry| source.map(|source| entry.source == source).unwrap_or(true))
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(json!({ "entries": entries })))
}

async fn local_clear_command_history(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let mut state = runtime.state.write().await;
    state.command_history.clear();
    state.save(runtime.state_path.as_path())?;
    Ok(Json(json!({ "entries": [] })))
}

async fn local_login(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<LocalAuthRequest>,
) -> Result<Json<Value>, LocalApiError> {
    local_auth(runtime, req, false).await
}

async fn local_register(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<LocalAuthRequest>,
) -> Result<Json<Value>, LocalApiError> {
    local_auth(runtime, req, true).await
}

async fn local_auth(
    runtime: LocalRuntime,
    req: LocalAuthRequest,
    register: bool,
) -> Result<Json<Value>, LocalApiError> {
    let cloud_base_url = normalize_required(req.cloud_base_url.as_str(), "cloud_base_url")?;
    let user_service_base_url = normalize_optional(req.user_service_base_url.as_deref())
        .unwrap_or_else(|| DEFAULT_USER_SERVICE_BASE_URL.to_string());
    let username = normalize_required(req.username.as_str(), "username")?;
    let password = normalize_required(req.password.as_str(), "password")?;
    let endpoint = if register {
        "/api/auth/register"
    } else {
        "/api/auth/login"
    };
    let mut body = json!({
        "username": username,
        "password": password,
    });
    if register {
        body["display_name"] = normalize_optional(req.display_name.as_deref())
            .map(Value::String)
            .unwrap_or(Value::Null);
    }
    let response = runtime
        .http_client
        .post(api_url(user_service_base_url.as_str(), endpoint).as_str())
        .json(&body)
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    ensure_success(response.status(), "authenticate user")
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let login = response
        .json::<LoginResponse>()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    {
        let mut state = runtime.state.write().await;
        let pairing_changed = state.device_id.is_some()
            && !state.pairing_context_matches(cloud_base_url.as_str(), login.user.id.as_str());
        state.auth = Some(AuthState {
            cloud_base_url: cloud_base_url.clone(),
            user_service_base_url,
            access_token: login.token,
            device_name: normalize_optional(req.device_name.as_deref())
                .unwrap_or_else(default_device_name),
            user: Some(login.user.clone()),
        });
        state.paired_cloud_base_url = Some(cloud_base_url);
        state.paired_user_id = Some(login.user.id);
        if pairing_changed {
            state.device_id = None;
            state.device_public_key = None;
        }
        state.save(runtime.state_path.as_path())?;
    }
    runtime.sync_saved_workspaces_if_needed().await?;
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

async fn local_logout(State(runtime): State<LocalRuntime>) -> Result<Json<Value>, LocalApiError> {
    let disconnect = {
        let state = runtime.state.read().await;
        ClientConfig::from_state(&state, runtime.state_path.clone())
            .and_then(|config| state.device_id.clone().map(|device_id| (config, device_id)))
    };
    {
        let mut task = runtime.connector_task.lock().await;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }
    if let Some((config, device_id)) = disconnect {
        if let Err(err) = disconnect_device(&runtime.http_client, &config, device_id.as_str()).await
        {
            tracing_stdout(format!("mark local connector device offline failed: {err}").as_str());
        }
    }
    {
        let mut state = runtime.state.write().await;
        state.auth = None;
        state.sandbox.enabled = false;
        state.save(runtime.state_path.as_path())?;
    }
    Ok(Json(status_payload(&runtime).await))
}

async fn local_fs_list_handler(
    Query(query): Query<FsListQuery>,
) -> Result<Json<FsListResponse>, LocalApiError> {
    let path = normalize_optional(query.path.as_deref())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap_or_else(|| PathBuf::from("/")));
    let canonical = canonicalize_existing_dir(path.as_path())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let parent = canonical
        .parent()
        .map(|path| path.display().to_string())
        .filter(|parent| parent != &canonical.display().to_string());
    let mut entries = Vec::new();
    for entry in fs::read_dir(canonical.as_path())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?
    {
        let entry = entry.map_err(|err| LocalApiError::bad_request(err.to_string()))?;
        let metadata = entry
            .metadata()
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
        if metadata.is_dir() {
            entries.push(FsEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().display().to_string(),
                is_dir: true,
            });
        }
    }
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(Json(FsListResponse {
        path: canonical.display().to_string(),
        parent,
        entries,
    }))
}

async fn local_add_workspace(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<AddWorkspaceRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let workspace_path = PathBuf::from(normalize_required(req.path.as_str(), "path")?);
    let config = {
        let state = runtime.state.read().await;
        ClientConfig::from_state(&state, runtime.state_path.clone())
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?
    };
    {
        let mut state = runtime.state.write().await;
        let device_id = ensure_device_registered(&runtime.http_client, &config, &mut state).await?;
        let workspace_config = ClientConfig {
            workspace_alias: normalize_optional(req.alias.as_deref()),
            ..config.clone()
        };
        ensure_workspace_registered(
            &runtime.http_client,
            &workspace_config,
            &mut state,
            device_id.as_str(),
            workspace_path,
            false,
        )
        .await?;
        state.save(runtime.state_path.as_path())?;
    }
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

async fn local_remove_workspace(
    State(runtime): State<LocalRuntime>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<Json<Value>, LocalApiError> {
    let (cloud_base_url, access_token) = {
        let state = runtime.state.read().await;
        let auth = state
            .auth
            .as_ref()
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?;
        (auth.cloud_base_url.clone(), auth.access_token.clone())
    };
    let response = runtime
        .http_client
        .delete(
            api_url(
                cloud_base_url.as_str(),
                format!(
                    "/api/local-connectors/workspaces/{}",
                    urlencoding::encode(workspace_id.as_str())
                )
                .as_str(),
            )
            .as_str(),
        )
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
        ensure_success(response.status(), "delete workspace")
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    }
    {
        let mut state = runtime.state.write().await;
        state
            .workspaces
            .retain(|workspace| workspace.id != workspace_id);
        state.save(runtime.state_path.as_path())?;
    }
    Ok(Json(status_payload(&runtime).await))
}

async fn local_docker_status() -> Json<Value> {
    Json(docker_status().await)
}

async fn local_toggle_sandbox(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<ToggleSandboxRequest>,
) -> Result<Json<Value>, LocalApiError> {
    if req.enabled {
        ensure_docker_running()
            .await
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    }
    {
        let mut state = runtime.state.write().await;
        state.sandbox.enabled = req.enabled;
        state.save(runtime.state_path.as_path())?;
    }
    upsert_sandbox_pairings(&runtime, req.enabled).await?;
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

async fn local_sandbox_images(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    Ok(Json(local_sandbox_image_catalog(&runtime).await))
}

async fn local_sandbox_image_jobs(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    let jobs = runtime.sandbox_runtime.jobs.read().await.clone();
    Ok(Json(json!(jobs)))
}

async fn local_sandbox_leases(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    let leases = runtime
        .sandbox_runtime
        .leases
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(json!(leases)))
}

async fn local_initialize_sandbox_image(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<InitializeImageRequest>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    ensure_docker_running()
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let job = start_local_sandbox_image_job(
        &runtime,
        req.features,
        normalize_optional(req.custom_build_script.as_deref()),
    )
    .await
    .map_err(LocalApiError::bad_request)?;
    Ok(Json(json!(job)))
}

async fn ensure_local_sandbox_enabled(runtime: &LocalRuntime) -> Result<(), LocalApiError> {
    let state = runtime.state.read().await;
    if state.sandbox.enabled {
        Ok(())
    } else {
        Err(LocalApiError::bad_request("local sandbox is disabled"))
    }
}

fn local_sandbox_runtime_specs() -> Vec<Value> {
    json!([
        runtime_spec(
            "java",
            "JDK",
            "OpenJDK development tools",
            "21",
            &[
                ("8", "JDK 8", "Temurin JDK 8 LTS", false),
                ("11", "JDK 11", "Temurin JDK 11 LTS", false),
                ("17", "JDK 17", "Temurin JDK 17 LTS", false),
                ("21", "JDK 21", "Temurin JDK 21 LTS", true),
                ("25", "JDK 25", "Temurin JDK 25 LTS", false),
            ]
        ),
        runtime_spec(
            "node",
            "Node.js",
            "Node.js, npm, pnpm and yarn",
            "24",
            &[
                ("20", "Node.js 20", "Node.js 20 legacy line", false),
                ("22", "Node.js 22", "Node.js 22 LTS", false),
                ("24", "Node.js 24", "Node.js 24 LTS", true),
                ("26", "Node.js 26", "Node.js 26 current line", false),
            ]
        ),
        runtime_spec(
            "python",
            "Python",
            "Python interpreter, pip and venv tooling",
            "3.14",
            &[
                (
                    "3.10",
                    "Python 3.10",
                    "Python 3.10 security support line",
                    false
                ),
                (
                    "3.11",
                    "Python 3.11",
                    "Python 3.11 security support line",
                    false
                ),
                (
                    "3.12",
                    "Python 3.12",
                    "Python 3.12 security support line",
                    false
                ),
                (
                    "3.13",
                    "Python 3.13",
                    "Python 3.13 security support line",
                    false
                ),
                (
                    "3.14",
                    "Python 3.14",
                    "Python 3.14 active support line",
                    true
                ),
            ]
        ),
        runtime_spec(
            "rust",
            "Rust",
            "Rust toolchain",
            "stable",
            &[
                (
                    "1.85.1",
                    "Rust 1.85.1",
                    "Pinned Rust 1.85.1 toolchain",
                    false
                ),
                (
                    "1.88.0",
                    "Rust 1.88.0",
                    "Pinned Rust 1.88.0 toolchain",
                    false
                ),
                (
                    "1.92.0",
                    "Rust 1.92.0",
                    "Pinned Rust 1.92.0 toolchain",
                    false
                ),
                (
                    "1.96.1",
                    "Rust 1.96.1",
                    "Pinned Rust 1.96.1 toolchain",
                    false
                ),
                ("stable", "Stable", "Rust stable channel", true),
                ("beta", "Beta", "Rust beta channel", false),
                ("nightly", "Nightly", "Rust nightly channel", false),
            ]
        ),
        runtime_spec(
            "go",
            "Go",
            "Go toolchain",
            "1.26",
            &[
                ("1.22", "Go 1.22", "Go 1.22 toolchain", false),
                ("1.23", "Go 1.23", "Go 1.23 toolchain", false),
                ("1.24", "Go 1.24", "Go 1.24 toolchain", false),
                ("1.25", "Go 1.25", "Go 1.25 toolchain", false),
                ("1.26", "Go 1.26", "Go 1.26 toolchain", true),
            ]
        ),
        runtime_spec(
            "dotnet",
            ".NET",
            ".NET SDK for C# and F# projects",
            "10.0",
            &[
                ("8.0", ".NET 8", ".NET 8 LTS SDK", false),
                ("9.0", ".NET 9", ".NET 9 STS SDK", false),
                ("10.0", ".NET 10", ".NET 10 LTS SDK", true),
            ]
        ),
        runtime_spec(
            "php",
            "PHP",
            "PHP CLI runtime and Composer",
            "8.4",
            &[
                ("8.2", "PHP 8.2", "PHP 8.2 security support line", false),
                ("8.3", "PHP 8.3", "PHP 8.3 security support line", false),
                ("8.4", "PHP 8.4", "PHP 8.4 active support line", true),
                ("8.5", "PHP 8.5", "PHP 8.5 active support line", false),
            ]
        ),
        runtime_spec(
            "ruby",
            "Ruby",
            "Ruby runtime, RubyGems and Bundler",
            "3.4.10",
            &[
                ("3.2.11", "Ruby 3.2.11", "Ruby 3.2 maintenance line", false),
                ("3.3.11", "Ruby 3.3.11", "Ruby 3.3 maintenance line", false),
                ("3.4.10", "Ruby 3.4.10", "Ruby 3.4 stable line", true),
                ("4.0.5", "Ruby 4.0.5", "Ruby 4.0 current line", false),
            ]
        ),
        runtime_spec(
            "gcc",
            "C/C++ (GCC)",
            "GNU C and C++ compiler toolchain",
            "14",
            &[
                ("13", "GCC 13", "GNU C/C++ compiler 13", false),
                ("14", "GCC 14", "GNU C/C++ compiler 14", true),
            ]
        ),
        runtime_spec(
            "clang",
            "C/C++ (Clang)",
            "LLVM, Clang, LLD and Clangd toolchain",
            "20",
            &[
                ("18", "Clang 18", "LLVM/Clang 18 toolchain", false),
                ("19", "Clang 19", "LLVM/Clang 19 toolchain", false),
                ("20", "Clang 20", "LLVM/Clang 20 toolchain", true),
            ]
        ),
    ])
    .as_array()
    .cloned()
    .unwrap_or_default()
}

fn runtime_spec(
    id: &str,
    label: &str,
    description: &str,
    default_version: &str,
    versions: &[(&str, &str, &str, bool)],
) -> Value {
    json!({
        "id": id,
        "label": label,
        "description": description,
        "default_version": default_version,
        "versions": versions.iter().map(|(id, label, description, default)| json!({
            "id": id,
            "label": label,
            "description": description,
            "default": default,
        })).collect::<Vec<_>>()
    })
}

async fn local_sandbox_image_catalog(runtime: &LocalRuntime) -> Value {
    let jobs = runtime.sandbox_runtime.jobs.read().await.clone();
    let mut images = vec![json!({
        "id": "default",
        "name": DEFAULT_LOCAL_SANDBOX_IMAGE,
        "image_ref": DEFAULT_LOCAL_SANDBOX_IMAGE,
        "features": ["java@21", "node@24", "rust@stable", "go@1.26"],
        "backend": LOCAL_SANDBOX_BACKEND,
        "status": local_docker_image_status(DEFAULT_LOCAL_SANDBOX_IMAGE).await,
    })];
    for job in jobs.iter().filter(|job| job.status == "succeeded") {
        images.push(json!({
            "id": job.image_id,
            "name": job.image_name,
            "image_ref": job.image_ref,
            "features": job.features,
            "backend": job.backend,
            "status": "local",
            "created_at": job.created_at,
        }));
    }
    json!({
        "backend": LOCAL_SANDBOX_BACKEND,
        "default_image_id": "default",
        "image_tag_prefix": DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX,
        "features": local_sandbox_runtime_specs(),
        "images": images,
    })
}

async fn local_docker_image_status(image_ref: &str) -> &'static str {
    match tokio::process::Command::new("docker")
        .args(["image", "inspect", image_ref])
        .output()
        .await
    {
        Ok(output) if output.status.success() => "local",
        _ => "missing",
    }
}

async fn start_local_sandbox_image_job(
    runtime: &LocalRuntime,
    features: Vec<String>,
    custom_build_script: Option<String>,
) -> Result<LocalSandboxImageJob, String> {
    if custom_build_script
        .as_deref()
        .map(str::len)
        .unwrap_or_default()
        > 128 * 1024
    {
        return Err("custom build script is too large".to_string());
    }
    let features = normalize_local_sandbox_features(features)?;
    let image_id = local_sandbox_image_id(features.as_slice(), custom_build_script.as_deref());
    if let Some(existing) = runtime
        .sandbox_runtime
        .jobs
        .read()
        .await
        .iter()
        .find(|job| job.image_id == image_id && job.status == "running")
        .cloned()
    {
        return Ok(existing);
    }
    let image_name = image_id.trim_start_matches("local-").to_string();
    let image_ref = format!("{DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX}:{image_name}");
    let now = local_now_rfc3339();
    let job = LocalSandboxImageJob {
        id: format!("image-job-{}", Uuid::new_v4()),
        image_id,
        image_name,
        image_ref,
        features,
        backend: LOCAL_SANDBOX_BACKEND.to_string(),
        status: "running".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        started_at: Some(now),
        finished_at: None,
        output: String::new(),
        error: None,
        custom_build_script,
    };
    runtime.sandbox_runtime.jobs.write().await.push(job.clone());

    let jobs = runtime.sandbox_runtime.jobs.clone();
    let state = runtime.state.clone();
    let state_path = runtime.state_path.clone();
    let job_id = job.id.clone();
    tokio::spawn(async move {
        run_local_sandbox_image_job(jobs, state, state_path, job_id).await;
    });
    Ok(job)
}

async fn run_local_sandbox_image_job(
    jobs: Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    state: Arc<RwLock<LocalState>>,
    state_path: PathBuf,
    job_id: String,
) {
    let job = {
        let jobs_guard = jobs.read().await;
        jobs_guard.iter().find(|job| job.id == job_id).cloned()
    };
    let Some(job) = job else {
        return;
    };
    let context = local_sandbox_image_build_context();
    let dockerfile = local_sandbox_image_dockerfile(context.as_path());
    let mut command = tokio::process::Command::new("docker");
    command
        .arg("build")
        .arg("-t")
        .arg(job.image_ref.as_str())
        .arg("-f")
        .arg(dockerfile.as_path())
        .arg("--build-arg")
        .arg(format!("SANDBOX_FEATURES={}", job.features.join(",")));
    if let Some(script) = job.custom_build_script.as_deref() {
        command.arg("--build-arg").arg(format!(
            "SANDBOX_CUSTOM_SCRIPT_B64={}",
            general_purpose::STANDARD.encode(script.as_bytes())
        ));
    }
    command
        .arg(context.as_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    append_local_sandbox_job_output(
        &jobs,
        job_id.as_str(),
        format!(
            "[local connector] docker build -t {} -f {} {}\n",
            job.image_ref,
            dockerfile.display(),
            context.display()
        )
        .as_str(),
    )
    .await;

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let message = format!("start docker build failed: {err}");
            append_local_sandbox_job_output(
                &jobs,
                job_id.as_str(),
                format!("{message}\n").as_str(),
            )
            .await;
            finish_local_sandbox_image_job(&jobs, job_id.as_str(), "failed", Some(message)).await;
            return;
        }
    };

    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(read_local_sandbox_job_stream(
            stdout,
            jobs.clone(),
            job_id.clone(),
        ))
    });
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(read_local_sandbox_job_stream(
            stderr,
            jobs.clone(),
            job_id.clone(),
        ))
    });

    let wait_result = child.wait().await;
    let stdout = join_sandbox_log_task(stdout_task).await;
    let stderr = join_sandbox_log_task(stderr_task).await;
    let (status, error) = match wait_result {
        Ok(exit_status) if exit_status.success() => ("succeeded", None),
        Ok(exit_status) => {
            let details = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            (
                "failed",
                Some(format!(
                    "docker build failed with status {exit_status}: {details}"
                )),
            )
        }
        Err(err) => (
            "failed",
            Some(format!("wait docker build process failed: {err}")),
        ),
    };
    finish_local_sandbox_image_job(&jobs, job_id.as_str(), status, error).await;
    if status == "succeeded" {
        let mut state_guard = state.write().await;
        state_guard.sandbox.selected_image_ref = Some(job.image_ref);
        if let Err(err) = state_guard.save(state_path.as_path()) {
            tracing_stdout(format!("save selected local sandbox image failed: {err}").as_str());
        }
    }
}

async fn read_local_sandbox_job_stream<R>(
    mut reader: R,
    jobs: Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    job_id: String,
) -> String
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut collected = String::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                append_local_sandbox_job_output(&jobs, job_id.as_str(), chunk.as_str()).await;
                collected.push_str(chunk.as_str());
                collected = truncate_local_sandbox_job_output(collected.as_str());
            }
            Err(err) => {
                let message = format!("[local connector] read docker build log failed: {err}\n");
                append_local_sandbox_job_output(&jobs, job_id.as_str(), message.as_str()).await;
                break;
            }
        }
    }
    collected
}

async fn join_sandbox_log_task(task: Option<JoinHandle<String>>) -> String {
    let Some(task) = task else {
        return String::new();
    };
    match task.await {
        Ok(output) => output,
        Err(err) => {
            tracing_stdout(format!("read docker build log task failed: {err}").as_str());
            String::new()
        }
    }
}

async fn append_local_sandbox_job_output(
    jobs: &Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    job_id: &str,
    chunk: &str,
) {
    let mut jobs_guard = jobs.write().await;
    if let Some(stored) = jobs_guard.iter_mut().find(|job| job.id == job_id) {
        stored.output.push_str(chunk);
        stored.output = truncate_local_sandbox_job_output(stored.output.as_str());
        stored.updated_at = local_now_rfc3339();
    }
}

async fn finish_local_sandbox_image_job(
    jobs: &Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    job_id: &str,
    status: &str,
    error: Option<String>,
) {
    let mut jobs_guard = jobs.write().await;
    if let Some(stored) = jobs_guard.iter_mut().find(|job| job.id == job_id) {
        stored.status = status.to_string();
        stored.updated_at = local_now_rfc3339();
        stored.finished_at = Some(local_now_rfc3339());
        if stored.output.trim().is_empty() {
            if let Some(error) = error.as_deref() {
                stored.output = truncate_local_sandbox_job_output(error);
            }
        }
        stored.error = error;
    }
}

fn truncate_local_sandbox_job_output(value: &str) -> String {
    const MAX_JOB_OUTPUT_LEN: usize = 80_000;
    if value.len() <= MAX_JOB_OUTPUT_LEN {
        return value.to_string();
    }
    let start = value.len().saturating_sub(MAX_JOB_OUTPUT_LEN);
    let mut boundary = start;
    while boundary < value.len() && !value.is_char_boundary(boundary) {
        boundary += 1;
    }
    format!("... output truncated ...\n{}", &value[boundary..])
}

fn local_sandbox_image_build_context() -> PathBuf {
    optional_env("LOCAL_CONNECTOR_SANDBOX_IMAGE_BUILD_CONTEXT")
        .map(PathBuf::from)
        .or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn local_sandbox_image_dockerfile(context: &Path) -> PathBuf {
    optional_env("LOCAL_CONNECTOR_SANDBOX_IMAGE_DOCKERFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            context
                .join("sandbox_manager_service")
                .join("sandbox_agent")
                .join("Dockerfile")
        })
}

fn normalize_local_sandbox_features(features: Vec<String>) -> Result<Vec<String>, String> {
    let catalog = local_sandbox_runtime_specs();
    let mut allowed = BTreeSet::new();
    for feature in catalog {
        let Some(runtime) = feature.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(versions) = feature.get("versions").and_then(Value::as_array) else {
            continue;
        };
        for version in versions {
            if let Some(version) = version.get("id").and_then(Value::as_str) {
                allowed.insert(format!("{runtime}@{version}"));
            }
        }
    }
    let mut normalized = BTreeSet::new();
    for feature in features {
        let value = feature.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if !allowed.contains(value.as_str()) {
            return Err(format!("unsupported sandbox runtime version: {value}"));
        }
        normalized.insert(value);
    }
    Ok(normalized.into_iter().collect())
}

fn local_sandbox_image_id(features: &[String], custom_build_script: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    for feature in features {
        hasher.update(feature.as_bytes());
        hasher.update(b"\n");
    }
    if let Some(script) = custom_build_script {
        hasher.update(b"custom\n");
        hasher.update(script.as_bytes());
    }
    let digest = hex::encode(hasher.finalize());
    let feature_slug = if features.is_empty() {
        "base".to_string()
    } else {
        features
            .iter()
            .map(|feature| feature.replace('@', "-").replace('.', "_"))
            .collect::<Vec<_>>()
            .join("_")
    };
    format!("local-{feature_slug}-{}", &digest[..12])
}

async fn local_terminal_exec(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<LocalTerminalExecRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let (cloud_base_url, access_token, device_id) = {
        let state = runtime.state.read().await;
        let auth = state
            .auth
            .as_ref()
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?;
        let device_id = state
            .device_id
            .clone()
            .ok_or_else(|| LocalApiError::bad_request("device is not registered yet"))?;
        (
            auth.cloud_base_url.clone(),
            auth.access_token.clone(),
            device_id,
        )
    };
    let response = runtime
        .http_client
        .post(
            api_url(
                cloud_base_url.as_str(),
                format!(
                    "/api/local-connectors/relay/{}/terminal/exec",
                    urlencoding::encode(device_id.as_str())
                )
                .as_str(),
            )
            .as_str(),
        )
        .bearer_auth(access_token)
        .json(&json!({
            "workspace_id": req.workspace_id,
            "command": req.command,
            "args": req.args.unwrap_or_default(),
            "cwd": req.cwd,
            "timeout_ms": req.timeout_ms,
            "source": "local_connector_ui",
        }))
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    if !status.is_success() {
        return Err(LocalApiError::bad_gateway(body.to_string()));
    }
    Ok(Json(body))
}

impl LocalState {
    fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("read state file {}", path.display()))?;
        serde_json::from_str(content.as_str())
            .with_context(|| format!("parse state file {}", path.display()))
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create state dir {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).with_context(|| format!("write state file {}", path.display()))
    }

    fn workspace_by_id(&self, workspace_id: &str) -> Option<&WorkspaceState> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
    }

    fn workspace_index_by_fingerprint(&self, fingerprint: &str) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|workspace| workspace.fingerprint == fingerprint)
    }

    fn pairing_context_matches(&self, cloud_base_url: &str, user_id: &str) -> bool {
        let stored_cloud_base_url = self
            .paired_cloud_base_url
            .as_deref()
            .or_else(|| self.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()));
        let stored_user_id = self.paired_user_id.as_deref().or_else(|| {
            self.auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref().map(|user| user.id.as_str()))
        });
        matches!(
            (stored_cloud_base_url, stored_user_id),
            (Some(stored_cloud_base_url), Some(stored_user_id))
                if stored_cloud_base_url == cloud_base_url && stored_user_id == user_id
        )
    }
}

async fn ensure_device_registered(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &mut LocalState,
) -> Result<String> {
    if let Some(device_id) = state.device_id.clone() {
        return Ok(device_id);
    }

    let public_key = config
        .public_key
        .clone()
        .or_else(|| state.device_public_key.clone())
        .unwrap_or_else(|| format!("dev-public-key-{}", Uuid::new_v4()));
    let response = client
        .post(api_url(
            &config.cloud_base_url,
            "/api/local-connectors/devices",
        ))
        .bearer_auth(config.access_token.as_str())
        .json(&json!({
            "display_name": config.device_name,
            "public_key": public_key,
            "client_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
        }))
        .send()
        .await
        .context("register local connector device")?;
    ensure_success(response.status(), "register local connector device")?;
    let device = response
        .json::<DeviceResponse>()
        .await
        .context("parse device registration response")?;
    state.device_id = Some(device.id.clone());
    state.device_public_key = Some(public_key);
    Ok(device.id)
}

async fn ensure_workspace_registered(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &mut LocalState,
    device_id: &str,
    workspace_path: PathBuf,
    force_register: bool,
) -> Result<String> {
    let absolute_root = canonicalize_existing_dir(workspace_path.as_path())?;
    let fingerprint = workspace_fingerprint(absolute_root.as_path());
    let existing_index = state.workspace_index_by_fingerprint(fingerprint.as_str());
    if let Some(index) = existing_index {
        if !force_register {
            return Ok(state.workspaces[index].id.clone());
        }
    }
    let alias = config
        .workspace_alias
        .clone()
        .or_else(|| existing_index.map(|index| state.workspaces[index].alias.clone()))
        .unwrap_or_else(|| display_alias(absolute_root.as_path()));
    let response = client
        .post(api_url(
            &config.cloud_base_url,
            "/api/local-connectors/workspaces",
        ))
        .bearer_auth(config.access_token.as_str())
        .json(&json!({
            "device_id": device_id,
            "display_name": alias,
            "local_path_alias": alias,
            "local_path_fingerprint": fingerprint,
            "capabilities": ["mcp", "terminal", "sandbox"],
        }))
        .send()
        .await
        .context("register local connector workspace")?;
    ensure_success(response.status(), "register local connector workspace")?;
    let workspace = response
        .json::<WorkspaceResponse>()
        .await
        .context("parse workspace registration response")?;
    let workspace_state = WorkspaceState {
        id: workspace.id.clone(),
        absolute_root,
        alias: workspace.local_path_alias,
        fingerprint: workspace.local_path_fingerprint,
    };
    if let Some(index) = existing_index {
        state.workspaces[index] = workspace_state;
    } else {
        state.workspaces.push(workspace_state);
    }
    Ok(workspace.id)
}

async fn disconnect_device(
    client: &reqwest::Client,
    config: &ClientConfig,
    device_id: &str,
) -> Result<()> {
    let response = client
        .post(api_url(
            &config.cloud_base_url,
            format!(
                "/api/local-connectors/devices/{}/disconnect",
                urlencoding::encode(device_id)
            )
            .as_str(),
        ))
        .bearer_auth(config.access_token.as_str())
        .send()
        .await
        .context("mark local connector device offline")?;
    if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    ensure_success(response.status(), "mark local connector device offline")
}

async fn bootstrap_env_config(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &Arc<RwLock<LocalState>>,
) -> Result<()> {
    let mut state_guard = state.write().await;
    if state_guard.auth.is_none() {
        state_guard.auth = Some(AuthState {
            cloud_base_url: config.cloud_base_url.clone(),
            user_service_base_url: optional_env("LOCAL_CONNECTOR_USER_SERVICE_BASE_URL")
                .unwrap_or_else(|| DEFAULT_USER_SERVICE_BASE_URL.to_string()),
            access_token: config.access_token.clone(),
            device_name: config.device_name.clone(),
            user: None,
        });
    }
    let device_id = ensure_device_registered(client, config, &mut state_guard).await?;
    if let Some(workspace_path) = config.workspace_path.clone() {
        ensure_workspace_registered(
            client,
            config,
            &mut state_guard,
            &device_id,
            workspace_path,
            false,
        )
        .await?;
    }
    state_guard.save(config.state_path.as_path())?;
    Ok(())
}

async fn connect_loop(
    config: ClientConfig,
    state: Arc<RwLock<LocalState>>,
    sandbox_runtime: LocalSandboxRuntime,
    device_id: String,
) -> Result<()> {
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("build local adapter HTTP client")?;
    let ws_url = websocket_url(
        &config.cloud_base_url,
        format!("/api/local-connectors/devices/{device_id}/connect").as_str(),
        config.access_token.as_str(),
    );
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url.as_str())
        .await
        .with_context(|| format!("connect local connector websocket {ws_url}"))?;
    let (mut write, mut read) = ws_stream.split();
    let terminal_manager = LocalTerminalManager::default();
    let history_recorder = CommandHistoryRecorder {
        state_path: config.state_path.clone(),
        state: state.clone(),
    };
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Value>();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS));
    tracing_stdout("connected to local_connector_service");

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                write
                    .send(Message::Text(json!({"type": "heartbeat"}).to_string().into()))
                    .await
                    .context("send heartbeat")?;
            }
            outbound = outbound_rx.recv() => {
                let Some(outbound) = outbound else {
                    return Err(anyhow!("local connector outbound channel closed"));
                };
                write
                    .send(Message::Text(outbound.to_string().into()))
                    .await
                    .context("send relay event")?;
            }
            message = read.next() => {
                let Some(message) = message else {
                    return Err(anyhow!("local connector websocket closed"));
                };
                let message = message.context("read websocket message")?;
                match message {
                    Message::Text(text) => {
                        let state_snapshot = state.read().await.clone();
                        if let Some(response) =
                            handle_text_message(
                                text.as_str(),
                                &state_snapshot,
                                &config,
                                &http_client,
                                &sandbox_runtime,
                                &terminal_manager,
                                &history_recorder,
                                outbound_tx.clone(),
                            ).await
                        {
                            write
                                .send(Message::Text(response.to_string().into()))
                                .await
                                .context("send relay response")?;
                        }
                    }
                    Message::Ping(bytes) => {
                        write.send(Message::Pong(bytes)).await.context("send pong")?;
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

async fn handle_text_message(
    text: &str,
    state: &LocalState,
    _config: &ClientConfig,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    terminal_manager: &LocalTerminalManager,
    history_recorder: &CommandHistoryRecorder,
    outbound_tx: mpsc::UnboundedSender<Value>,
) -> Option<Value> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match message_type {
        "connected" | "pong" | "ack" => {
            tracing_stdout(format!("service message: {message_type}").as_str());
            None
        }
        "mcp_request" => Some(handle_mcp_request(value, state, history_recorder).await),
        "sandbox_request" => Some(
            handle_sandbox_request(value, state, http_client, sandbox_runtime, history_recorder)
                .await,
        ),
        "terminal_exec_request" => {
            Some(handle_terminal_exec_request(value, state, history_recorder).await)
        }
        "terminal_session_create_request" => Some(
            handle_terminal_session_create_request(value, state, terminal_manager, outbound_tx)
                .await,
        ),
        "terminal_input" => {
            handle_terminal_input(
                value,
                state,
                terminal_manager,
                history_recorder,
                outbound_tx,
            )
            .await;
            None
        }
        "terminal_resize" => {
            handle_terminal_resize(value, terminal_manager, outbound_tx).await;
            None
        }
        "terminal_snapshot_request" => {
            handle_terminal_snapshot_request(value, terminal_manager, outbound_tx).await;
            None
        }
        "terminal_close" => {
            handle_terminal_close(value, terminal_manager).await;
            None
        }
        _ => {
            tracing_stdout(format!("ignored service message: {message_type}").as_str());
            None
        }
    }
}

async fn handle_mcp_request(
    value: Value,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("mcp_response", "", 400, err.to_string());
        }
    };
    let body = match handle_mcp_body(&request, state, history_recorder).await {
        Ok(body) => body,
        Err(err) => {
            return RelayResponse {
                message_type: "mcp_response".to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    RelayResponse {
        message_type: "mcp_response".to_string(),
        request_id: request.request_id,
        status: 200,
        headers: BTreeMap::new(),
        body,
    }
    .to_value()
}

async fn handle_mcp_body(
    request: &RelayRequest,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let body = &request.body;
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match method {
        "initialize" => Ok(json!({
            "jsonrpc": "2.0",
            "id": body.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "local_connector",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "notifications/initialized" => Ok(json!({
            "jsonrpc": "2.0",
            "id": body.get("id").cloned().unwrap_or(Value::Null),
            "result": {}
        })),
        "ping" => Ok(json!({
            "jsonrpc": "2.0",
            "id": body.get("id").cloned().unwrap_or(Value::Null),
            "result": {}
        })),
        "local_connector/terminal/start" => {
            handle_local_mcp_terminal_start(request, state, history_recorder).await
        }
        "local_connector/terminal/cleanup" => {
            handle_local_mcp_terminal_cleanup(request, state).await
        }
        "tools/list" => Ok(json!({
            "jsonrpc": "2.0",
            "id": body.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "tools": local_mcp_builtin_compatible_tools(request, state)?
            }
        })),
        "tools/call" => handle_tool_call(request, state, history_recorder).await,
        _ => Ok(json!({
            "jsonrpc": "2.0",
            "id": body.get("id").cloned().unwrap_or(Value::Null),
            "error": {
                "code": -32601,
                "message": format!("unsupported local connector MCP method: {method}")
            }
        })),
    }
}

async fn handle_tool_call(
    request: &RelayRequest,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let body = &request.body;
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("tools/call missing params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(result) = call_builtin_compatible_local_tool(
        request,
        state,
        name,
        arguments.clone(),
        history_recorder,
    )
    .await?
    {
        return Ok(json!({
            "jsonrpc": "2.0",
            "id": body.get("id").cloned().unwrap_or(Value::Null),
            "result": result
        }));
    }
    Ok(json!({
        "jsonrpc": "2.0",
        "id": body.get("id").cloned().unwrap_or(Value::Null),
        "error": {
            "code": -32601,
            "message": format!("unsupported local connector tool: {name}")
        }
    }))
}

async fn handle_local_mcp_terminal_start(
    request: &RelayRequest,
    state: &LocalState,
    _history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    if !local_mcp_tool_selection(request).terminal {
        return Err(anyhow!(
            "local connector terminal tools are not enabled for this task"
        ));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let body = &request.body;
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    let requested_path = params.get("path").and_then(Value::as_str).unwrap_or(".");
    let normalized_path =
        normalize_request_project_relative_path(workspace, request, requested_path)?;
    let context = local_terminal_controller_context_for_root(
        project_root.as_path(),
        request,
        DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    );
    let payload = LocalConnectorTerminalControllerStore
        .start_shell_session(context, normalized_path)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(json!({
        "jsonrpc": "2.0",
        "id": body.get("id").cloned().unwrap_or(Value::Null),
        "result": payload
    }))
}

async fn handle_local_mcp_terminal_cleanup(
    request: &RelayRequest,
    state: &LocalState,
) -> Result<Value> {
    if !local_mcp_tool_selection(request).terminal {
        return Ok(json!({
            "jsonrpc": "2.0",
            "id": request.body.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "ok": true,
                "total": 0,
                "killed": 0,
                "already_exited": 0,
                "terminal_ids": [],
                "errors": [],
                "skipped": "terminal tools are not enabled for this task"
            }
        }));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let body = &request.body;
    let context = local_terminal_controller_context_for_root(
        project_root.as_path(),
        request,
        DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    );
    let payload = LocalConnectorTerminalControllerStore
        .kill_sessions_for_context(context)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(json!({
        "jsonrpc": "2.0",
        "id": body.get("id").cloned().unwrap_or(Value::Null),
        "result": payload
    }))
}

fn local_mcp_builtin_compatible_tools(
    request: &RelayRequest,
    state: &LocalState,
) -> Result<Vec<Value>> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let selection = local_mcp_tool_selection(request);
    let mut tools = Vec::new();
    if selection.code_read || selection.code_write {
        let code_service = code_maintainer_service_for_root(
            project_root.as_path(),
            Some(workspace.id.clone()),
            selection.code_write,
            selection.code_read,
            selection.code_write,
        )?;
        tools.extend(code_service.list_tools());
    }
    if selection.terminal {
        let terminal_service =
            local_terminal_controller_service_for_root(project_root.as_path(), request, 60_000)?;
        tools.extend(terminal_service.list_tools());
    }
    if selection.browser {
        let browser_service =
            local_browser_tools_service_for_root(project_root.as_path(), request)?;
        tools.extend(browser_service.list_tools());
    }
    Ok(tools)
}

async fn call_builtin_compatible_local_tool(
    request: &RelayRequest,
    state: &LocalState,
    name: &str,
    arguments: Value,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Option<Value>> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let selection = local_mcp_tool_selection(request);
    if is_code_maintainer_tool(name) {
        if !selection.allows_code_tool(name) {
            return Ok(None);
        }
        let project_root = request_project_root(workspace, request)?;
        let service = code_maintainer_service_for_root(
            project_root.as_path(),
            Some(workspace.id.clone()),
            selection.code_write,
            selection.code_read,
            selection.code_write,
        )?;
        let arguments = normalize_code_maintainer_arguments(workspace, request, name, arguments)?;
        let result = service
            .call_tool(name, arguments, None)
            .map_err(|err| anyhow!(err))?;
        return Ok(Some(result));
    }
    if is_terminal_controller_tool(name) {
        if !selection.terminal {
            return Ok(None);
        }
        let result = call_local_terminal_controller_tool(
            request,
            state,
            workspace,
            name,
            arguments,
            history_recorder,
        )
        .await?;
        return Ok(Some(result));
    }
    if is_browser_tool(name) {
        if !selection.browser {
            return Ok(None);
        }
        let project_root = request_project_root(workspace, request)?;
        let service = local_browser_tools_service_for_root(project_root.as_path(), request)?;
        let result = service
            .call_tool(
                name,
                arguments,
                Some(local_browser_conversation_id(request).as_str()),
            )
            .map_err(|err| anyhow!(err))?;
        return Ok(Some(result));
    }
    Ok(None)
}

fn is_code_maintainer_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file_raw"
            | "read_file_range"
            | "read_file"
            | "list_dir"
            | "search_text"
            | "search_files"
            | "write_file"
            | "edit_file"
            | "append_file"
            | "delete_path"
            | "apply_patch"
            | "patch"
    )
}

fn is_terminal_controller_tool(name: &str) -> bool {
    matches!(
        name,
        "execute_command"
            | "get_recent_logs"
            | "process_list"
            | "process_poll"
            | "process_log"
            | "process_wait"
            | "process_write"
            | "process_kill"
            | "process"
    )
}

fn is_browser_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_navigate"
            | "browser_snapshot"
            | "browser_click"
            | "browser_type"
            | "browser_scroll"
            | "browser_back"
            | "browser_press"
            | "browser_console"
            | "browser_get_images"
            | "browser_inspect"
            | "browser_research"
            | "browser_vision"
    )
}

#[derive(Debug, Clone, Copy)]
struct LocalMcpToolSelection {
    code_read: bool,
    code_write: bool,
    terminal: bool,
    browser: bool,
}

impl LocalMcpToolSelection {
    fn allows_code_tool(&self, name: &str) -> bool {
        if is_code_maintainer_write_tool(name) {
            return self.code_write;
        }
        is_code_maintainer_read_tool(name) && self.code_read
    }
}

fn local_mcp_tool_selection(request: &RelayRequest) -> LocalMcpToolSelection {
    let mut selection = LocalMcpToolSelection {
        code_read: false,
        code_write: false,
        terminal: false,
        browser: false,
    };
    let Some(raw) = relay_header(request, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER) else {
        return selection;
    };
    for token in raw.split([',', ';', '|', ' ']).map(str::trim) {
        match normalize_local_mcp_builtin_kind_token(token).as_str() {
            "codemaintainerread" => selection.code_read = true,
            "codemaintainerwrite" => {
                selection.code_read = true;
                selection.code_write = true;
            }
            "terminalcontroller" => selection.terminal = true,
            "browsertools" => selection.browser = true,
            _ => {}
        }
    }
    selection
}

fn relay_header<'a>(request: &'a RelayRequest, key: &str) -> Option<&'a str> {
    request
        .headers
        .get(key)
        .or_else(|| {
            request
                .headers
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                .map(|(_, value)| value)
        })
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_local_mcp_builtin_kind_token(token: &str) -> String {
    token
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_code_maintainer_read_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file_raw"
            | "read_file_range"
            | "read_file"
            | "list_dir"
            | "search_text"
            | "search_files"
    )
}

fn is_code_maintainer_write_tool(name: &str) -> bool {
    matches!(
        name,
        "write_file" | "edit_file" | "append_file" | "delete_path" | "apply_patch" | "patch"
    )
}

fn request_project_root(workspace: &WorkspaceState, request: &RelayRequest) -> Result<PathBuf> {
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let Some(cwd) = request_cwd(request) else {
        return Ok(root);
    };
    resolve_workspace_dir(workspace, normalize_relative_workspace_path(cwd)?.as_str())
}

fn code_maintainer_service_for_root(
    root: &Path,
    project_id: Option<String>,
    allow_writes: bool,
    enable_read_tools: bool,
    enable_write_tools: bool,
) -> Result<CodeMaintainerService> {
    CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "local_connector_code_maintainer".to_string(),
        root: root.to_path_buf(),
        project_id,
        allow_writes,
        max_file_bytes: MAX_LOCAL_MCP_READ_BYTES as i64,
        max_write_bytes: MAX_LOCAL_MCP_WRITE_BYTES as i64,
        search_limit: MAX_LOCAL_MCP_SEARCH_RESULTS,
        enable_read_tools,
        enable_write_tools,
        conversation_id: None,
        run_id: None,
        db_path: None,
        hooks: None,
    })
    .map_err(|err| anyhow!(err))
}

#[derive(Default)]
struct LocalBrowserToolsRegistry {
    services: StdMutex<HashMap<String, BrowserToolsService>>,
}

fn local_browser_tools_registry() -> &'static LocalBrowserToolsRegistry {
    static REGISTRY: OnceLock<LocalBrowserToolsRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LocalBrowserToolsRegistry::default)
}

fn local_browser_tools_service_for_root(
    root: &Path,
    request: &RelayRequest,
) -> Result<BrowserToolsService> {
    let root = canonicalize_existing_dir(root)?;
    let key = local_browser_tools_registry_key(root.as_path(), request);
    {
        let registry = local_browser_tools_registry()
            .services
            .lock()
            .map_err(|_| anyhow!("local browser tools registry is poisoned"))?;
        if let Some(service) = registry.get(key.as_str()) {
            return Ok(service.clone());
        }
    }
    let service = BrowserToolsService::new(BrowserToolsOptions {
        server_name: "local_connector_browser_tools".to_string(),
        workspace_dir: root,
        command_timeout_seconds: 30,
        max_snapshot_chars: 8_000,
        vision_adapter: None,
    })
    .map_err(|err| anyhow!(err))?;
    let mut registry = local_browser_tools_registry()
        .services
        .lock()
        .map_err(|_| anyhow!("local browser tools registry is poisoned"))?;
    Ok(registry
        .entry(key)
        .or_insert_with(|| service.clone())
        .clone())
}

fn local_browser_tools_registry_key(root: &Path, request: &RelayRequest) -> String {
    let user = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous");
    let project =
        local_mcp_terminal_project_id(request).unwrap_or_else(|| request.workspace_id.clone());
    format!(
        "{}|{}|{}",
        user,
        project,
        root.to_string_lossy().replace('\\', "/")
    )
}

fn local_browser_conversation_id(request: &RelayRequest) -> String {
    local_mcp_terminal_project_id(request).unwrap_or_else(|| request.workspace_id.clone())
}

fn normalize_request_project_relative_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<String> {
    let workspace_relative =
        normalize_request_workspace_relative_path(workspace, request, requested)?;
    let Some(base) = request_cwd(request)
        .map(normalize_relative_workspace_path)
        .transpose()?
        .filter(|value| value != ".")
    else {
        return Ok(workspace_relative);
    };
    if workspace_relative == base {
        return Ok(".".to_string());
    }
    workspace_relative
        .strip_prefix(format!("{base}/").as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path is outside current local project"))
}

fn normalize_code_maintainer_arguments(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    tool_name: &str,
    mut arguments: Value,
) -> Result<Value> {
    if matches!(tool_name, "apply_patch" | "patch") {
        return Ok(arguments);
    }
    let Some(map) = arguments.as_object_mut() else {
        return Ok(arguments);
    };
    if let Some(path) = map.get("path").and_then(Value::as_str) {
        let normalized = normalize_request_project_relative_path(workspace, request, path)?;
        map.insert("path".to_string(), Value::String(normalized));
    }
    Ok(arguments)
}

async fn call_local_terminal_controller_tool(
    request: &RelayRequest,
    state: &LocalState,
    workspace: &WorkspaceState,
    tool_name: &str,
    mut arguments: Value,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let timeout_ms = arguments
        .get("timeout_ms")
        .or_else(|| arguments.get("max_wait_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);
    let project_root = request_project_root(workspace, request)?;
    let normalized_path = if tool_name == "execute_command" {
        let path = arguments.get("path").and_then(Value::as_str).unwrap_or(".");
        let normalized_path = normalize_request_project_relative_path(workspace, request, path)?;
        if let Some(map) = arguments.as_object_mut() {
            map.insert("path".to_string(), Value::String(normalized_path.clone()));
        }
        Some(normalized_path)
    } else {
        None
    };
    let service =
        local_terminal_controller_service_for_root(project_root.as_path(), request, timeout_ms)?;
    let result = service
        .call_tool(tool_name, arguments, None)
        .map_err(|err| anyhow!(err))?;
    if tool_name != "execute_command" {
        return Ok(result);
    }

    let structured = code_maintainer_structured_result(result.clone());
    let command = structured
        .get("common")
        .or_else(|| structured.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("execute_command");
    let cwd_label = structured
        .get("path")
        .and_then(Value::as_str)
        .and_then(|path| {
            Path::new(path)
                .strip_prefix(project_root.as_path())
                .ok()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        })
        .filter(|value| !value.is_empty())
        .or(normalized_path)
        .unwrap_or_else(|| ".".to_string());
    let history_body = json!({
        "command": command,
        "args": [],
        "cwd": cwd_label,
        "success": structured.get("success").and_then(Value::as_bool).unwrap_or(false),
        "exit_code": structured.get("exit_code").and_then(Value::as_i64),
        "timed_out": structured.get("timed_out").and_then(Value::as_bool).unwrap_or(false),
        "stdout": structured
            .get("stdout")
            .or_else(|| structured.get("output"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "stderr": structured.get("stderr").and_then(Value::as_str).unwrap_or_default(),
    });
    history_recorder
        .append(command_history_entry_from_exec_result(
            state,
            request,
            &CommandExecutionContext::local_mcp(request, "execute_command"),
            command,
            &[],
            history_body
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or("."),
            local_now_rfc3339(),
            &history_body,
        ))
        .await;
    Ok(result)
}

fn code_maintainer_structured_result(result: Value) -> Value {
    if let Some(payload) = result.get("_structured_result") {
        return payload.clone();
    }
    if let Some(text) = result.pointer("/content/0/text").and_then(Value::as_str) {
        return serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({ "text": text }));
    }
    result
}

#[derive(Debug, Default)]
struct LocalConnectorTerminalControllerStore;

#[derive(Default)]
struct LocalMcpTerminalRegistry {
    sessions: RwLock<HashMap<String, Arc<LocalMcpTerminalSession>>>,
}

struct LocalMcpTerminalSession {
    meta: Mutex<LocalMcpTerminalMeta>,
    child: Mutex<tokio::process::Child>,
    stdin: Mutex<Option<tokio::process::ChildStdin>>,
    logs: Mutex<Vec<LocalMcpTerminalLog>>,
    command_lock: Mutex<()>,
    active_shell_marker: Mutex<Option<String>>,
}

#[derive(Debug, Clone)]
struct LocalMcpTerminalMeta {
    id: String,
    root: String,
    cwd: String,
    project_id: Option<String>,
    user_id: Option<String>,
    command: String,
    started_at: String,
    last_active_at: String,
    finished_at: Option<String>,
    status: String,
    exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
struct LocalMcpTerminalLog {
    offset: i64,
    kind: String,
    content: String,
    created_at: String,
}

#[derive(Debug)]
struct LocalMcpTerminalWaitResult {
    waited_ms: u64,
    busy: bool,
    timed_out: bool,
    finished_by: &'static str,
    exit_code: Option<i32>,
}

#[derive(Debug)]
struct LocalMcpTerminalOutput {
    text: String,
    char_count: usize,
    truncated: bool,
}

fn local_mcp_terminal_registry() -> &'static LocalMcpTerminalRegistry {
    static REGISTRY: OnceLock<LocalMcpTerminalRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LocalMcpTerminalRegistry::default)
}

impl LocalConnectorTerminalControllerStore {
    async fn start_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> std::result::Result<Value, String> {
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let session = self.ensure_shell_session(context, path).await?;
        let meta = session.meta.lock().await.clone();
        let display_project_root =
            display_local_mcp_workspace_path(project_root.as_path(), project_root.as_path());
        let display_cwd =
            display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        Ok(json!({
            "project_root": display_project_root,
            "terminal_id": meta.id,
            "process_id": meta.id,
            "path": display_cwd,
            "command": meta.command,
            "background": true,
            "busy": meta.status != "exited",
            "status": meta.status,
            "started_at": meta.started_at,
            "project_id": meta.project_id,
            "user_id": meta.user_id,
        }))
    }

    async fn ensure_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
        if let Some(session) = find_local_mcp_primary_shell_session(&context).await? {
            return Ok(session);
        }
        self.spawn_shell_session(context, path).await
    }

    async fn spawn_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let cwd = resolve_terminal_controller_cwd(project_root.as_path(), path.as_str())?;
        let shell = select_local_shell();
        let mut child = shell_session_for_terminal_controller(shell.as_str());
        child
            .current_dir(cwd.as_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = child.spawn().map_err(|err| err.to_string())?;
        let session = register_local_mcp_terminal_session(
            context.clone(),
            project_root,
            cwd,
            format!("task terminal shell: {shell}"),
            child,
        )
        .await?;
        append_local_mcp_terminal_log(
            session.clone(),
            "system",
            "[task terminal shell started]\n".to_string(),
        )
        .await;
        Ok(session)
    }

    async fn kill_sessions_for_context(
        &self,
        context: TerminalControllerContext,
    ) -> std::result::Result<Value, String> {
        let sessions = local_mcp_sessions_for_context(&context).await?;
        let total = sessions.len();
        let mut killed = 0usize;
        let mut already_exited = 0usize;
        let mut errors = Vec::new();
        let mut terminal_ids = Vec::new();

        for session in sessions {
            if let Err(err) = refresh_local_mcp_terminal_session_status(&session).await {
                errors.push(err);
                continue;
            }
            let meta = session.meta.lock().await.clone();
            terminal_ids.push(meta.id.clone());
            if meta.status == "exited" {
                already_exited += 1;
                continue;
            }
            {
                let mut child = session.child.lock().await;
                if let Err(err) = child.kill().await {
                    errors.push(format!("kill {} failed: {}", meta.id, err));
                    continue;
                }
                let _ = child.wait().await;
            }
            mark_local_mcp_terminal_exited(&session, None).await;
            append_local_mcp_terminal_log(
                session.clone(),
                "system",
                "[task terminal cleanup killed process]\n".to_string(),
            )
            .await;
            killed += 1;
        }

        if !terminal_ids.is_empty() {
            let mut registry = local_mcp_terminal_registry().sessions.write().await;
            for terminal_id in &terminal_ids {
                registry.remove(terminal_id);
            }
        }

        Ok(json!({
            "ok": errors.is_empty(),
            "total": total,
            "killed": killed,
            "already_exited": already_exited,
            "terminal_ids": terminal_ids,
            "errors": errors,
        }))
    }
}

#[async_trait::async_trait]
impl TerminalControllerStore for LocalConnectorTerminalControllerStore {
    async fn execute_command(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
    ) -> std::result::Result<Value, String> {
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let cwd = resolve_terminal_controller_cwd(project_root.as_path(), path.as_str())?;
        let display_project_root =
            display_local_mcp_workspace_path(project_root.as_path(), project_root.as_path());
        let display_cwd = display_local_mcp_workspace_path(project_root.as_path(), cwd.as_path());

        if background || cfg!(windows) {
            return execute_local_mcp_standalone_command(
                context,
                project_root,
                cwd,
                display_project_root,
                display_cwd,
                command,
                background,
                if background {
                    Some("background")
                } else {
                    Some("windows")
                },
            )
            .await;
        }

        let session = self
            .ensure_shell_session(context.clone(), path.clone())
            .await?;
        if local_mcp_shell_session_is_busy(&session).await {
            return execute_local_mcp_standalone_command(
                context,
                project_root,
                cwd,
                display_project_root,
                display_cwd,
                command,
                false,
                Some("primary_terminal_busy"),
            )
            .await;
        }

        execute_local_mcp_reused_shell_command(
            context,
            session,
            project_root,
            cwd,
            display_project_root,
            display_cwd,
            command,
        )
        .await
    }

    async fn get_recent_logs(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> std::result::Result<Value, String> {
        let sessions = local_mcp_sessions_for_context(&context).await?;
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let total = sessions.len();
        let mut terminals = Vec::new();
        for session in sessions.into_iter().take(terminal_limit) {
            refresh_local_mcp_terminal_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            let logs = session.logs.lock().await;
            let recent = take_recent_local_mcp_logs(&logs, per_terminal_limit.max(1) as usize);
            let cwd = display_local_mcp_workspace_path(
                project_root.as_path(),
                Path::new(meta.cwd.as_str()),
            );
            terminals.push(json!({
                "terminal_id": meta.id,
                "terminal_name": derive_local_mcp_terminal_name(cwd.as_str()),
                "status": meta.status,
                "cwd": cwd,
                "project_id": meta.project_id,
                "last_active_at": meta.last_active_at,
                "log_count": logs.len(),
                "returned_log_count": recent.len(),
                "truncated": false,
                "truncation": { "truncated": false },
                "logs": recent,
            }));
        }
        Ok(json!({
            "result_scope": if terminals.len() > 1 { "multiple_terminals" } else if terminals.is_empty() { "no_terminal" } else { "single_terminal" },
            "is_multiple_terminals": terminals.len() > 1,
            "terminal_count": terminals.len(),
            "total_terminals": total,
            "per_terminal_limit": per_terminal_limit,
            "terminal_limit": terminal_limit,
            "terminals": terminals,
        }))
    }
    async fn process_list(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> std::result::Result<Value, String> {
        let sessions = local_mcp_sessions_for_context(&context).await?;
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let mut processes = Vec::new();
        for session in sessions {
            refresh_local_mcp_terminal_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            if !include_exited && meta.status == "exited" {
                continue;
            }
            let busy = if is_local_mcp_primary_shell_command(meta.command.as_str()) {
                local_mcp_shell_session_is_busy(&session).await
            } else {
                meta.status != "exited"
            };
            let output = collect_local_mcp_terminal_output(&session, 1200).await;
            let cwd = display_local_mcp_workspace_path(
                project_root.as_path(),
                Path::new(meta.cwd.as_str()),
            );
            processes.push(json!({
                "terminal_id": meta.id,
                "process_id": meta.id,
                "terminal_name": derive_local_mcp_terminal_name(cwd.as_str()),
                "status": meta.status,
                "process_status": if meta.status == "exited" { "exited" } else if busy { "running" } else { "idle" },
                "busy": busy,
                "has_session": true,
                "command": meta.command,
                "pid": Value::Null,
                "started_at": meta.started_at,
                "uptime_seconds": Value::Null,
                "cwd": cwd,
                "project_id": meta.project_id,
                "last_active_at": meta.last_active_at,
                "output_preview": output.text,
                "output_tail": output.text,
                "output_tail_chars": output.char_count,
                "exit_code": meta.exit_code,
            }));
            if processes.len() >= limit {
                break;
            }
        }
        Ok(json!({
            "status": "ok",
            "result_scope": if processes.len() > 1 { "multiple_terminals" } else if processes.is_empty() { "no_terminal" } else { "single_terminal" },
            "is_multiple_terminals": processes.len() > 1,
            "terminal_count": processes.len(),
            "process_count": processes.len(),
            "visible_total": processes.len(),
            "total_terminals": processes.len(),
            "include_exited": include_exited,
            "limit": limit,
            "terminals": processes.clone(),
            "processes": processes,
        }))
    }

    async fn process_poll(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> std::result::Result<Value, String> {
        let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
        refresh_local_mcp_terminal_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        let busy = if is_local_mcp_primary_shell_command(meta.command.as_str()) {
            local_mcp_shell_session_is_busy(&session).await
        } else {
            meta.status != "exited"
        };
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let cwd =
            display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        let logs = session.logs.lock().await;
        let effective_limit = limit.clamp(1, 200) as usize;
        let selected = select_local_mcp_logs(&logs, offset, effective_limit);
        let output = collect_local_mcp_output_from_logs(
            selected
                .iter()
                .filter_map(|value| value.get("content").and_then(Value::as_str)),
            1200,
        );
        Ok(json!({
            "terminal_id": meta.id,
            "process_id": meta.id,
            "terminal_name": derive_local_mcp_terminal_name(cwd.as_str()),
            "status": meta.status,
            "process_status": if meta.status == "exited" { "exited" } else if busy { "running" } else { "idle" },
            "busy": busy,
            "has_session": true,
            "command": meta.command,
            "pid": Value::Null,
            "started_at": meta.started_at,
            "uptime_seconds": Value::Null,
            "cwd": cwd,
            "project_id": meta.project_id,
            "last_active_at": meta.last_active_at,
            "mode": if offset.is_some() { "offset" } else { "recent" },
            "requested_offset": offset,
            "next_offset": selected.last().and_then(|value| value.get("offset")).and_then(Value::as_i64).map(|value| value + 1),
            "limit": effective_limit,
            "fetched_log_count": selected.len(),
            "returned_log_count": selected.len(),
            "has_more": offset.is_some() && logs.len() > selected.len(),
            "truncated": false,
            "truncation": { "truncated": false },
            "logs": selected,
            "output_preview": output.text,
            "output_tail": output.text,
            "output_tail_chars": output.char_count,
            "exit_code": meta.exit_code,
        }))
    }

    async fn process_log(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> std::result::Result<Value, String> {
        let poll = self
            .process_poll(context, terminal_id, offset, limit)
            .await?;
        let output = poll
            .get("logs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| value.get("content").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        Ok(json!({
            "terminal_id": poll.get("terminal_id").cloned().unwrap_or(Value::Null),
            "status": poll.get("status").cloned().unwrap_or(Value::String("unknown".to_string())),
            "output": output,
            "offset": offset,
            "limit": limit,
            "has_more": poll.get("has_more").cloned().unwrap_or(Value::Bool(false)),
            "next_offset": poll.get("next_offset").cloned().unwrap_or(Value::Null),
        }))
    }

    async fn process_wait(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> std::result::Result<Value, String> {
        let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
        let result = wait_for_local_mcp_terminal_session(session.clone(), timeout_ms).await?;
        let output = collect_local_mcp_terminal_output(&session, context.max_output_chars).await;
        let meta = session.meta.lock().await.clone();
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let cwd =
            display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        Ok(json!({
            "terminal_id": meta.id,
            "process_id": meta.id,
            "terminal_name": derive_local_mcp_terminal_name(cwd.as_str()),
            "status": meta.status,
            "wait_status": if result.timed_out { "timeout" } else if meta.status == "exited" { "exited" } else { "running" },
            "busy": result.busy,
            "exited": meta.status == "exited",
            "completed": !result.timed_out,
            "timed_out": result.timed_out,
            "finished_by": result.finished_by,
            "exit_code": result.exit_code,
            "timeout_ms": timeout_ms,
            "waited_ms": result.waited_ms,
            "output": output.text,
            "output_preview": output.text,
            "output_chars": output.char_count,
            "truncated": output.truncated,
        }))
    }

    async fn process_write(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> std::result::Result<Value, String> {
        let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
        refresh_local_mcp_terminal_session_status(&session).await?;
        {
            let mut stdin = session.stdin.lock().await;
            let Some(stdin) = stdin.as_mut() else {
                return Err("terminal stdin is unavailable".to_string());
            };
            stdin
                .write_all(data.as_bytes())
                .await
                .map_err(|err| err.to_string())?;
            if submit {
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(|err| err.to_string())?;
            }
            stdin.flush().await.map_err(|err| err.to_string())?;
        }
        let mut content = data.clone();
        if submit {
            content.push('\n');
        }
        append_local_mcp_terminal_log(session, "input", content).await;
        Ok(json!({
            "ok": true,
            "terminal_id": terminal_id,
            "bytes_written": data.len() + usize::from(submit),
            "submit": submit,
        }))
    }

    async fn process_kill(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
    ) -> std::result::Result<Value, String> {
        let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
        {
            let mut child = session.child.lock().await;
            child.kill().await.map_err(|err| err.to_string())?;
            let _ = child.wait().await;
        }
        mark_local_mcp_terminal_exited(&session, None).await;
        append_local_mcp_terminal_log(session, "system", "[terminal killed]\n".to_string()).await;
        Ok(json!({
            "ok": true,
            "terminal_id": terminal_id,
            "killed": true,
        }))
    }
}

async fn execute_local_mcp_standalone_command(
    context: TerminalControllerContext,
    project_root: PathBuf,
    cwd: PathBuf,
    display_project_root: String,
    display_cwd: String,
    command: String,
    background: bool,
    reuse_skipped_reason: Option<&str>,
) -> std::result::Result<Value, String> {
    let mut child = shell_command_for_terminal_controller(command.as_str());
    child
        .current_dir(cwd.as_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = child.spawn().map_err(|err| err.to_string())?;
    let session = register_local_mcp_terminal_session(
        context.clone(),
        project_root.clone(),
        cwd.clone(),
        command.clone(),
        child,
    )
    .await?;
    append_local_mcp_terminal_log(session.clone(), "command", command.clone()).await;
    let session_id = session.meta.lock().await.id.clone();

    if background {
        let mut response = json!({
            "project_id": context.project_id,
            "project_root": display_project_root,
            "terminal_id": session_id.clone(),
            "process_id": session_id,
            "terminal_reused": false,
            "path": display_cwd,
            "common": command,
            "background": true,
            "busy": true,
            "output": "",
            "output_chars": 0,
            "truncated": false,
            "finished_by": "background",
            "idle_timeout_ms": context.idle_timeout_ms,
            "max_wait_ms": context.max_wait_ms,
            "max_output_chars": context.max_output_chars
        });
        if let Some(reason) = reuse_skipped_reason {
            if let Some(map) = response.as_object_mut() {
                map.insert(
                    "terminal_reuse_skipped_reason".to_string(),
                    Value::String(reason.to_string()),
                );
            }
        }
        return Ok(response);
    }

    let wait_result =
        wait_for_local_mcp_terminal_session(session.clone(), context.max_wait_ms).await?;
    let stdout =
        collect_local_mcp_terminal_output_by_kinds(&session, context.max_output_chars, &["stdout"])
            .await;
    let stderr =
        collect_local_mcp_terminal_output_by_kinds(&session, context.max_output_chars, &["stderr"])
            .await;
    let output = collect_local_mcp_terminal_output_by_kinds(
        &session,
        context.max_output_chars,
        &["stdout", "stderr"],
    )
    .await;
    let mut response = json!({
        "project_id": context.project_id,
        "project_root": display_project_root,
        "terminal_id": session_id.clone(),
        "process_id": session_id,
        "terminal_reused": false,
        "path": display_cwd,
        "common": command,
        "background": false,
        "busy": wait_result.busy,
        "success": wait_result.exit_code == Some(0),
        "stdout": stdout.text,
        "stderr": stderr.text,
        "output": output.text,
        "output_chars": output.char_count,
        "truncated": output.truncated,
        "finished_by": wait_result.finished_by,
        "exit_code": wait_result.exit_code,
        "idle_timeout_ms": context.idle_timeout_ms,
        "max_wait_ms": context.max_wait_ms,
        "max_output_chars": context.max_output_chars
    });
    if let Some(reason) = reuse_skipped_reason {
        if let Some(map) = response.as_object_mut() {
            map.insert(
                "terminal_reuse_skipped_reason".to_string(),
                Value::String(reason.to_string()),
            );
        }
    }
    Ok(response)
}

async fn execute_local_mcp_reused_shell_command(
    context: TerminalControllerContext,
    session: Arc<LocalMcpTerminalSession>,
    _project_root: PathBuf,
    cwd: PathBuf,
    display_project_root: String,
    display_cwd: String,
    command: String,
) -> std::result::Result<Value, String> {
    let _guard = session.command_lock.lock().await;
    refresh_local_mcp_terminal_session_status(&session).await?;
    {
        let meta = session.meta.lock().await;
        if meta.status == "exited" {
            return Err("primary terminal has exited".to_string());
        }
    }
    {
        let mut active = session.active_shell_marker.lock().await;
        if active.is_some() {
            return Err(
                "primary terminal is busy; run long commands with background=true".to_string(),
            );
        }
        let marker = format!("__CHATO_LOCAL_CMD_DONE_{}__", Uuid::new_v4().simple());
        *active = Some(marker);
    }

    let active_marker = session
        .active_shell_marker
        .lock()
        .await
        .clone()
        .ok_or_else(|| "primary terminal marker is unavailable".to_string())?;
    let start_marker = active_marker.replace("_DONE_", "_START_");
    {
        let mut meta = session.meta.lock().await;
        meta.cwd = cwd.to_string_lossy().to_string();
        meta.last_active_at = local_now_rfc3339();
    }
    append_local_mcp_terminal_log(session.clone(), "command", command.clone()).await;
    let output_start_offset = next_local_mcp_log_offset(&session).await;
    let script = build_local_mcp_shell_command_script(
        cwd.as_path(),
        command.as_str(),
        start_marker.as_str(),
        active_marker.as_str(),
    );
    let write_result = async {
        let mut stdin = session.stdin.lock().await;
        let Some(stdin) = stdin.as_mut() else {
            return Err("primary terminal stdin is unavailable".to_string());
        };
        stdin
            .write_all(script.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
        stdin.flush().await.map_err(|err| err.to_string())
    }
    .await;
    if let Err(err) = write_result {
        clear_local_mcp_shell_active_marker(&session, active_marker.as_str()).await;
        return Err(err);
    }

    let wait_result = wait_for_local_mcp_shell_command(
        session.clone(),
        active_marker.as_str(),
        context.max_wait_ms,
    )
    .await?;
    if wait_result.timed_out {
        spawn_clear_local_mcp_shell_active_marker_when_done(session.clone(), active_marker.clone());
    } else {
        clear_local_mcp_shell_active_marker(&session, active_marker.as_str()).await;
    }

    let stdout = collect_local_mcp_terminal_output_since_by_kinds(
        &session,
        output_start_offset,
        context.max_output_chars,
        &["stdout"],
    )
    .await;
    let stderr = collect_local_mcp_terminal_output_since_by_kinds(
        &session,
        output_start_offset,
        context.max_output_chars,
        &["stderr"],
    )
    .await;
    let output = collect_local_mcp_terminal_output_since_by_kinds(
        &session,
        output_start_offset,
        context.max_output_chars,
        &["stdout", "stderr"],
    )
    .await;
    let session_id = session.meta.lock().await.id.clone();
    Ok(json!({
        "project_id": context.project_id,
        "project_root": display_project_root,
        "terminal_id": session_id.clone(),
        "process_id": session_id,
        "terminal_reused": true,
        "path": display_cwd,
        "common": command,
        "background": false,
        "busy": wait_result.busy,
        "success": wait_result.exit_code == Some(0),
        "stdout": stdout.text,
        "stderr": stderr.text,
        "output": output.text,
        "output_chars": output.char_count,
        "truncated": output.truncated,
        "finished_by": wait_result.finished_by,
        "exit_code": wait_result.exit_code,
        "idle_timeout_ms": context.idle_timeout_ms,
        "max_wait_ms": context.max_wait_ms,
        "max_output_chars": context.max_output_chars
    }))
}

async fn find_local_mcp_primary_shell_session(
    context: &TerminalControllerContext,
) -> std::result::Result<Option<Arc<LocalMcpTerminalSession>>, String> {
    let sessions = local_mcp_sessions_for_context(context).await?;
    for session in sessions {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let is_match = {
            let meta = session.meta.lock().await;
            meta.status != "exited" && is_local_mcp_primary_shell_command(meta.command.as_str())
        };
        if is_match {
            return Ok(Some(session));
        }
    }
    Ok(None)
}

fn is_local_mcp_primary_shell_command(command: &str) -> bool {
    command.starts_with("task terminal shell:")
}

async fn local_mcp_shell_session_is_busy(session: &Arc<LocalMcpTerminalSession>) -> bool {
    session.active_shell_marker.lock().await.is_some()
}

async fn wait_for_local_mcp_shell_command(
    session: Arc<LocalMcpTerminalSession>,
    done_marker: &str,
    timeout_ms: u64,
) -> std::result::Result<LocalMcpTerminalWaitResult, String> {
    let timeout = Duration::from_millis(timeout_ms.clamp(1_000, 600_000));
    let started = std::time::Instant::now();
    loop {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        if meta.status == "exited" {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "exit",
                exit_code: meta.exit_code,
            });
        }
        if let Some(exit_code) = local_mcp_shell_done_exit_code(&session, done_marker).await {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "sentinel",
                exit_code: Some(exit_code),
            });
        }
        if started.elapsed() >= timeout {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(LocalMcpTerminalWaitResult {
        waited_ms: started.elapsed().as_millis() as u64,
        busy: true,
        timed_out: true,
        finished_by: "timeout",
        exit_code: None,
    })
}

async fn local_mcp_shell_done_exit_code(
    session: &Arc<LocalMcpTerminalSession>,
    done_marker: &str,
) -> Option<i32> {
    let logs = session.logs.lock().await;
    let text = logs
        .iter()
        .filter(|entry| matches!(entry.kind.as_str(), "stdout" | "stderr"))
        .map(|entry| entry.content.as_str())
        .collect::<Vec<_>>()
        .join("");
    parse_local_mcp_shell_done_exit_code(text.as_str(), done_marker)
}

fn parse_local_mcp_shell_done_exit_code(text: &str, done_marker: &str) -> Option<i32> {
    let needle = format!("{done_marker}:");
    let start = text.find(needle.as_str())? + needle.len();
    let code = text[start..]
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    if code.is_empty() {
        return None;
    }
    code.parse::<i32>().ok()
}

async fn clear_local_mcp_shell_active_marker(
    session: &Arc<LocalMcpTerminalSession>,
    done_marker: &str,
) {
    let mut active = session.active_shell_marker.lock().await;
    if active.as_deref() == Some(done_marker) {
        *active = None;
    }
}

fn spawn_clear_local_mcp_shell_active_marker_when_done(
    session: Arc<LocalMcpTerminalSession>,
    done_marker: String,
) {
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        loop {
            if refresh_local_mcp_terminal_session_status(&session)
                .await
                .is_err()
            {
                clear_local_mcp_shell_active_marker(&session, done_marker.as_str()).await;
                break;
            }
            let exited = {
                let meta = session.meta.lock().await;
                meta.status == "exited"
            };
            if exited
                || local_mcp_shell_done_exit_code(&session, done_marker.as_str())
                    .await
                    .is_some()
            {
                clear_local_mcp_shell_active_marker(&session, done_marker.as_str()).await;
                break;
            }
            if started.elapsed() >= Duration::from_secs(10 * 60) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
}

fn build_local_mcp_shell_command_script(
    cwd: &Path,
    command: &str,
    start_marker: &str,
    done_marker: &str,
) -> String {
    format!(
        "printf '\\n%s\\n' {}\ncd {}\n__chatos_local_connector_cd_exit=$?\nif [ \"$__chatos_local_connector_cd_exit\" -eq 0 ]; then\n{}\n__chatos_local_connector_exit=$?\nelse\n__chatos_local_connector_exit=$__chatos_local_connector_cd_exit\nfi\nprintf '\\n%s:%s\\n' {} \"$__chatos_local_connector_exit\"\n",
        shell_single_quote(start_marker),
        shell_single_quote(cwd.to_string_lossy().as_ref()),
        command,
        shell_single_quote(done_marker),
    )
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

async fn next_local_mcp_log_offset(session: &Arc<LocalMcpTerminalSession>) -> i64 {
    let logs = session.logs.lock().await;
    logs.last().map(|entry| entry.offset + 1).unwrap_or(0)
}

async fn collect_local_mcp_terminal_output_since_by_kinds(
    session: &Arc<LocalMcpTerminalSession>,
    min_offset: i64,
    max_chars: usize,
    kinds: &[&str],
) -> LocalMcpTerminalOutput {
    let logs = session.logs.lock().await;
    collect_local_mcp_output_from_strings(
        logs.iter()
            .filter(|entry| entry.offset >= min_offset)
            .filter(|entry| kinds.iter().any(|kind| *kind == entry.kind))
            .map(|entry| strip_local_mcp_internal_shell_markers(entry.content.as_str())),
        max_chars,
    )
}

async fn register_local_mcp_terminal_session(
    context: TerminalControllerContext,
    root: PathBuf,
    cwd: PathBuf,
    command: String,
    mut child: tokio::process::Child,
) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();
    let session_id = format!("local-proc-{}", Uuid::new_v4());
    let now = local_now_rfc3339();
    let session = Arc::new(LocalMcpTerminalSession {
        meta: Mutex::new(LocalMcpTerminalMeta {
            id: session_id.clone(),
            root: root.to_string_lossy().to_string(),
            cwd: cwd.to_string_lossy().to_string(),
            project_id: context.project_id,
            user_id: context.user_id,
            command,
            started_at: now.clone(),
            last_active_at: now,
            finished_at: None,
            status: "running".to_string(),
            exit_code: None,
        }),
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        logs: Mutex::new(Vec::new()),
        command_lock: Mutex::new(()),
        active_shell_marker: Mutex::new(None),
    });
    if let Some(stdout) = stdout {
        spawn_local_mcp_terminal_reader(session.clone(), stdout, "stdout");
    }
    if let Some(stderr) = stderr {
        spawn_local_mcp_terminal_reader(session.clone(), stderr, "stderr");
    }
    local_mcp_terminal_registry()
        .sessions
        .write()
        .await
        .insert(session_id, session.clone());
    Ok(session)
}

fn spawn_local_mcp_terminal_reader<R>(
    session: Arc<LocalMcpTerminalSession>,
    mut reader: R,
    kind: &'static str,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = vec![0_u8; 2048];
        loop {
            match reader.read(buf.as_mut_slice()).await {
                Ok(0) => break,
                Ok(count) => {
                    let chunk = String::from_utf8_lossy(&buf[..count]).to_string();
                    append_local_mcp_terminal_log(session.clone(), kind, chunk).await;
                }
                Err(_) => break,
            }
        }
    });
}

async fn append_local_mcp_terminal_log(
    session: Arc<LocalMcpTerminalSession>,
    kind: &str,
    content: String,
) {
    if content.is_empty() {
        return;
    }
    let now = local_now_rfc3339();
    {
        let mut logs = session.logs.lock().await;
        let offset = logs.last().map(|entry| entry.offset + 1).unwrap_or(0);
        logs.push(LocalMcpTerminalLog {
            offset,
            kind: kind.to_string(),
            content,
            created_at: now.clone(),
        });
        if logs.len() > 4_000 {
            let drain = logs.len() - 4_000;
            logs.drain(0..drain);
        }
    }
    let mut meta = session.meta.lock().await;
    meta.last_active_at = now;
}

async fn refresh_local_mcp_terminal_session_status(
    session: &Arc<LocalMcpTerminalSession>,
) -> std::result::Result<(), String> {
    {
        let meta = session.meta.lock().await;
        if meta.status == "exited" {
            return Ok(());
        }
    }
    let status = {
        let mut child = session.child.lock().await;
        child.try_wait().map_err(|err| err.to_string())?
    };
    if let Some(status) = status {
        mark_local_mcp_terminal_exited(session, status.code()).await;
    }
    Ok(())
}

async fn mark_local_mcp_terminal_exited(
    session: &Arc<LocalMcpTerminalSession>,
    exit_code: Option<i32>,
) {
    let mut meta = session.meta.lock().await;
    if meta.status == "exited" {
        return;
    }
    meta.status = "exited".to_string();
    meta.exit_code = exit_code;
    meta.finished_at = Some(local_now_rfc3339());
    meta.last_active_at = meta.finished_at.clone().unwrap_or_else(local_now_rfc3339);
}

async fn local_mcp_sessions_for_context(
    context: &TerminalControllerContext,
) -> std::result::Result<Vec<Arc<LocalMcpTerminalSession>>, String> {
    let root = canonicalize_terminal_root(context.root.as_path())?;
    let sessions = local_mcp_terminal_registry().sessions.read().await;
    let mut matched = Vec::new();
    for session in sessions.values() {
        let meta = session.meta.lock().await.clone();
        let same_user = match context.user_id.as_deref() {
            Some(user_id) => meta.user_id.as_deref() == Some(user_id),
            None => true,
        };
        let same_project = match context.project_id.as_deref() {
            Some(project_id) => meta.project_id.as_deref() == Some(project_id),
            None => true,
        };
        let same_root = PathBuf::from(meta.root.as_str()) == root;
        if same_user && same_project && same_root {
            matched.push(session.clone());
        }
    }
    matched.sort_by(|left, right| {
        let left = left.meta.try_lock();
        let right = right.meta.try_lock();
        match (left, right) {
            (Ok(left), Ok(right)) => right.last_active_at.cmp(&left.last_active_at),
            _ => std::cmp::Ordering::Equal,
        }
    });
    Ok(matched)
}

async fn local_mcp_session_for_context(
    context: &TerminalControllerContext,
    terminal_id: &str,
) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
    let sessions = local_mcp_sessions_for_context(context).await?;
    sessions
        .into_iter()
        .find(|session| {
            session
                .meta
                .try_lock()
                .map(|meta| meta.id == terminal_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("terminal not found in current project context: {terminal_id}"))
}

async fn wait_for_local_mcp_terminal_session(
    session: Arc<LocalMcpTerminalSession>,
    timeout_ms: u64,
) -> std::result::Result<LocalMcpTerminalWaitResult, String> {
    let timeout = Duration::from_millis(timeout_ms.clamp(1_000, 600_000));
    let started = std::time::Instant::now();
    loop {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        if meta.status == "exited" {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "exit",
                exit_code: meta.exit_code,
            });
        }
        if is_local_mcp_primary_shell_command(meta.command.as_str())
            && !local_mcp_shell_session_is_busy(&session).await
        {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "idle",
                exit_code: meta.exit_code,
            });
        }
        if started.elapsed() >= timeout {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let meta = session.meta.lock().await.clone();
    Ok(LocalMcpTerminalWaitResult {
        waited_ms: started.elapsed().as_millis() as u64,
        busy: meta.status != "exited",
        timed_out: true,
        finished_by: "timeout",
        exit_code: meta.exit_code,
    })
}

async fn collect_local_mcp_terminal_output(
    session: &Arc<LocalMcpTerminalSession>,
    max_chars: usize,
) -> LocalMcpTerminalOutput {
    collect_local_mcp_terminal_output_by_kinds(session, max_chars, &["stdout", "stderr"]).await
}

async fn collect_local_mcp_terminal_output_by_kinds(
    session: &Arc<LocalMcpTerminalSession>,
    max_chars: usize,
    kinds: &[&str],
) -> LocalMcpTerminalOutput {
    let logs = session.logs.lock().await;
    collect_local_mcp_output_from_logs(
        logs.iter()
            .filter(|entry| kinds.iter().any(|kind| *kind == entry.kind))
            .map(|entry| entry.content.as_str()),
        max_chars,
    )
}

fn collect_local_mcp_output_from_logs<'a, I>(items: I, max_chars: usize) -> LocalMcpTerminalOutput
where
    I: Iterator<Item = &'a str>,
{
    let full = items
        .map(strip_local_mcp_internal_shell_markers)
        .collect::<Vec<_>>()
        .join("");
    collect_local_mcp_output_from_text(full, max_chars)
}

fn collect_local_mcp_output_from_strings<I>(items: I, max_chars: usize) -> LocalMcpTerminalOutput
where
    I: Iterator<Item = String>,
{
    let full = items.collect::<Vec<_>>().join("");
    collect_local_mcp_output_from_text(full, max_chars)
}

fn collect_local_mcp_output_from_text(full: String, max_chars: usize) -> LocalMcpTerminalOutput {
    let char_count = full.chars().count();
    if char_count <= max_chars {
        return LocalMcpTerminalOutput {
            text: full,
            char_count,
            truncated: false,
        };
    }
    let text = full
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect::<String>();
    LocalMcpTerminalOutput {
        text,
        char_count,
        truncated: true,
    }
}

fn strip_local_mcp_internal_shell_markers(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| {
            !line.contains("__CHATO_LOCAL_CMD_START_") && !line.contains("__CHATO_LOCAL_CMD_DONE_")
        })
        .collect()
}

fn select_local_mcp_logs(
    logs: &[LocalMcpTerminalLog],
    offset: Option<i64>,
    limit: usize,
) -> Vec<Value> {
    let selected = if let Some(offset) = offset {
        logs.iter()
            .filter(|entry| entry.offset >= offset.max(0))
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        logs.iter().rev().take(limit).collect::<Vec<_>>()
    };
    let ordered = if offset.is_some() {
        selected
    } else {
        selected.into_iter().rev().collect::<Vec<_>>()
    };
    ordered.into_iter().map(local_mcp_log_to_value).collect()
}

fn take_recent_local_mcp_logs(logs: &[LocalMcpTerminalLog], limit: usize) -> Vec<Value> {
    logs.iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(local_mcp_log_to_value)
        .collect()
}

fn local_mcp_log_to_value(entry: &LocalMcpTerminalLog) -> Value {
    json!({
        "offset": entry.offset,
        "kind": entry.kind,
        "content": strip_local_mcp_internal_shell_markers(entry.content.as_str()),
        "created_at": entry.created_at,
    })
}

fn canonicalize_terminal_root(root: &Path) -> std::result::Result<PathBuf, String> {
    root.canonicalize()
        .map_err(|_| "workspace path is not available".to_string())
}

fn display_local_mcp_workspace_path(root: &Path, path: &Path) -> String {
    if path == root {
        return "/workspace".to_string();
    }
    if let Ok(relative) = path.strip_prefix(root) {
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            "/workspace".to_string()
        } else {
            format!("/workspace/{}", relative.trim_start_matches('/'))
        }
    } else {
        "/workspace".to_string()
    }
}

fn derive_local_mcp_terminal_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("terminal")
        .to_string()
}

fn resolve_terminal_controller_cwd(
    root: &Path,
    path: &str,
) -> std::result::Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|err| {
        format!(
            "canonicalize terminal root {} failed: {err}",
            root.display()
        )
    })?;
    let trimmed = path.trim();
    let candidate = if trimmed.is_empty() || trimmed == "." {
        root.clone()
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        root.join(trimmed)
    };
    let canonical = candidate.canonicalize().map_err(|err| {
        format!(
            "canonicalize terminal cwd {} failed: {err}",
            candidate.display()
        )
    })?;
    if !canonical.starts_with(root.as_path()) {
        return Err("terminal cwd is outside workspace root".to_string());
    }
    if !canonical.is_dir() {
        return Err(format!(
            "terminal cwd is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn shell_command_for_terminal_controller(command: &str) -> tokio::process::Command {
    if cfg!(windows) {
        let mut cmd = tokio::process::Command::new(
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        );
        cmd.arg("/C").arg(command);
        return cmd;
    }
    let mut cmd = tokio::process::Command::new(select_local_shell());
    cmd.arg("-lc").arg(command);
    cmd
}

fn shell_session_for_terminal_controller(shell: &str) -> tokio::process::Command {
    if cfg!(windows) {
        let mut cmd = tokio::process::Command::new(
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        );
        cmd.arg("/K");
        return cmd;
    }
    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg("-l");
    cmd
}

fn local_terminal_controller_context_for_root(
    root: &Path,
    request: &RelayRequest,
    timeout_ms: u64,
) -> TerminalControllerContext {
    TerminalControllerContext {
        root: root.to_path_buf(),
        user_id: request.owner_user_id.clone(),
        project_id: local_mcp_terminal_project_id(request),
        idle_timeout_ms: 1_000,
        max_wait_ms: timeout_ms,
        max_output_chars: MAX_TERMINAL_OUTPUT_BYTES,
    }
}

fn local_mcp_terminal_project_id(request: &RelayRequest) -> Option<String> {
    request
        .headers
        .get("x-task-runner-task-id")
        .or_else(|| request.headers.get("x-task-runner-run-id"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(request.workspace_id.clone()))
}

fn local_terminal_controller_service_for_root(
    root: &Path,
    request: &RelayRequest,
    timeout_ms: u64,
) -> Result<TerminalControllerService> {
    TerminalControllerService::new(TerminalControllerOptions {
        root: root.to_path_buf(),
        user_id: request.owner_user_id.clone(),
        project_id: local_mcp_terminal_project_id(request),
        idle_timeout_ms: 1_000,
        max_wait_ms: timeout_ms,
        max_output_chars: MAX_TERMINAL_OUTPUT_BYTES,
        store: TerminalControllerStoreRef::new(Arc::new(LocalConnectorTerminalControllerStore)),
    })
    .map_err(|err| anyhow!(err))
}

async fn handle_terminal_exec_request(
    value: Value,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("terminal_response", "", 400, err.to_string());
        }
    };
    match run_terminal_exec(
        &request,
        state,
        request.body.clone(),
        CommandExecutionContext::terminal_exec(&request),
        Some(history_recorder),
    )
    .await
    {
        Ok(body) => {
            let status = if body
                .get("timed_out")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                408
            } else {
                200
            };
            RelayResponse {
                message_type: "terminal_response".to_string(),
                request_id: request.request_id,
                status,
                headers: BTreeMap::new(),
                body,
            }
            .to_value()
        }
        Err(err) => RelayResponse {
            message_type: "terminal_response".to_string(),
            request_id: request.request_id,
            status: 400,
            headers: BTreeMap::new(),
            body: json!({ "error": err.to_string() }),
        }
        .to_value(),
    }
}

async fn handle_terminal_session_create_request(
    value: Value,
    state: &LocalState,
    terminal_manager: &LocalTerminalManager,
    outbound_tx: mpsc::UnboundedSender<Value>,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(
                "terminal_session_create_response",
                "",
                400,
                err.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<TerminalSessionCreateRequest>(request.body.clone()) {
        Ok(body) => body,
        Err(err) => {
            return RelayResponse {
                message_type: "terminal_session_create_response".to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    let workspace = match workspace_for_request(state, request.workspace_id.as_str()) {
        Ok(workspace) => workspace,
        Err(err) => {
            return RelayResponse {
                message_type: "terminal_session_create_response".to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    let cwd = match resolve_request_workspace_dir(
        workspace,
        &request,
        body.cwd.as_deref().unwrap_or("."),
    ) {
        Ok(cwd) => cwd,
        Err(err) => {
            return RelayResponse {
                message_type: "terminal_session_create_response".to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    let root_cwd = match canonicalize_existing_dir(workspace.absolute_root.as_path()) {
        Ok(root) => root,
        Err(err) => {
            return RelayResponse {
                message_type: "terminal_session_create_response".to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    let session = match terminal_manager
        .ensure_session(
            body.terminal_session_id.clone(),
            root_cwd,
            cwd,
            body.cols.unwrap_or(80).max(1),
            body.rows.unwrap_or(24).max(1),
            outbound_tx,
        )
        .await
    {
        Ok(session) => session,
        Err(err) => {
            return RelayResponse {
                message_type: "terminal_session_create_response".to_string(),
                request_id: request.request_id,
                status: 500,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    RelayResponse {
        message_type: "terminal_session_create_response".to_string(),
        request_id: request.request_id,
        status: 200,
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": body.terminal_session_id,
            "snapshot": session.snapshot(500),
            "busy": session.busy.load(Ordering::SeqCst),
        }),
    }
    .to_value()
}

async fn handle_terminal_input(
    value: Value,
    state: &LocalState,
    terminal_manager: &LocalTerminalManager,
    history_recorder: &CommandHistoryRecorder,
    outbound_tx: mpsc::UnboundedSender<Value>,
) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(input) = serde_json::from_value::<TerminalSessionInputRequest>(request.body.clone())
    else {
        return;
    };
    let Some(session) = terminal_manager
        .get(input.terminal_session_id.as_str())
        .await
    else {
        let _ = outbound_tx.send(terminal_event(
            "terminal_error",
            input.terminal_session_id.as_str(),
            json!({ "error": "terminal session not found" }),
        ));
        return;
    };
    let submissions = match session.write_input(input.data.as_str()) {
        Ok(submissions) => submissions,
        Err(err) => {
            let _ = outbound_tx.send(terminal_event(
                "terminal_error",
                input.terminal_session_id.as_str(),
                json!({ "error": err.to_string() }),
            ));
            return;
        }
    };
    for submission in submissions {
        history_recorder
            .append(command_history_entry_for_interactive_submission(
                state,
                &request,
                input.terminal_session_id.as_str(),
                submission,
            ))
            .await;
    }
    let _ = outbound_tx.send(terminal_event(
        "terminal_state",
        input.terminal_session_id.as_str(),
        json!({ "busy": session.busy.load(Ordering::SeqCst) }),
    ));
}

async fn handle_terminal_resize(
    value: Value,
    terminal_manager: &LocalTerminalManager,
    outbound_tx: mpsc::UnboundedSender<Value>,
) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(resize) = serde_json::from_value::<TerminalSessionResizeRequest>(request.body) else {
        return;
    };
    let Some(session) = terminal_manager
        .get(resize.terminal_session_id.as_str())
        .await
    else {
        return;
    };
    if let Err(err) = session.resize(resize.cols, resize.rows) {
        let _ = outbound_tx.send(terminal_event(
            "terminal_error",
            resize.terminal_session_id.as_str(),
            json!({ "error": err.to_string() }),
        ));
    }
}

async fn handle_terminal_snapshot_request(
    value: Value,
    terminal_manager: &LocalTerminalManager,
    outbound_tx: mpsc::UnboundedSender<Value>,
) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(snapshot) = serde_json::from_value::<TerminalSessionSnapshotRequest>(request.body)
    else {
        return;
    };
    let Some(session) = terminal_manager
        .get(snapshot.terminal_session_id.as_str())
        .await
    else {
        return;
    };
    let data = session.snapshot(snapshot.lines.unwrap_or(500));
    let _ = outbound_tx.send(terminal_event(
        "terminal_snapshot",
        snapshot.terminal_session_id.as_str(),
        json!({ "data": data }),
    ));
}

async fn handle_terminal_close(value: Value, terminal_manager: &LocalTerminalManager) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(close) = serde_json::from_value::<TerminalSessionCloseRequest>(request.body) else {
        return;
    };
    terminal_manager
        .close(close.terminal_session_id.as_str())
        .await;
}

async fn run_terminal_exec(
    request: &RelayRequest,
    state: &LocalState,
    body: Value,
    mut context: CommandExecutionContext,
    history_recorder: Option<&CommandHistoryRecorder>,
) -> Result<Value> {
    let started_at = local_now_rfc3339();
    let exec = serde_json::from_value::<TerminalExecRequest>(body)
        .context("parse terminal exec request")?;
    let command = exec.command.trim().to_string();
    if command.is_empty() {
        return Err(anyhow!("terminal exec requires command"));
    }
    let args = exec.args;
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let cwd =
        resolve_request_workspace_dir(workspace, request, exec.cwd.as_deref().unwrap_or("."))?;
    let cwd_label = relative_to_workspace(workspace, cwd.as_path());
    if let Some(source) = exec.source.as_deref().and_then(normalize_history_source) {
        context.source = source;
    }
    let timeout_ms = exec
        .timeout_ms
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);

    let mut child = tokio::process::Command::new(command.as_str());
    child
        .args(&args)
        .current_dir(cwd.as_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(Duration::from_millis(timeout_ms), child.output()).await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            let body = json!({
                "command": command,
                "args": args,
                "cwd": cwd_label,
                "workspace_id": request.workspace_id.as_str(),
                "success": false,
                "exit_code": Option::<i32>::None,
                "timed_out": false,
                "timeout_ms": timeout_ms,
                "stdout": "",
                "stderr": "",
                "error": err.to_string(),
            });
            if let Some(recorder) = history_recorder {
                recorder
                    .append(command_history_entry_from_exec_result(
                        state,
                        request,
                        &context,
                        command.as_str(),
                        &args,
                        cwd_label.as_str(),
                        started_at,
                        &body,
                    ))
                    .await;
            }
            return Ok(body);
        }
        Err(_) => {
            let body = json!({
                "command": command,
                "args": args,
                "cwd": cwd_label,
                "workspace_id": request.workspace_id.as_str(),
                "success": false,
                "exit_code": Option::<i32>::None,
                "timed_out": true,
                "timeout_ms": timeout_ms,
                "stdout": "",
                "stderr": format!("command timed out after {timeout_ms} ms"),
            });
            if let Some(recorder) = history_recorder {
                recorder
                    .append(command_history_entry_from_exec_result(
                        state,
                        request,
                        &context,
                        command.as_str(),
                        &args,
                        cwd_label.as_str(),
                        started_at,
                        &body,
                    ))
                    .await;
            }
            return Ok(body);
        }
    };

    let (stdout, stdout_truncated) = output_text(output.stdout.as_slice());
    let (stderr, stderr_truncated) = output_text(output.stderr.as_slice());
    let body = json!({
        "command": command,
        "args": args,
        "cwd": cwd_label,
        "workspace_id": request.workspace_id.as_str(),
        "success": output.status.success(),
        "exit_code": output.status.code(),
        "timed_out": false,
        "timeout_ms": timeout_ms,
        "stdout": stdout,
        "stderr": stderr,
        "stdout_bytes": output.stdout.len(),
        "stderr_bytes": output.stderr.len(),
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
    });
    if let Some(recorder) = history_recorder {
        let command = body
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = body
            .get("args")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let cwd_label = body.get("cwd").and_then(Value::as_str).unwrap_or(".");
        recorder
            .append(command_history_entry_from_exec_result(
                state, request, &context, command, &args, cwd_label, started_at, &body,
            ))
            .await;
    }
    Ok(body)
}

async fn handle_sandbox_request(
    value: Value,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("sandbox_response", "", 400, err.to_string());
        }
    };
    match handle_local_sandbox_request(
        &request,
        state,
        http_client,
        sandbox_runtime,
        history_recorder,
    )
    .await
    {
        Ok((status, headers, body)) => RelayResponse {
            message_type: "sandbox_response".to_string(),
            request_id: request.request_id,
            status,
            headers,
            body,
        }
        .to_value(),
        Err(err) => RelayResponse {
            message_type: "sandbox_response".to_string(),
            request_id: request.request_id,
            status: 502,
            headers: BTreeMap::new(),
            body: json!({ "error": err.to_string() }),
        }
        .to_value(),
    }
}

async fn handle_local_sandbox_request(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let method = request
        .method
        .as_deref()
        .unwrap_or("POST")
        .parse::<Method>()
        .context("parse sandbox request method")?;
    let path = request.path.as_deref().unwrap_or("/");
    let path = normalize_http_path(path);
    if method == Method::POST && path == "/api/sandboxes/leases" {
        return create_local_sandbox_lease(request, state, http_client, sandbox_runtime).await;
    }
    if method == Method::GET && path == "/api/sandboxes" {
        let leases = sandbox_runtime
            .leases
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        return Ok((200, BTreeMap::new(), json!(leases)));
    }
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() >= 3 && parts[0] == "api" && parts[1] == "sandboxes" {
        let sandbox_id = parts[2];
        if method == Method::GET && parts.len() == 3 {
            return get_local_sandbox(sandbox_runtime, sandbox_id).await;
        }
        if method == Method::GET && parts.len() == 4 && parts[3] == "health" {
            return health_local_sandbox(http_client, sandbox_runtime, sandbox_id).await;
        }
        if method == Method::POST && parts.len() == 4 && parts[3] == "release" {
            return release_local_sandbox(request, sandbox_runtime, sandbox_id).await;
        }
        if method == Method::POST && parts.len() == 4 && parts[3] == "mcp" {
            return proxy_local_sandbox_mcp(
                request,
                state,
                http_client,
                sandbox_runtime,
                sandbox_id,
                history_recorder,
            )
            .await;
        }
        if method == Method::POST && parts.len() == 5 && parts[3] == "mcp" && parts[4] == "call" {
            return proxy_local_sandbox_mcp_call(
                request,
                state,
                http_client,
                sandbox_runtime,
                sandbox_id,
                history_recorder,
            )
            .await;
        }
        if method == Method::GET && parts.len() == 5 && parts[3] == "mcp" && parts[4] == "tools" {
            return proxy_local_sandbox_mcp_tools(http_client, sandbox_runtime, sandbox_id).await;
        }
    }
    Ok((
        404,
        BTreeMap::new(),
        json!({ "error": format!("unsupported local sandbox path: {path}") }),
    ))
}

async fn create_local_sandbox_lease(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    if !state.sandbox.enabled {
        return Ok((
            400,
            BTreeMap::new(),
            json!({ "error": "local sandbox is disabled" }),
        ));
    }
    ensure_docker_running().await?;
    let body = local_sandbox_request_body(request, state, &Method::POST, "/api/sandboxes/leases")?;
    let input = serde_json::from_value::<CreateLocalSandboxLeaseRequest>(body)
        .context("parse local sandbox lease request")?;
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let lease_id = format!("lease-{}", Uuid::new_v4());
    let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
    let agent_token = format!("sat-{lease_id}");
    let run_workspace = local_sandbox_run_workspace(workspace, input.run_id.as_str())?;
    let response_seed = json!({ "run_workspace": run_workspace.to_string_lossy() });
    prepare_local_sandbox_workspace(request, state, &response_seed)?;
    let resource_limits = input.resource_limits.unwrap_or_default();
    let network = input.network.unwrap_or_default();
    let image_ref =
        select_local_sandbox_image_ref(state, sandbox_runtime, input.image_id.as_deref()).await;
    let backend_id = start_local_sandbox_container(
        sandbox_id.as_str(),
        run_workspace.as_path(),
        image_ref.as_str(),
        agent_token.as_str(),
        &resource_limits,
        &network,
    )
    .await?;
    let Some(agent_endpoint) = published_local_sandbox_agent_endpoint(sandbox_id.as_str()).await
    else {
        let _ = destroy_local_sandbox_container(sandbox_id.as_str()).await;
        return Err(anyhow!("local sandbox agent port was not published"));
    };
    if let Err(err) = wait_for_local_sandbox_agent(http_client, agent_endpoint.as_str()).await {
        let _ = destroy_local_sandbox_container(sandbox_id.as_str()).await;
        return Err(err);
    }
    let now = local_now_rfc3339();
    let expires_at = (Utc::now()
        + ChronoDuration::seconds(input.ttl_seconds.unwrap_or(7200) as i64))
    .to_rfc3339();
    let lease = LocalSandboxLease {
        id: lease_id.clone(),
        sandbox_id: sandbox_id.clone(),
        tenant_id: input.tenant_id,
        user_id: input.user_id,
        project_id: input.project_id,
        run_id: input.run_id,
        workspace_root: input.workspace_root,
        run_workspace: run_workspace.to_string_lossy().to_string(),
        backend: LOCAL_SANDBOX_BACKEND.to_string(),
        backend_id: Some(backend_id),
        image_id: input.image_id,
        image_ref: Some(image_ref),
        status: LOCAL_SANDBOX_STATUS_READY.to_string(),
        agent_endpoint: Some(agent_endpoint),
        agent_token: agent_token.clone(),
        resource_limits,
        network,
        tools: if input.tools.is_empty() {
            vec!["filesystem".to_string(), "terminal".to_string()]
        } else {
            input.tools
        },
        created_at: now.clone(),
        updated_at: now,
        expires_at,
        destroyed_at: None,
        last_error: None,
    };
    let response = local_sandbox_lease_response(&lease);
    sandbox_runtime
        .leases
        .write()
        .await
        .insert(sandbox_id, lease);
    Ok((201, BTreeMap::new(), response))
}

async fn select_local_sandbox_image_ref(
    state: &LocalState,
    sandbox_runtime: &LocalSandboxRuntime,
    image_id: Option<&str>,
) -> String {
    if let Some(image_id) = image_id.filter(|value| *value != "default") {
        if let Some(job) = sandbox_runtime
            .jobs
            .read()
            .await
            .iter()
            .find(|job| job.image_id == image_id && job.status == "succeeded")
        {
            return job.image_ref.clone();
        }
    }
    state
        .sandbox
        .selected_image_ref
        .clone()
        .or_else(|| optional_env("LOCAL_CONNECTOR_SANDBOX_DOCKER_IMAGE"))
        .unwrap_or_else(|| DEFAULT_LOCAL_SANDBOX_IMAGE.to_string())
}

fn local_sandbox_lease_response(lease: &LocalSandboxLease) -> Value {
    json!({
        "lease_id": lease.id,
        "sandbox_id": lease.sandbox_id,
        "backend_id": lease.backend_id,
        "image_id": lease.image_id,
        "image_ref": lease.image_ref,
        "status": lease.status,
        "agent_endpoint": lease.agent_endpoint,
        "agent_token": lease.agent_token,
        "run_workspace": lease.run_workspace,
        "expires_at": lease.expires_at,
    })
}

async fn get_local_sandbox(
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let Some(lease) = sandbox_runtime.leases.read().await.get(sandbox_id).cloned() else {
        return Ok((
            404,
            BTreeMap::new(),
            json!({ "error": "sandbox not found" }),
        ));
    };
    Ok((200, BTreeMap::new(), json!(lease)))
}

async fn health_local_sandbox(
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let Some(lease) = sandbox_runtime.leases.read().await.get(sandbox_id).cloned() else {
        return Ok((
            404,
            BTreeMap::new(),
            json!({ "error": "sandbox not found" }),
        ));
    };
    let backend_alive = inspect_local_sandbox_container(sandbox_id).await?;
    let workspace_alive = Path::new(lease.run_workspace.as_str()).is_dir();
    let agent_alive = match lease.agent_endpoint.as_deref() {
        Some(endpoint) => http_client
            .get(format!("{}/health", endpoint.trim_end_matches('/')))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()
            .map(|response| response.status().is_success()),
        None => None,
    };
    let ok = backend_alive && workspace_alive && agent_alive.unwrap_or(false);
    Ok((
        200,
        BTreeMap::new(),
        json!({
            "ok": ok,
            "sandbox_id": lease.sandbox_id,
            "lease_id": lease.id,
            "status": lease.status,
            "backend": lease.backend,
            "backend_id": lease.backend_id,
            "backend_alive": backend_alive,
            "agent_endpoint": lease.agent_endpoint,
            "agent_alive": agent_alive,
            "workspace_alive": workspace_alive,
            "checked_at": local_now_rfc3339(),
            "message": if ok { "ok" } else { "local sandbox is not healthy" },
            "checks": [
                { "name": "docker_container", "ok": backend_alive, "message": if backend_alive { "running" } else { "not running" } },
                { "name": "workspace", "ok": workspace_alive, "message": if workspace_alive { "available" } else { "missing" } },
                { "name": "agent", "ok": agent_alive.unwrap_or(false), "message": if agent_alive.unwrap_or(false) { "reachable" } else { "unreachable" } }
            ]
        }),
    ))
}

async fn release_local_sandbox(
    request: &RelayRequest,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let input = serde_json::from_value::<ReleaseLocalSandboxRequest>(request.body.clone())
        .context("parse local sandbox release request")?;
    let mut lease = {
        let leases = sandbox_runtime.leases.read().await;
        let Some(lease) = leases.get(sandbox_id).cloned() else {
            return Ok((
                404,
                BTreeMap::new(),
                json!({ "error": "sandbox not found" }),
            ));
        };
        lease
    };
    if lease.id != input.lease_id {
        return Ok((
            400,
            BTreeMap::new(),
            json!({ "error": "lease_id does not match sandbox" }),
        ));
    }
    let (output_workspace, change_manifest, diff_summary, output_error) = if input.export_result {
        match export_local_sandbox_output(&lease) {
            Ok(manifest) => {
                let output_workspace = manifest
                    .get("output_workspace")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let summary = manifest
                    .get("counts")
                    .map(summarize_local_sandbox_manifest_counts);
                (output_workspace, Some(manifest), summary, None)
            }
            Err(err) => (None, None, None, Some(err.to_string())),
        }
    } else {
        (None, None, None, None)
    };
    if input.destroy {
        destroy_local_sandbox_container(sandbox_id).await?;
        lease.status = LOCAL_SANDBOX_STATUS_DESTROYED.to_string();
        lease.destroyed_at = Some(local_now_rfc3339());
    }
    lease.updated_at = local_now_rfc3339();
    sandbox_runtime
        .leases
        .write()
        .await
        .insert(sandbox_id.to_string(), lease.clone());
    Ok((
        200,
        BTreeMap::new(),
        json!({
            "ok": true,
            "status": lease.status,
            "output_workspace": output_workspace,
            "diff_summary": diff_summary,
            "output_error": output_error,
            "change_manifest": change_manifest,
        }),
    ))
}

async fn proxy_local_sandbox_mcp(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let started_at = local_now_rfc3339();
    let tool_call = sandbox_tool_call_details(&request.body);
    let lease = require_local_sandbox_lease(sandbox_runtime, sandbox_id).await?;
    let endpoint = require_local_sandbox_agent_endpoint(&lease)?;
    let response = http_client
        .post(format!("{}/mcp", endpoint.trim_end_matches('/')))
        .bearer_auth(lease.agent_token.as_str())
        .json(&request.body)
        .send()
        .await
        .context("proxy local sandbox mcp request")?;
    let result = local_sandbox_http_response(response).await?;
    if let Some(tool_call) = tool_call {
        history_recorder
            .append(command_history_entry_for_sandbox_tool_call(
                state,
                request,
                &CommandExecutionContext::task_runner_sandbox(
                    request,
                    sandbox_id,
                    tool_call.tool_name.as_str(),
                ),
                tool_call,
                result.0,
                &result.2,
                started_at,
            ))
            .await;
    }
    Ok(result)
}

async fn proxy_local_sandbox_mcp_call(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let started_at = local_now_rfc3339();
    let tool_call = sandbox_tool_call_details(&request.body);
    let lease = require_local_sandbox_lease(sandbox_runtime, sandbox_id).await?;
    let endpoint = require_local_sandbox_agent_endpoint(&lease)?;
    let response = http_client
        .post(format!("{}/mcp/call", endpoint.trim_end_matches('/')))
        .bearer_auth(lease.agent_token.as_str())
        .json(&request.body)
        .send()
        .await
        .context("proxy local sandbox mcp call")?;
    let result = local_sandbox_http_response(response).await?;
    if let Some(tool_call) = tool_call {
        history_recorder
            .append(command_history_entry_for_sandbox_tool_call(
                state,
                request,
                &CommandExecutionContext::task_runner_sandbox(
                    request,
                    sandbox_id,
                    tool_call.tool_name.as_str(),
                ),
                tool_call,
                result.0,
                &result.2,
                started_at,
            ))
            .await;
    }
    Ok(result)
}

async fn proxy_local_sandbox_mcp_tools(
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let lease = require_local_sandbox_lease(sandbox_runtime, sandbox_id).await?;
    let endpoint = require_local_sandbox_agent_endpoint(&lease)?;
    let response = http_client
        .get(format!("{}/mcp/tools", endpoint.trim_end_matches('/')))
        .bearer_auth(lease.agent_token.as_str())
        .send()
        .await
        .context("proxy local sandbox mcp tools")?;
    local_sandbox_http_response(response).await
}

async fn local_sandbox_http_response(
    response: reqwest::Response,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let status = response.status().as_u16();
    let headers = response_headers(response.headers());
    let bytes = response
        .bytes()
        .await
        .context("read local sandbox response")?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(bytes.as_ref())
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes.as_ref()).into_owned()))
    };
    Ok((status, headers, body))
}

async fn require_local_sandbox_lease(
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<LocalSandboxLease> {
    sandbox_runtime
        .leases
        .read()
        .await
        .get(sandbox_id)
        .cloned()
        .ok_or_else(|| anyhow!("sandbox not found"))
}

fn require_local_sandbox_agent_endpoint(lease: &LocalSandboxLease) -> Result<String> {
    lease
        .agent_endpoint
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("local sandbox agent endpoint is not ready"))
}

fn local_sandbox_run_workspace(workspace: &WorkspaceState, run_id: &str) -> Result<PathBuf> {
    let root = local_sandbox_workspace_root(workspace)?;
    let run_workspace = root
        .join("runs")
        .join(sanitize_path_segment(run_id))
        .join("input")
        .join("workspace");
    fs::create_dir_all(run_workspace.as_path()).with_context(|| {
        format!(
            "create local sandbox run workspace {}",
            run_workspace.display()
        )
    })?;
    Ok(run_workspace)
}

async fn start_local_sandbox_container(
    sandbox_id: &str,
    run_workspace: &Path,
    image_ref: &str,
    agent_token: &str,
    resource_limits: &LocalSandboxResourceLimits,
    network: &LocalSandboxNetworkPolicy,
) -> Result<String> {
    let name = local_sandbox_container_name(sandbox_id);
    let network_mode = if network.mode.trim().is_empty() {
        "bridge"
    } else {
        network.mode.trim()
    };
    let mut command = tokio::process::Command::new("docker");
    command
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(name.as_str())
        .arg("--hostname")
        .arg(name.as_str())
        .arg("--label")
        .arg(format!("chatos.local_connector.sandbox_id={sandbox_id}"))
        .arg("--network")
        .arg(network_mode)
        .arg("--cpus")
        .arg(resource_limits.cpu.max(0.1).to_string())
        .arg("--memory")
        .arg(format!("{}m", resource_limits.memory_mb.max(128)))
        .arg("--pids-limit")
        .arg(resource_limits.max_processes.max(16).to_string())
        .arg("--workdir")
        .arg("/workspace")
        .arg("-e")
        .arg(format!("CHATOS_SANDBOX_ID={sandbox_id}"))
        .arg("-e")
        .arg(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"));
    if network_mode != "none" {
        command
            .arg("-p")
            .arg(format!("127.0.0.1::{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}"));
    }
    command
        .arg("--tmpfs")
        .arg("/tmp:rw,nosuid,size=512m")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("-v")
        .arg(format!("{}:/workspace:rw", run_workspace.display()))
        .arg(image_ref);
    let output = command
        .output()
        .await
        .context("start local docker sandbox")?;
    if !output.status.success() {
        return Err(anyhow!(
            "docker run failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ));
    }
    Ok(String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string())
}

async fn inspect_local_sandbox_container(sandbox_id: &str) -> Result<bool> {
    let output = tokio::process::Command::new("docker")
        .arg("inspect")
        .arg("-f")
        .arg("{{.State.Running}}")
        .arg(local_sandbox_container_name(sandbox_id))
        .output()
        .await
        .context("inspect local sandbox container")?;
    Ok(output.status.success()
        && String::from_utf8_lossy(output.stdout.as_slice())
            .trim()
            .eq_ignore_ascii_case("true"))
}

async fn destroy_local_sandbox_container(sandbox_id: &str) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .arg("rm")
        .arg("-f")
        .arg(local_sandbox_container_name(sandbox_id))
        .output()
        .await
        .context("remove local sandbox container")?;
    if output.status.success()
        || String::from_utf8_lossy(output.stderr.as_slice()).contains("No such container")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "docker rm failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ))
    }
}

async fn published_local_sandbox_agent_endpoint(sandbox_id: &str) -> Option<String> {
    let output = tokio::process::Command::new("docker")
        .arg("port")
        .arg(local_sandbox_container_name(sandbox_id))
        .arg(format!("{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}/tcp"))
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    let line = stdout.lines().next()?.trim();
    let host_port = line.rsplit(':').next()?.trim();
    if host_port.is_empty() {
        None
    } else {
        Some(format!("http://127.0.0.1:{host_port}"))
    }
}

async fn wait_for_local_sandbox_agent(
    http_client: &reqwest::Client,
    agent_endpoint: &str,
) -> Result<()> {
    let health_url = format!("{}/health", agent_endpoint.trim_end_matches('/'));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let last_error = match http_client
            .get(health_url.as_str())
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => format!("HTTP {}", response.status()),
            Err(err) => err.to_string(),
        };
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "local sandbox agent did not become healthy at {agent_endpoint}: {last_error}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn local_sandbox_container_name(sandbox_id: &str) -> String {
    format!("chatos-local-{sandbox_id}")
}

fn export_local_sandbox_output(lease: &LocalSandboxLease) -> Result<Value> {
    let run_workspace = PathBuf::from(lease.run_workspace.as_str());
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("invalid run workspace path"))?;
    let output_workspace = run_root.join("output").join("workspace");
    clear_directory(output_workspace.as_path())?;
    copy_workspace_contents_to_sandbox(
        run_workspace.as_path(),
        output_workspace.as_path(),
        run_workspace.as_path(),
    )?;
    let baseline_workspace = local_sandbox_baseline_workspace(run_workspace.as_path())?;
    let manifest = build_local_sandbox_change_manifest(
        lease,
        baseline_workspace.as_path(),
        output_workspace.as_path(),
    )?;
    let output_root = output_workspace
        .parent()
        .ok_or_else(|| anyhow!("invalid output workspace path"))?;
    let manifest_path = output_root.join("change_manifest.json");
    let mut manifest = manifest;
    manifest["output_workspace"] = Value::String(output_workspace.to_string_lossy().to_string());
    manifest["manifest_path"] = Value::String(manifest_path.to_string_lossy().to_string());
    fs::write(
        manifest_path.as_path(),
        serde_json::to_string_pretty(&manifest)?,
    )
    .with_context(|| format!("write {}", manifest_path.display()))?;
    Ok(manifest)
}

fn build_local_sandbox_change_manifest(
    lease: &LocalSandboxLease,
    baseline_workspace: &Path,
    output_workspace: &Path,
) -> Result<Value> {
    let baseline_files = collect_file_index(baseline_workspace)?;
    let output_files = collect_file_index(output_workspace)?;
    let paths = baseline_files
        .keys()
        .chain(output_files.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();
    for path in paths {
        let old_file = baseline_files.get(path.as_str());
        let new_file = output_files.get(path.as_str());
        let status = match (old_file, new_file) {
            (None, Some(_)) => "added",
            (Some(_), None) => "deleted",
            (Some(old_file), Some(new_file)) if old_file.sha256 != new_file.sha256 => "modified",
            _ => continue,
        };
        files.push(json!({
            "path": path,
            "status": status,
            "old_size": old_file.map(|file| file.size),
            "new_size": new_file.map(|file| file.size),
            "old_sha256": old_file.map(|file| file.sha256.clone()),
            "new_sha256": new_file.map(|file| file.sha256.clone()),
            "added_lines": 0,
            "deleted_lines": 0,
            "binary": false,
            "diff_available": false,
            "diff_truncated": false,
            "diff_ref": null,
        }));
    }
    let counts = local_sandbox_change_counts(files.as_slice());
    Ok(json!({
        "schema_version": 1,
        "run_id": lease.run_id,
        "sandbox_id": lease.sandbox_id,
        "lease_id": lease.id,
        "generated_at": local_now_rfc3339(),
        "output_workspace": null,
        "manifest_path": null,
        "counts": counts,
        "files": files,
    }))
}

#[derive(Debug, Clone)]
struct LocalFileSnapshot {
    size: u64,
    sha256: String,
}

fn collect_file_index(root: &Path) -> Result<BTreeMap<String, LocalFileSnapshot>> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_file_index_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_file_index_inner(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, LocalFileSnapshot>,
) -> Result<()> {
    for entry in fs::read_dir(current).with_context(|| format!("read {}", current.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_file_index_inner(root, path.as_path(), files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = entry.metadata()?;
            files.insert(
                relative,
                LocalFileSnapshot {
                    size: metadata.len(),
                    sha256: sha256_file(path.as_path())?,
                },
            );
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn local_sandbox_change_counts(files: &[Value]) -> Value {
    let mut added = 0usize;
    let mut modified = 0usize;
    let mut deleted = 0usize;
    for file in files {
        match file
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "added" => added += 1,
            "modified" => modified += 1,
            "deleted" => deleted += 1,
            _ => {}
        }
    }
    json!({
        "added": added,
        "modified": modified,
        "deleted": deleted,
        "binary": 0,
        "diff_available": 0,
        "total": files.len(),
    })
}

fn summarize_local_sandbox_manifest_counts(counts: &Value) -> String {
    format!(
        "added={}, modified={}, deleted={}, total={}",
        counts.get("added").and_then(Value::as_u64).unwrap_or(0),
        counts.get("modified").and_then(Value::as_u64).unwrap_or(0),
        counts.get("deleted").and_then(Value::as_u64).unwrap_or(0),
        counts.get("total").and_then(Value::as_u64).unwrap_or(0),
    )
}

fn clear_directory(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))
}

fn sanitize_path_segment(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches(['-', '.', '_']);
    if sanitized.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        sanitized.to_string()
    }
}

fn local_now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn default_true() -> bool {
    true
}

fn local_sandbox_request_body(
    request: &RelayRequest,
    state: &LocalState,
    method: &Method,
    path: &str,
) -> Result<Value> {
    if !is_sandbox_create_lease_request(method, path) {
        return Ok(request.body.clone());
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let workspace_root = local_sandbox_workspace_root(workspace)?;
    let mut body = request.body.clone();
    let object = body
        .as_object_mut()
        .ok_or_else(|| anyhow!("sandbox create lease body must be a JSON object"))?;
    object.insert(
        "workspace_root".to_string(),
        Value::String(workspace_root.to_string_lossy().to_string()),
    );
    Ok(body)
}

fn is_sandbox_create_lease_request(method: &Method, path: &str) -> bool {
    *method == Method::POST && normalize_http_path(path) == "/api/sandboxes/leases"
}

fn local_sandbox_workspace_root(workspace: &WorkspaceState) -> Result<PathBuf> {
    let root = workspace.absolute_root.join(".chatos").join("task-runner");
    fs::create_dir_all(root.as_path())
        .with_context(|| format!("create local sandbox workspace root {}", root.display()))?;
    Ok(root)
}

fn prepare_local_sandbox_workspace(
    request: &RelayRequest,
    state: &LocalState,
    response_body: &Value,
) -> Result<()> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let run_workspace = response_body
        .get("run_workspace")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local sandbox create lease response missing run_workspace"))?;
    let run_workspace = PathBuf::from(run_workspace);
    let baseline_workspace = local_sandbox_baseline_workspace(run_workspace.as_path())?;
    clear_directory(baseline_workspace.as_path())?;
    clear_directory(run_workspace.as_path())?;
    copy_workspace_contents_to_sandbox(
        workspace.absolute_root.as_path(),
        baseline_workspace.as_path(),
        workspace.absolute_root.as_path(),
    )?;
    copy_workspace_contents_to_sandbox(
        workspace.absolute_root.as_path(),
        run_workspace.as_path(),
        workspace.absolute_root.as_path(),
    )?;
    Ok(())
}

fn local_sandbox_baseline_workspace(run_workspace: &Path) -> Result<PathBuf> {
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("invalid local sandbox run_workspace"))?;
    Ok(run_root.join("baseline").join("workspace"))
}

fn copy_workspace_contents_to_sandbox(
    source: &Path,
    destination: &Path,
    root: &Path,
) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("create sandbox workspace {}", destination.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let source_path = entry.path();
        if should_skip_local_sandbox_copy(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry.file_type()?;
        let destination_path = destination.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            copy_workspace_contents_to_sandbox(
                source_path.as_path(),
                destination_path.as_path(),
                root,
            )?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::copy(source_path.as_path(), destination_path.as_path()).with_context(|| {
                format!(
                    "copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn should_skip_local_sandbox_copy(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative.components().next().is_some_and(
        |component| matches!(component, std::path::Component::Normal(name) if name == ".chatos"),
    )
}

fn validate_local_terminal_directory_change(
    line: &str,
    root_cwd: &Path,
    current_cwd: &mut PathBuf,
) -> Option<String> {
    let command = parse_local_terminal_directory_change(line)?;
    if command.has_extra_args {
        return Some(
            "Blocked: run directory-change commands alone (no chained arguments).".to_string(),
        );
    }
    if matches!(
        command.kind,
        LocalDirectoryChangeKind::Pushd | LocalDirectoryChangeKind::Popd
    ) {
        return Some("Blocked: pushd/popd are disabled for this restricted terminal.".to_string());
    }
    if let Some(target) = command.target.as_deref() {
        let target = target.trim();
        if target == "-" {
            return Some("Blocked: cd - is disabled in this restricted terminal.".to_string());
        }
        if has_dynamic_local_cd_syntax(target) {
            return Some(
                "Blocked: cd path cannot contain shell expansions or control operators."
                    .to_string(),
            );
        }
    }

    let target_is_absolute = command
        .target
        .as_deref()
        .map(|target| Path::new(target.trim()).is_absolute())
        .unwrap_or(false);
    let resolved = match resolve_local_cd_target(root_cwd, current_cwd, command.target.as_deref()) {
        Some(path) => path,
        None => {
            if target_is_absolute {
                return Some(
                    "Blocked: cannot verify absolute cd target (path does not resolve)."
                        .to_string(),
                );
            }
            return None;
        }
    };
    if !path_is_inside_root(resolved.as_path(), root_cwd) {
        return Some("Blocked: cannot leave terminal workspace.".to_string());
    }
    *current_cwd = resolved;
    None
}

fn validate_local_terminal_command(
    line: &str,
    root_cwd: &Path,
    current_cwd: &mut PathBuf,
) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if parse_local_terminal_directory_change(trimmed).is_some() {
        return validate_local_terminal_directory_change(trimmed, root_cwd, current_cwd);
    }
    validate_local_terminal_path_arguments(trimmed, root_cwd, current_cwd.as_path())
}

fn validate_local_terminal_path_arguments(
    line: &str,
    root_cwd: &Path,
    current_cwd: &Path,
) -> Option<String> {
    let words = split_local_shell_words(line.trim())?;
    for word in words {
        let word = word.trim();
        if word.is_empty() || word.starts_with('-') || word.contains("://") {
            continue;
        }
        if word.starts_with('~') {
            return Some(
                "Blocked: paths outside the terminal workspace are not allowed.".to_string(),
            );
        }
        let candidate = Path::new(word);
        if candidate.is_absolute() {
            let Ok(canonical) = fs::canonicalize(candidate) else {
                return Some("Blocked: cannot verify absolute path target.".to_string());
            };
            if !path_is_inside_root(canonical.as_path(), root_cwd) {
                return Some(
                    "Blocked: paths outside the terminal workspace are not allowed.".to_string(),
                );
            }
        } else if candidate.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            let resolved = current_cwd.join(candidate);
            let Ok(canonical) = fs::canonicalize(resolved.as_path()) else {
                return Some("Blocked: cannot verify parent-directory path target.".to_string());
            };
            if !path_is_inside_root(canonical.as_path(), root_cwd) {
                return Some(
                    "Blocked: paths outside the terminal workspace are not allowed.".to_string(),
                );
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalDirectoryChangeKind {
    Cd,
    SetLocation,
    Pushd,
    Popd,
}

#[derive(Debug, Clone)]
struct LocalDirectoryChangeCommand {
    kind: LocalDirectoryChangeKind,
    target: Option<String>,
    has_extra_args: bool,
}

fn parse_local_terminal_directory_change(line: &str) -> Option<LocalDirectoryChangeCommand> {
    let words = split_local_shell_words(line.trim())?;
    if words.is_empty() {
        return None;
    }
    match words[0].to_ascii_lowercase().as_str() {
        "cd" | "chdir" => parse_local_cd_command(words),
        "set-location" | "sl" => parse_local_set_location_command(words),
        "pushd" => Some(LocalDirectoryChangeCommand {
            kind: LocalDirectoryChangeKind::Pushd,
            target: words.get(1).cloned(),
            has_extra_args: words.len() > 2,
        }),
        "popd" => Some(LocalDirectoryChangeCommand {
            kind: LocalDirectoryChangeKind::Popd,
            target: None,
            has_extra_args: words.len() > 1,
        }),
        _ => None,
    }
}

fn parse_local_cd_command(words: Vec<String>) -> Option<LocalDirectoryChangeCommand> {
    let mut idx = 1;
    if idx < words.len() && words[idx].eq_ignore_ascii_case("/d") {
        idx += 1;
    }
    let target = words.get(idx).cloned();
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };
    Some(LocalDirectoryChangeCommand {
        kind: LocalDirectoryChangeKind::Cd,
        target,
        has_extra_args,
    })
}

fn parse_local_set_location_command(words: Vec<String>) -> Option<LocalDirectoryChangeCommand> {
    let mut idx = 1;
    if idx < words.len()
        && (words[idx].eq_ignore_ascii_case("-path")
            || words[idx].eq_ignore_ascii_case("-literalpath"))
    {
        idx += 1;
    }
    let target = words.get(idx).cloned();
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };
    Some(LocalDirectoryChangeCommand {
        kind: LocalDirectoryChangeKind::SetLocation,
        target,
        has_extra_args,
    })
}

fn split_local_shell_words(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None::<char>;
    for ch in input.chars() {
        match quote {
            Some(marker) => {
                if ch == marker {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                } else if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        words.push(current);
    }
    Some(words)
}

fn has_dynamic_local_cd_syntax(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed.starts_with('~')
        || trimmed
            .chars()
            .any(|ch| matches!(ch, '$' | '%' | '`' | ';' | '|' | '&' | '>' | '<'))
}

fn resolve_local_cd_target(
    root_cwd: &Path,
    current_cwd: &Path,
    target: Option<&str>,
) -> Option<PathBuf> {
    let raw_target = target.unwrap_or("").trim();
    if raw_target.is_empty() {
        return Some(root_cwd.to_path_buf());
    }
    let candidate = if Path::new(raw_target).is_absolute() {
        PathBuf::from(raw_target)
    } else {
        current_cwd.join(raw_target)
    };
    canonicalize_existing_dir(candidate.as_path()).ok()
}

fn path_is_inside_root(candidate: &Path, root: &Path) -> bool {
    let candidate = normalize_path_for_guard(candidate);
    let root = normalize_path_for_guard(root);
    candidate == root || candidate.starts_with(format!("{root}/").as_str())
}

fn normalize_path_for_guard(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

fn normalize_terminal_input(data: &str) -> String {
    if cfg!(windows) {
        data.replace("\r\n", "\r").replace('\n', "\r")
    } else {
        data.to_string()
    }
}

fn sanitize_terminal_command_line(command_line: &str) -> String {
    strip_terminal_ansi(command_line)
        .chars()
        .filter(|ch| !ch.is_control())
        .collect()
}

fn strip_terminal_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                let _ = chars.next();
                for marker in chars.by_ref() {
                    if ('@'..='~').contains(&marker) {
                        break;
                    }
                }
            }
            Some(']') => {
                let _ = chars.next();
                let mut previous_escape = false;
                for marker in chars.by_ref() {
                    if marker == '\u{7}' || (previous_escape && marker == '\\') {
                        break;
                    }
                    previous_escape = marker == '\u{1b}';
                }
            }
            Some(_) => {
                let _ = chars.next();
            }
            None => {}
        }
    }
    out
}

fn clear_terminal_input_line(command_line: &str) -> String {
    let mut seq = String::new();
    for _ in command_line.chars() {
        seq.push('\u{8}');
        seq.push(' ');
        seq.push('\u{8}');
    }
    seq
}

fn normalize_http_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(key, value)| {
            let key = key.as_str().to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "set-cookie" | "transfer-encoding" | "connection"
            ) {
                return None;
            }
            value.to_str().ok().map(|value| (key, value.to_string()))
        })
        .collect()
}

#[derive(Debug, Serialize)]
struct DockerStatusLocal {
    installed: bool,
    running: bool,
    version: Option<String>,
    error: Option<String>,
}

async fn status_payload(runtime: &LocalRuntime) -> Value {
    let state = runtime.state.read().await.clone();
    let connector_running = runtime
        .connector_task
        .lock()
        .await
        .as_ref()
        .map(|handle| !handle.is_finished())
        .unwrap_or(false);
    json!({
        "configured": state.auth.is_some(),
        "connector_running": connector_running,
        "cloud_base_url": state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()),
        "user_service_base_url": state.auth.as_ref().map(|auth| auth.user_service_base_url.as_str()),
        "device_id": state.device_id,
        "device_name": state.auth.as_ref().map(|auth| auth.device_name.as_str()),
        "user": state.auth.as_ref().and_then(|auth| auth.user.clone()),
        "workspaces": state.workspaces,
        "sandbox": {
            "enabled": state.sandbox.enabled,
            "backend": LOCAL_SANDBOX_BACKEND,
            "isolation": "local_docker",
            "selected_image_ref": state.sandbox.selected_image_ref,
        },
        "docker": docker_status_struct().await,
    })
}

async fn docker_status() -> Value {
    json!(docker_status_struct().await)
}

async fn docker_status_struct() -> DockerStatusLocal {
    let version = run_command_capture("docker", &["--version"], Duration::from_secs(5)).await;
    let Ok(version) = version else {
        return DockerStatusLocal {
            installed: false,
            running: false,
            version: None,
            error: Some("docker command is not available".to_string()),
        };
    };
    let version_text = first_non_empty_line(version.1.as_str())
        .or_else(|| first_non_empty_line(version.2.as_str()));
    let info = run_command_capture("docker", &["info"], Duration::from_secs(5)).await;
    match info {
        Ok((code, _, _)) if code == 0 => DockerStatusLocal {
            installed: true,
            running: true,
            version: version_text,
            error: None,
        },
        Ok((_, _, stderr)) => DockerStatusLocal {
            installed: true,
            running: false,
            version: version_text,
            error: normalize_optional(Some(stderr.as_str())),
        },
        Err(err) => DockerStatusLocal {
            installed: true,
            running: false,
            version: version_text,
            error: Some(err.to_string()),
        },
    }
}

async fn ensure_docker_running() -> Result<()> {
    let status = docker_status_struct().await;
    if !status.installed {
        return Err(anyhow!(
            "Docker is not installed or docker command is not in PATH"
        ));
    }
    if status.running {
        return Ok(());
    }
    start_docker_desktop().await?;
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < Duration::from_secs(60) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if docker_status_struct().await.running {
            return Ok(());
        }
    }
    Err(anyhow!("Docker did not become ready within 60 seconds"))
}

async fn start_docker_desktop() -> Result<()> {
    match std::env::consts::OS {
        "macos" => {
            let _ = tokio::process::Command::new("open")
                .args(["-a", "Docker"])
                .status()
                .await
                .context("start Docker Desktop")?;
        }
        "windows" => {
            let _ = tokio::process::Command::new("cmd")
                .args(["/C", "start", "", "Docker Desktop"])
                .status()
                .await
                .context("start Docker Desktop")?;
        }
        _ => {
            let _ = tokio::process::Command::new("systemctl")
                .args(["--user", "start", "docker"])
                .status()
                .await;
        }
    }
    Ok(())
}

async fn run_command_capture(
    program: &str,
    args: &[&str],
    timeout_duration: Duration,
) -> Result<(i32, String, String)> {
    let output = tokio::time::timeout(
        timeout_duration,
        tokio::process::Command::new(program).args(args).output(),
    )
    .await
    .with_context(|| format!("{program} timed out"))?
    .with_context(|| format!("run {program}"))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(output.stdout.as_slice()).into_owned(),
        String::from_utf8_lossy(output.stderr.as_slice()).into_owned(),
    ))
}

async fn upsert_sandbox_pairings(
    runtime: &LocalRuntime,
    enabled: bool,
) -> Result<(), LocalApiError> {
    let (cloud_base_url, access_token, device_id, workspaces) = {
        let state = runtime.state.read().await;
        let auth = state
            .auth
            .as_ref()
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?;
        let device_id = state
            .device_id
            .clone()
            .ok_or_else(|| LocalApiError::bad_request("device is not registered yet"))?;
        (
            auth.cloud_base_url.clone(),
            auth.access_token.clone(),
            device_id,
            state.workspaces.clone(),
        )
    };
    for workspace in workspaces {
        let response = runtime
            .http_client
            .post(
                api_url(
                    cloud_base_url.as_str(),
                    "/api/local-connectors/sandbox-pairings",
                )
                .as_str(),
            )
            .bearer_auth(access_token.as_str())
            .json(&json!({
                "device_id": device_id.as_str(),
                "workspace_id": workspace.id,
                "enabled": enabled,
                "sandbox_mode": "docker",
            }))
            .send()
            .await
            .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
        ensure_success(response.status(), "upsert sandbox pairing")
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    }
    Ok(())
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_required(value: &str, field: &str) -> Result<String, LocalApiError> {
    normalize_optional(Some(value))
        .ok_or_else(|| LocalApiError::bad_request(format!("{field} is required")))
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn workspace_for_request<'a>(
    state: &'a LocalState,
    workspace_id: &str,
) -> Result<&'a WorkspaceState> {
    state
        .workspace_by_id(workspace_id)
        .ok_or_else(|| anyhow!("workspace is not registered locally: {workspace_id}"))
}

fn request_cwd(request: &RelayRequest) -> Option<&str> {
    request
        .headers
        .get("x-local-connector-cwd")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestedPathOrigin {
    EmptyOrCurrent,
    ProjectRelative,
    WorkspaceAbsolute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedRequestedPath {
    relative_path: String,
    origin: RequestedPathOrigin,
}

fn normalize_request_workspace_relative_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<String> {
    let base = request_cwd(request)
        .map(normalize_relative_workspace_path)
        .transpose()?
        .filter(|value| value != ".");
    let requested = normalize_requested_path(workspace, request, requested)?;
    combine_request_path(base.as_deref(), requested)
}

fn normalize_requested_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<NormalizedRequestedPath> {
    let trimmed = requested.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
        return Ok(NormalizedRequestedPath {
            relative_path: ".".to_string(),
            origin: RequestedPathOrigin::EmptyOrCurrent,
        });
    }

    if let Some(relative_path) = connector_uri_workspace_relative_path(request, trimmed)? {
        return Ok(NormalizedRequestedPath {
            relative_path: normalize_relative_workspace_path(relative_path.as_str())?,
            origin: RequestedPathOrigin::WorkspaceAbsolute,
        });
    }

    if Path::new(trimmed).is_absolute() {
        return Ok(NormalizedRequestedPath {
            relative_path: absolute_workspace_relative_path(workspace, trimmed)?,
            origin: RequestedPathOrigin::WorkspaceAbsolute,
        });
    }

    Ok(NormalizedRequestedPath {
        relative_path: normalize_relative_workspace_path(trimmed)?,
        origin: RequestedPathOrigin::ProjectRelative,
    })
}

fn connector_uri_workspace_relative_path(
    request: &RelayRequest,
    requested: &str,
) -> Result<Option<String>> {
    let Some(stripped) = requested.strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX) else {
        return Ok(None);
    };
    let mut parts = stripped.split('/');
    let device_id = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local connector path is missing device id"))?;
    let workspace_id = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local connector path is missing workspace id"))?;
    if request
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|request_device_id| request_device_id != device_id)
    {
        return Err(anyhow!("local connector path targets another device"));
    }
    if workspace_id != request.workspace_id {
        return Err(anyhow!("local connector path targets another workspace"));
    }
    Ok(Some(parts.collect::<Vec<_>>().join("/")))
}

fn absolute_workspace_relative_path(workspace: &WorkspaceState, requested: &str) -> Result<String> {
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let requested_path = normalize_absolute_path_for_workspace(Path::new(requested.trim()));
    if !requested_path.starts_with(root.as_path()) {
        return Err(anyhow!("absolute path is outside authorized workspace"));
    }
    let relative = requested_path
        .strip_prefix(root.as_path())
        .map_err(|_| anyhow!("absolute path is outside authorized workspace"))?;
    normalize_relative_workspace_path(relative.to_string_lossy().as_ref())
}

fn normalize_absolute_path_for_workspace(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let mut suffix = PathBuf::new();
    let mut cursor = path;
    while let Some(parent) = cursor.parent() {
        if let Some(file_name) = cursor.file_name() {
            let mut next_suffix = PathBuf::from(file_name);
            next_suffix.push(suffix);
            suffix = next_suffix;
        }
        if let Ok(canonical_parent) = parent.canonicalize() {
            return canonical_parent.join(suffix);
        }
        cursor = parent;
    }
    path.to_path_buf()
}

fn normalize_relative_workspace_path(value: &str) -> Result<String> {
    let normalized = value.trim().replace('\\', "/");
    let stripped = normalized.trim_start_matches('/');
    if stripped.is_empty() || stripped == "." {
        return Ok(".".to_string());
    }

    let mut clean = PathBuf::new();
    for component in Path::new(stripped).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => clean.push(part),
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(anyhow!(
                    "workspace path contains unsupported parent/root component"
                ));
            }
        }
    }
    let value = clean.to_string_lossy().replace('\\', "/");
    Ok(if value.is_empty() {
        ".".to_string()
    } else {
        value
    })
}

fn combine_request_path(base: Option<&str>, requested: NormalizedRequestedPath) -> Result<String> {
    let requested_path = requested.relative_path.as_str();
    let Some(base) = base
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
    else {
        return Ok(requested.relative_path);
    };

    if requested_path == "." {
        return Ok(base.to_string());
    }
    if requested_path == base || requested_path.starts_with(format!("{base}/").as_str()) {
        return Ok(requested.relative_path);
    }
    if requested.origin == RequestedPathOrigin::WorkspaceAbsolute {
        return Err(anyhow!("path is outside current local project"));
    }
    Ok(format!("{base}/{requested_path}"))
}

#[cfg(test)]
fn resolve_request_workspace_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<PathBuf> {
    let combined = normalize_request_workspace_relative_path(workspace, request, requested)?;
    resolve_workspace_path(workspace, combined.as_str())
}

fn resolve_request_workspace_dir(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<PathBuf> {
    let combined = normalize_request_workspace_relative_path(workspace, request, requested)?;
    resolve_workspace_dir(workspace, combined.as_str())
}

fn resolve_workspace_path(workspace: &WorkspaceState, requested: &str) -> Result<PathBuf> {
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let safe_requested = requested.trim_start_matches('/');
    let requested_path = Path::new(safe_requested);
    if requested_path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(anyhow!(
            "write path contains unsupported parent/root component"
        ));
    }
    let candidate = root.join(requested_path);
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("resolve workspace path {}", candidate.display()))?;
    if !canonical.starts_with(root.as_path()) {
        return Err(anyhow!("path escapes authorized workspace"));
    }
    Ok(canonical)
}

fn resolve_workspace_dir(workspace: &WorkspaceState, requested: &str) -> Result<PathBuf> {
    let dir = resolve_workspace_path(workspace, requested)?;
    if !dir.is_dir() {
        return Err(anyhow!("cwd is not a directory: {}", dir.display()));
    }
    Ok(dir)
}

fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize workspace path {}", path.display()))?;
    if !canonical.is_dir() {
        return Err(anyhow!(
            "workspace path is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn relative_to_workspace(workspace: &WorkspaceState, path: &Path) -> String {
    path.strip_prefix(workspace.absolute_root.as_path())
        .ok()
        .map(|path| path.display().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".".to_string())
}

fn workspace_fingerprint(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.display().to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn output_text(bytes: &[u8]) -> (String, bool) {
    truncate_text(
        String::from_utf8_lossy(bytes).into_owned(),
        MAX_TERMINAL_OUTPUT_BYTES,
    )
}

fn command_history_entry_from_exec_result(
    state: &LocalState,
    request: &RelayRequest,
    context: &CommandExecutionContext,
    command: &str,
    args: &[String],
    cwd: &str,
    started_at: String,
    body: &Value,
) -> CommandHistoryEntry {
    let workspace = state.workspace_by_id(request.workspace_id.as_str());
    let timed_out = body
        .get("timed_out")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let success = body
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let error = body
        .get("error")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let exit_code = body
        .get("exit_code")
        .and_then(Value::as_i64)
        .map(|value| value as i32);
    let status = if timed_out {
        "timed_out"
    } else if success {
        "succeeded"
    } else {
        "failed"
    };
    CommandHistoryEntry {
        id: format!("cmd-{}", Uuid::new_v4()),
        source: context.source.clone(),
        workspace_id: Some(request.workspace_id.clone()),
        workspace_alias: workspace.map(|workspace| workspace.alias.clone()),
        cwd: Some(cwd.to_string()),
        command: command.to_string(),
        args: args.to_vec(),
        display: format_command_display(command, args),
        status: status.to_string(),
        exit_code,
        stdout_preview: body
            .get("stdout")
            .and_then(Value::as_str)
            .map(history_output_preview),
        stderr_preview: body
            .get("stderr")
            .and_then(Value::as_str)
            .map(history_output_preview),
        error,
        started_at,
        finished_at: Some(local_now_rfc3339()),
        request_id: context.request_id.clone(),
        terminal_session_id: context.terminal_session_id.clone(),
        sandbox_id: context.sandbox_id.clone(),
        tool_name: context.tool_name.clone(),
    }
}

fn command_history_entry_for_interactive_submission(
    state: &LocalState,
    request: &RelayRequest,
    terminal_session_id: &str,
    submission: InteractiveCommandSubmission,
) -> CommandHistoryEntry {
    let workspace = state.workspace_by_id(request.workspace_id.as_str());
    let cwd = workspace
        .map(|workspace| relative_to_workspace(workspace, submission.cwd.as_path()))
        .unwrap_or_else(|| submission.cwd.display().to_string());
    let status = if submission.blocked_reason.is_some() {
        "blocked"
    } else {
        "submitted"
    };
    CommandHistoryEntry {
        id: format!("cmd-{}", Uuid::new_v4()),
        source: "chatos_terminal_session".to_string(),
        workspace_id: Some(request.workspace_id.clone()),
        workspace_alias: workspace.map(|workspace| workspace.alias.clone()),
        cwd: Some(cwd),
        command: submission.command.clone(),
        args: Vec::new(),
        display: submission.command,
        status: status.to_string(),
        exit_code: None,
        stdout_preview: None,
        stderr_preview: None,
        error: submission.blocked_reason,
        started_at: local_now_rfc3339(),
        finished_at: None,
        request_id: Some(request.request_id.clone()),
        terminal_session_id: Some(terminal_session_id.to_string()),
        sandbox_id: None,
        tool_name: None,
    }
}

fn command_history_entry_for_sandbox_tool_call(
    state: &LocalState,
    request: &RelayRequest,
    context: &CommandExecutionContext,
    details: SandboxToolCallDetails,
    http_status: u16,
    body: &Value,
    started_at: String,
) -> CommandHistoryEntry {
    let workspace = state.workspace_by_id(request.workspace_id.as_str());
    let extracted = extract_sandbox_tool_result(body);
    let failed_http = !(200..300).contains(&http_status);
    let has_error = extracted.error.is_some();
    let timed_out = extracted.timed_out.unwrap_or(false);
    let exit_failed = extracted.exit_code.map(|code| code != 0).unwrap_or(false);
    let status = if timed_out {
        "timed_out"
    } else if failed_http || has_error || exit_failed {
        "failed"
    } else {
        "succeeded"
    };
    CommandHistoryEntry {
        id: format!("cmd-{}", Uuid::new_v4()),
        source: context.source.clone(),
        workspace_id: Some(request.workspace_id.clone()),
        workspace_alias: workspace.map(|workspace| workspace.alias.clone()),
        cwd: details.cwd,
        command: details.command,
        args: details.args,
        display: details.display,
        status: status.to_string(),
        exit_code: extracted.exit_code,
        stdout_preview: extracted
            .stdout
            .map(|value| history_output_preview(value.as_str())),
        stderr_preview: extracted
            .stderr
            .map(|value| history_output_preview(value.as_str())),
        error: extracted.error,
        started_at,
        finished_at: Some(local_now_rfc3339()),
        request_id: context.request_id.clone(),
        terminal_session_id: None,
        sandbox_id: context.sandbox_id.clone(),
        tool_name: context.tool_name.clone(),
    }
}

fn sandbox_tool_call_details(body: &Value) -> Option<SandboxToolCallDetails> {
    let (tool_name, arguments) = if body.get("method").and_then(Value::as_str) == Some("tools/call")
    {
        let params = body.get("params")?;
        (
            params
                .get("name")
                .and_then(Value::as_str)?
                .trim()
                .to_string(),
            params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({})),
        )
    } else {
        let tool_name = body
            .get("tool")
            .or_else(|| body.get("name"))
            .and_then(Value::as_str)?
            .trim()
            .to_string();
        (
            tool_name,
            body.get("arguments").cloned().unwrap_or_else(|| json!({})),
        )
    };
    if tool_name.is_empty() {
        return None;
    }
    let command =
        sandbox_command_from_arguments(&arguments).unwrap_or_else(|| format!("mcp:{tool_name}"));
    let cwd = arguments
        .get("cwd")
        .or_else(|| arguments.get("path"))
        .or_else(|| arguments.get("working_dir"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let display = if command == format!("mcp:{tool_name}") {
        format!("mcp:{tool_name} {}", compact_json(&arguments, 480))
    } else {
        command.clone()
    };
    Some(SandboxToolCallDetails {
        tool_name,
        command,
        args: Vec::new(),
        cwd,
        display,
    })
}

fn sandbox_command_from_arguments(arguments: &Value) -> Option<String> {
    ["command", "common", "cmd", "shell_command", "script"]
        .iter()
        .find_map(|key| {
            arguments
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

#[derive(Default)]
struct SandboxToolResultPreview {
    exit_code: Option<i32>,
    timed_out: Option<bool>,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
}

fn extract_sandbox_tool_result(body: &Value) -> SandboxToolResultPreview {
    let result_body = body
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .or_else(|| body.get("result").cloned())
        .unwrap_or_else(|| body.clone());
    let mut preview = SandboxToolResultPreview {
        exit_code: result_body
            .get("exit_code")
            .or_else(|| result_body.get("code"))
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        timed_out: result_body
            .get("timed_out")
            .or_else(|| result_body.get("timeout"))
            .and_then(Value::as_bool),
        stdout: result_body
            .get("stdout")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        stderr: result_body
            .get("stderr")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        error: extract_error_message(body).or_else(|| extract_error_message(&result_body)),
    };
    if preview.stdout.is_none() {
        preview.stdout = result_body
            .get("output")
            .or_else(|| result_body.get("text"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    preview
}

fn extract_error_message(value: &Value) -> Option<String> {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return value
            .get("error")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| Some("sandbox tool call failed".to_string()));
    }
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .map(ToOwned::to_owned)
}

fn compact_json(value: &Value, max_bytes: usize) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    truncate_text(text, max_bytes).0
}

fn history_output_preview(value: &str) -> String {
    truncate_text(value.to_string(), MAX_COMMAND_HISTORY_OUTPUT_PREVIEW_BYTES).0
}

fn normalize_history_source(source: &str) -> Option<String> {
    let normalized = source.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "chatos_terminal_exec"
        | "chatos_terminal_session"
        | "local_mcp"
        | "task_runner_sandbox"
        | "local_connector_ui" => Some(normalized),
        _ => None,
    }
}

fn format_command_display(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().map(|arg| shell_like_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_like_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn truncate_text(mut text: String, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text, false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
}

fn relay_error_response(
    message_type: &str,
    request_id: &str,
    status: u16,
    message: String,
) -> Value {
    RelayResponse {
        message_type: message_type.to_string(),
        request_id: request_id.to_string(),
        status,
        headers: BTreeMap::new(),
        body: json!({ "error": message }),
    }
    .to_value()
}

fn terminal_event(message_type: &str, terminal_session_id: &str, body: Value) -> Value {
    json!({
        "type": message_type,
        "terminal_session_id": terminal_session_id,
        "body": body,
    })
}

fn select_local_shell() -> String {
    if cfg!(windows) {
        return std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    }
    std::env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if Path::new("/bin/zsh").exists() {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

impl RelayResponse {
    fn to_value(self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|err| {
            json!({
                "type": "relay_response",
                "request_id": "",
                "status": 500,
                "body": {"error": err.to_string()}
            })
        })
    }
}

fn api_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn websocket_url(base: &str, path: &str, token: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    let scheme = if trimmed.starts_with("https://") {
        "wss://"
    } else {
        "ws://"
    };
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    format!(
        "{scheme}{without_scheme}{path}?token={}",
        urlencoding::encode(token)
    )
}

fn ensure_success(status: StatusCode, context: &str) -> Result<()> {
    if status.is_success() {
        Ok(())
    } else {
        Err(anyhow!("{context} failed with status {status}"))
    }
}

fn display_alias(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn required_env(key: &str) -> Result<String> {
    optional_env(key).ok_or_else(|| anyhow!("{key} is required"))
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_state_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chatos")
        .join("local_connector")
        .join("state.json")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

fn default_device_name() -> String {
    optional_env("HOSTNAME")
        .or_else(|| optional_env("COMPUTERNAME"))
        .unwrap_or_else(|| "Local Connector".to_string())
}

fn load_dotenv() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        Some(manifest_dir.join(".env")),
        manifest_dir.parent().map(|path| path.join(".env")),
        manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join(".env")),
    ]
    .into_iter()
    .flatten()
    {
        let _ = dotenvy::from_path(path);
    }
}

fn tracing_stdout(message: &str) {
    println!("[local-connector] {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("chatos-local-connector-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(path.as_path()).expect("create temp test dir");
        path
    }

    fn test_relay_request(workspace_id: &str) -> RelayRequest {
        RelayRequest {
            _message_type: "sandbox_request".to_string(),
            request_id: "req-test".to_string(),
            owner_user_id: Some("user-test".to_string()),
            device_id: Some("device-test".to_string()),
            workspace_id: workspace_id.to_string(),
            method: Some("POST".to_string()),
            path: Some("/api/sandboxes/leases".to_string()),
            headers: BTreeMap::new(),
            body: json!({}),
        }
    }

    fn test_workspace(root: &Path) -> WorkspaceState {
        WorkspaceState {
            id: "workspace-test".to_string(),
            absolute_root: fs::canonicalize(root).expect("canonical root"),
            alias: "workspace".to_string(),
            fingerprint: "fingerprint-test".to_string(),
        }
    }

    fn request_with_cwd(cwd: &str) -> RelayRequest {
        let mut request = test_relay_request("workspace-test");
        request
            .headers
            .insert("x-local-connector-cwd".to_string(), cwd.to_string());
        request
    }

    fn request_with_cwd_and_builtin_kinds(cwd: &str, kinds: &str) -> RelayRequest {
        let mut request = request_with_cwd(cwd);
        request.headers.insert(
            LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
            kinds.to_string(),
        );
        request
    }

    fn test_state_with_workspace(workspace: WorkspaceState) -> LocalState {
        LocalState {
            workspaces: vec![workspace],
            ..LocalState::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn local_mcp_exposes_builtin_compatible_tools_and_project_relative_args() {
        let root = temp_test_dir("builtin-compatible");
        let project = root.join("apps").join("web");
        fs::create_dir_all(project.as_path()).expect("create project");
        fs::write(project.join("package.json"), "{\"name\":\"web\"}\n").expect("write package");
        let workspace = test_workspace(root.as_path());
        let state = test_state_with_workspace(workspace);
        let request = request_with_cwd_and_builtin_kinds(
            "apps/web",
            "CodeMaintainerRead,CodeMaintainerWrite,TerminalController,BrowserTools",
        );
        let recorder = CommandHistoryRecorder {
            state_path: root.join("state.json"),
            state: Arc::new(RwLock::new(state.clone())),
        };

        let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert!(names.contains("read_file_raw"));
        assert!(names.contains("list_dir"));
        assert!(names.contains("write_file"));
        assert!(names.contains("execute_command"));
        assert!(names.contains("get_recent_logs"));
        assert!(names.contains("process"));
        assert!(names.contains("process_list"));
        assert!(names.contains("process_poll"));
        assert!(names.contains("process_log"));
        assert!(names.contains("process_wait"));
        assert!(names.contains("process_write"));
        assert!(names.contains("process_kill"));
        assert!(!names.contains("local_fs_read"));
        assert!(!names.contains("local_terminal_exec"));
        let browser_service = local_browser_tools_service_for_root(project.as_path(), &request)
            .expect("browser service");
        let browser_names = browser_service
            .list_tools()
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<BTreeSet<_>>();
        if browser_names.contains("browser_navigate") {
            assert!(names.contains("browser_navigate"));
            assert!(names.contains("browser_snapshot"));
            assert!(names.contains("browser_inspect"));
            assert!(!names.contains("browser_vision"));
        }

        let mut legacy_request = request_with_cwd_and_builtin_kinds(
            "apps/web",
            "CodeMaintainerRead,CodeMaintainerWrite",
        );
        legacy_request.body = json!({
            "jsonrpc": "2.0",
            "id": "legacy-tool",
            "method": "tools/call",
            "params": {
                "name": "local_fs_read",
                "arguments": { "path": "package.json" }
            }
        });
        let legacy_response = handle_mcp_body(&legacy_request, &state, &recorder)
            .await
            .expect("legacy tool response");
        assert_eq!(
            legacy_response
                .pointer("/error/code")
                .and_then(Value::as_i64),
            Some(-32601)
        );

        let read = call_builtin_compatible_local_tool(
            &request,
            &state,
            "read_file_raw",
            json!({ "path": "package.json", "with_line_numbers": false }),
            &recorder,
        )
        .await
        .expect("read call")
        .expect("read result");
        let structured = code_maintainer_structured_result(read);
        assert_eq!(
            structured.get("path").and_then(Value::as_str),
            Some("package.json")
        );
        assert_eq!(
            structured.get("content").and_then(Value::as_str),
            Some("{\"name\":\"web\"}\n")
        );

        let listed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "list_dir",
            json!({
                "path": "local://connector/device-test/workspace-test/apps/web",
                "max_entries": 20
            }),
            &recorder,
        )
        .await
        .expect("list call")
        .expect("list result");
        let structured = code_maintainer_structured_result(listed);
        assert!(structured
            .get("entries")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|entry| entry.get("path").and_then(Value::as_str) == Some("package.json")));

        let executed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "execute_command",
            json!({ "path": ".", "common": "pwd", "background": false }),
            &recorder,
        )
        .await
        .expect("execute call")
        .expect("execute result");
        let structured = code_maintainer_structured_result(executed);
        assert_eq!(
            structured.get("terminal_reused").and_then(Value::as_bool),
            Some(true)
        );
        assert!(structured
            .get("stdout")
            .or_else(|| structured.get("output"))
            .and_then(Value::as_str)
            .unwrap()
            .trim_end()
            .ends_with("apps/web"));

        let exported = call_builtin_compatible_local_tool(
            &request,
            &state,
            "execute_command",
            json!({ "path": ".", "common": "export CHATO_LOCAL_REUSE_TEST=ok", "background": false }),
            &recorder,
        )
        .await
        .expect("export call")
        .expect("export result");
        let structured = code_maintainer_structured_result(exported);
        assert_eq!(
            structured.get("terminal_reused").and_then(Value::as_bool),
            Some(true)
        );

        let echoed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "execute_command",
            json!({ "path": ".", "common": "echo $CHATO_LOCAL_REUSE_TEST", "background": false }),
            &recorder,
        )
        .await
        .expect("echo call")
        .expect("echo result");
        let structured = code_maintainer_structured_result(echoed);
        assert_eq!(
            structured
                .get("stdout")
                .or_else(|| structured.get("output"))
                .and_then(Value::as_str)
                .unwrap()
                .trim(),
            "ok"
        );

        let processes = call_builtin_compatible_local_tool(
            &request,
            &state,
            "process_list",
            json!({ "include_exited": true, "limit": 5 }),
            &recorder,
        )
        .await
        .expect("process list call")
        .expect("process list result");
        let structured = code_maintainer_structured_result(processes);
        assert!(structured
            .get("processes")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|process| process
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| command.contains("task terminal shell"))));

        let recent_logs = call_builtin_compatible_local_tool(
            &request,
            &state,
            "get_recent_logs",
            json!({ "per_terminal_limit": 20, "terminal_limit": 5 }),
            &recorder,
        )
        .await
        .expect("recent logs call")
        .expect("recent logs result");
        let structured = code_maintainer_structured_result(recent_logs);
        assert!(structured
            .get("terminals")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|terminal| terminal
                .get("logs")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .any(|log| log.get("content").and_then(Value::as_str) == Some("pwd"))));

        let context = local_terminal_controller_context_for_root(
            project.as_path(),
            &request,
            DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
        );
        LocalConnectorTerminalControllerStore
            .kill_sessions_for_context(context)
            .await
            .expect("cleanup local shell");
        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn local_mcp_without_selected_builtin_kinds_exposes_no_tools() {
        let root = temp_test_dir("no-selected-tools");
        let project = root.join("apps").join("web");
        fs::create_dir_all(project.as_path()).expect("create project");
        let workspace = test_workspace(root.as_path());
        let state = test_state_with_workspace(workspace);
        let request = request_with_cwd("apps/web");

        let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
        assert!(tools.is_empty());

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn local_mcp_respects_selected_builtin_kind_header() {
        let root = temp_test_dir("selected-tools");
        let project = root.join("apps").join("web");
        fs::create_dir_all(project.as_path()).expect("create project");
        fs::write(project.join("package.json"), "{\"name\":\"web\"}\n").expect("write package");
        let workspace = test_workspace(root.as_path());
        let state = test_state_with_workspace(workspace);
        let mut request = request_with_cwd_and_builtin_kinds("apps/web", "CodeMaintainerRead");
        let recorder = CommandHistoryRecorder {
            state_path: root.join("state.json"),
            state: Arc::new(RwLock::new(state.clone())),
        };

        let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert!(names.contains("read_file_raw"));
        assert!(names.contains("list_dir"));
        assert!(!names.contains("write_file"));
        assert!(!names.contains("execute_command"));
        assert!(!names.contains("browser_navigate"));

        request.body = json!({
            "jsonrpc": "2.0",
            "id": "blocked-write",
            "method": "tools/call",
            "params": {
                "name": "write_file",
                "arguments": { "path": "package.json", "content": "{}\n" }
            }
        });
        let response = handle_mcp_body(&request, &state, &recorder)
            .await
            .expect("blocked write response");
        assert_eq!(
            response.pointer("/error/code").and_then(Value::as_i64),
            Some(-32601)
        );

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn local_mcp_lifecycle_starts_and_cleans_task_terminal() {
        let root = temp_test_dir("lifecycle-terminal");
        let project = root.join("apps").join("web");
        fs::create_dir_all(project.as_path()).expect("create project");
        let workspace = test_workspace(root.as_path());
        let state = test_state_with_workspace(workspace);
        let recorder = CommandHistoryRecorder {
            state_path: root.join("state.json"),
            state: Arc::new(RwLock::new(state.clone())),
        };
        let mut request = request_with_cwd_and_builtin_kinds("apps/web", "TerminalController");
        request
            .headers
            .insert("x-task-runner-task-id".to_string(), "task-test".to_string());

        request.body = json!({
            "jsonrpc": "2.0",
            "id": "terminal-start",
            "method": "local_connector/terminal/start",
            "params": { "path": "." }
        });
        let started = handle_mcp_body(&request, &state, &recorder)
            .await
            .expect("start lifecycle terminal");
        assert_eq!(
            started.pointer("/result/status").and_then(Value::as_str),
            Some("running")
        );
        let started_terminal_id = started
            .pointer("/result/terminal_id")
            .and_then(Value::as_str)
            .expect("started terminal id")
            .to_string();

        let executed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "execute_command",
            json!({ "path": ".", "common": "echo lifecycle", "background": false }),
            &recorder,
        )
        .await
        .expect("execute on lifecycle shell")
        .expect("execute result");
        let structured = code_maintainer_structured_result(executed);
        assert_eq!(
            structured.get("terminal_id").and_then(Value::as_str),
            Some(started_terminal_id.as_str())
        );
        assert_eq!(
            structured.get("terminal_reused").and_then(Value::as_bool),
            Some(true)
        );

        let listed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "process_list",
            json!({ "include_exited": false, "limit": 10 }),
            &recorder,
        )
        .await
        .expect("process list call")
        .expect("process list result");
        let structured = code_maintainer_structured_result(listed);
        assert!(structured
            .get("processes")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|process| process
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| command.contains("task terminal shell"))));

        request.body = json!({
            "jsonrpc": "2.0",
            "id": "terminal-cleanup",
            "method": "local_connector/terminal/cleanup",
            "params": {}
        });
        let cleanup = handle_mcp_body(&request, &state, &recorder)
            .await
            .expect("cleanup lifecycle terminal");
        assert_eq!(
            cleanup.pointer("/result/ok").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            cleanup.pointer("/result/total").and_then(Value::as_u64),
            Some(1)
        );

        let listed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "process_list",
            json!({ "include_exited": true, "limit": 10 }),
            &recorder,
        )
        .await
        .expect("process list call after cleanup")
        .expect("process list result after cleanup");
        let structured = code_maintainer_structured_result(listed);
        assert_eq!(
            structured
                .get("processes")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            0
        );

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn local_connector_path_at_current_cwd_resolves_to_project_root() {
        let root = temp_test_dir("path-local-uri");
        let project = root
            .join("learn")
            .join("applocations")
            .join("react-fs-explorer");
        fs::create_dir_all(project.as_path()).expect("create project");
        let workspace = test_workspace(root.as_path());
        let request = request_with_cwd("learn/applocations/react-fs-explorer");

        let resolved = resolve_request_workspace_path(
            &workspace,
            &request,
            "local://connector/device-test/workspace-test/learn/applocations/react-fs-explorer",
        )
        .expect("resolve local connector project path");

        assert_eq!(
            normalize_path_for_guard(resolved.as_path()),
            normalize_path_for_guard(
                fs::canonicalize(project.as_path())
                    .expect("canonical project")
                    .as_path()
            )
        );
        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn absolute_workspace_path_at_current_cwd_resolves_to_project_root() {
        let root = temp_test_dir("path-absolute-project");
        let project = root
            .join("learn")
            .join("applocations")
            .join("react-fs-explorer");
        fs::create_dir_all(project.as_path()).expect("create project");
        let workspace = test_workspace(root.as_path());
        let request = request_with_cwd("learn/applocations/react-fs-explorer");

        let resolved = resolve_request_workspace_path(
            &workspace,
            &request,
            project.to_string_lossy().as_ref(),
        )
        .expect("resolve absolute project path");

        assert_eq!(
            normalize_path_for_guard(resolved.as_path()),
            normalize_path_for_guard(
                fs::canonicalize(project.as_path())
                    .expect("canonical project")
                    .as_path()
            )
        );
        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn workspace_root_absolute_path_is_clamped_to_current_project_cwd() {
        let root = temp_test_dir("path-absolute-root");
        let project = root
            .join("learn")
            .join("applocations")
            .join("react-fs-explorer");
        fs::create_dir_all(project.as_path()).expect("create project");
        let workspace = test_workspace(root.as_path());
        let request = request_with_cwd("learn/applocations/react-fs-explorer");

        let resolved = resolve_request_workspace_path(
            &workspace,
            &request,
            workspace.absolute_root.to_string_lossy().as_ref(),
        )
        .expect("resolve absolute workspace root path");

        assert_eq!(
            normalize_path_for_guard(resolved.as_path()),
            normalize_path_for_guard(
                fs::canonicalize(project.as_path())
                    .expect("canonical project")
                    .as_path()
            )
        );
        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn workspace_absolute_path_outside_current_project_is_rejected() {
        let root = temp_test_dir("path-absolute-outside");
        let project = root
            .join("learn")
            .join("applocations")
            .join("react-fs-explorer");
        let sibling = root.join("learn").join("other-project");
        fs::create_dir_all(project.as_path()).expect("create project");
        fs::create_dir_all(sibling.as_path()).expect("create sibling");
        let workspace = test_workspace(root.as_path());
        let request = request_with_cwd("learn/applocations/react-fs-explorer");

        let err = resolve_request_workspace_path(
            &workspace,
            &request,
            sibling.to_string_lossy().as_ref(),
        )
        .expect_err("sibling project should be outside current project cwd");

        assert!(err.to_string().contains("outside current local project"));
        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn relative_workspace_path_matching_cwd_is_not_duplicated() {
        let root = temp_test_dir("path-relative-cwd");
        let project = root
            .join("learn")
            .join("applocations")
            .join("react-fs-explorer");
        let package_json = project.join("package.json");
        fs::create_dir_all(project.as_path()).expect("create project");
        fs::write(package_json.as_path(), "{}").expect("write package");
        let workspace = test_workspace(root.as_path());
        let request = request_with_cwd("learn/applocations/react-fs-explorer");

        let resolved = resolve_request_workspace_path(
            &workspace,
            &request,
            "learn/applocations/react-fs-explorer/package.json",
        )
        .expect("resolve workspace-relative file path");

        assert_eq!(
            normalize_path_for_guard(resolved.as_path()),
            normalize_path_for_guard(
                fs::canonicalize(package_json.as_path())
                    .expect("canonical package")
                    .as_path()
            )
        );
        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn prepare_local_sandbox_workspace_clears_existing_run_copy() {
        let root = temp_test_dir("workspace-copy");
        let workspace_root = root.join("project");
        fs::create_dir_all(workspace_root.as_path()).expect("create project root");
        fs::write(workspace_root.join("keep.txt"), "current").expect("write project file");
        fs::create_dir_all(workspace_root.join(".chatos").join("task-runner"))
            .expect("create internal dir");
        fs::write(
            workspace_root
                .join(".chatos")
                .join("task-runner")
                .join("skip.txt"),
            "internal",
        )
        .expect("write internal file");

        let run_workspace = workspace_root
            .join(".chatos")
            .join("task-runner")
            .join("runs")
            .join("run-test")
            .join("input")
            .join("workspace");
        let baseline_workspace =
            local_sandbox_baseline_workspace(run_workspace.as_path()).expect("baseline path");
        fs::create_dir_all(run_workspace.as_path()).expect("create run workspace");
        fs::create_dir_all(baseline_workspace.as_path()).expect("create baseline workspace");
        fs::write(run_workspace.join("stale.txt"), "old").expect("write stale run file");
        fs::write(baseline_workspace.join("stale.txt"), "old").expect("write stale baseline file");

        let workspace = WorkspaceState {
            id: "workspace-test".to_string(),
            absolute_root: fs::canonicalize(workspace_root.as_path()).expect("canonical root"),
            alias: "project".to_string(),
            fingerprint: "fingerprint-test".to_string(),
        };
        let state = LocalState {
            workspaces: vec![workspace],
            ..LocalState::default()
        };
        prepare_local_sandbox_workspace(
            &test_relay_request("workspace-test"),
            &state,
            &json!({ "run_workspace": run_workspace.to_string_lossy() }),
        )
        .expect("prepare workspace");

        assert!(run_workspace.join("keep.txt").is_file());
        assert!(baseline_workspace.join("keep.txt").is_file());
        assert!(!run_workspace.join("stale.txt").exists());
        assert!(!baseline_workspace.join("stale.txt").exists());
        assert!(!run_workspace.join(".chatos").exists());
        assert!(!baseline_workspace.join(".chatos").exists());

        fs::remove_dir_all(root.as_path()).expect("cleanup temp test dir");
    }

    #[test]
    fn local_terminal_directory_guard_allows_descendants_and_blocks_escape() {
        let root = temp_test_dir("terminal-guard");
        let project = root.join("project");
        let child = project.join("child");
        fs::create_dir_all(child.as_path()).expect("create child");
        let project = fs::canonicalize(project.as_path()).expect("canonical project");
        let mut current = project.clone();

        assert!(validate_local_terminal_directory_change(
            "cd child",
            project.as_path(),
            &mut current,
        )
        .is_none());
        assert!(path_is_inside_root(current.as_path(), project.as_path()));

        assert!(
            validate_local_terminal_directory_change("cd ..", project.as_path(), &mut current,)
                .is_none()
        );
        assert_eq!(
            normalize_path_for_guard(current.as_path()),
            normalize_path_for_guard(project.as_path())
        );

        let blocked =
            validate_local_terminal_directory_change("cd ..", project.as_path(), &mut current);
        assert_eq!(
            blocked.as_deref(),
            Some("Blocked: cannot leave terminal workspace.")
        );

        let blocked_root =
            validate_local_terminal_directory_change("cd /", project.as_path(), &mut current);
        assert_eq!(
            blocked_root.as_deref(),
            Some("Blocked: cannot leave terminal workspace.")
        );

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn local_terminal_directory_guard_blocks_dynamic_and_pushd() {
        let root = temp_test_dir("terminal-guard-dynamic");
        let project = root.join("project");
        fs::create_dir_all(project.as_path()).expect("create project");
        let project = fs::canonicalize(project.as_path()).expect("canonical project");
        let mut current = project.clone();

        assert!(validate_local_terminal_directory_change(
            "cd $HOME",
            project.as_path(),
            &mut current
        )
        .is_some());
        assert!(validate_local_terminal_directory_change(
            "pushd .",
            project.as_path(),
            &mut current
        )
        .is_some());

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn local_terminal_directory_guard_blocks_ansi_wrapped_escape() {
        let root = temp_test_dir("terminal-guard-ansi");
        let project = root.join("project");
        let outside = root.join("outside");
        fs::create_dir_all(project.as_path()).expect("create project");
        fs::create_dir_all(outside.as_path()).expect("create outside");
        let project = fs::canonicalize(project.as_path()).expect("canonical project");
        let outside = fs::canonicalize(outside.as_path()).expect("canonical outside");
        let mut current = project.clone();
        let command = format!("\x1b[200~cd {}\x1b[201~", outside.display());
        let sanitized = sanitize_terminal_command_line(command.as_str());
        assert_eq!(sanitized, format!("cd {}", outside.display()));
        assert!(validate_local_terminal_directory_change(
            sanitized.as_str(),
            project.as_path(),
            &mut current,
        )
        .is_some());

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }

    #[test]
    fn local_terminal_command_guard_blocks_obvious_outside_path_arguments() {
        let root = temp_test_dir("terminal-guard-path-args");
        let project = root.join("project");
        let child = project.join("child");
        let outside = root.join("outside");
        fs::create_dir_all(child.as_path()).expect("create child");
        fs::create_dir_all(outside.as_path()).expect("create outside");
        fs::write(child.join("file.txt"), "ok").expect("write child file");
        fs::write(outside.join("secret.txt"), "nope").expect("write outside file");
        let project = fs::canonicalize(project.as_path()).expect("canonical project");
        let outside = fs::canonicalize(outside.as_path()).expect("canonical outside");
        let mut current = project.clone();

        assert!(
            validate_local_terminal_command("ls child", project.as_path(), &mut current,).is_none()
        );
        assert!(validate_local_terminal_command(
            "cat child/file.txt",
            project.as_path(),
            &mut current,
        )
        .is_none());
        assert!(validate_local_terminal_command("ls /", project.as_path(), &mut current).is_some());
        assert!(validate_local_terminal_command(
            format!("cat {}", outside.join("secret.txt").display()).as_str(),
            project.as_path(),
            &mut current,
        )
        .is_some());
        assert!(validate_local_terminal_command(
            "cat ../outside/secret.txt",
            project.as_path(),
            &mut current,
        )
        .is_some());

        fs::remove_dir_all(root.as_path()).expect("cleanup");
    }
}
