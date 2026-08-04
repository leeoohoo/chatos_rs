// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chatos_mcp_runtime::{
    invalidate_stdio_session, jsonrpc_stdio_call_with_timeout, McpStdioServer,
};
use chatos_plugin_management_sdk::PluginMcpCloudRuntimeBundle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::cloud_plugin_artifact::CloudPluginArtifactStore;
use crate::config::ServerConfig;

mod validation;
use validation::*;

const WRAPPER_MODE: &str = "--internal-cloud-stdio-wrapper";
const PLUGIN_WRAPPER_MODE: &str = "--internal-plugin-stdio-wrapper";
const WRAPPER_SPEC_PATH_ENV: &str = "CHATOS_CLOUD_STDIO_LAUNCH_SPEC_PATH";
const MAX_WRAPPER_SPEC_BYTES: u64 = 512 * 1024;
const MAX_COMMAND_BYTES: usize = 256;
const MAX_IDENTITY_BYTES: usize = 256;
const MAX_ACTIVE_INVOCATIONS: usize = 256;
const MAX_SESSION_LIFETIME_SECONDS: i64 = 2 * 60 * 60 + 60;
const MIN_CALL_TIMEOUT_MS: u64 = 1_000;
const MAX_CALL_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const CANCELLATION_ACK_TIMEOUT: Duration = Duration::from_secs(4);
const INVOCATION_RUNNING: u8 = 0;
const INVOCATION_CANCELLED: u8 = 1;
const INVOCATION_COMPLETED: u8 = 2;
const SAFE_PATH: &str = "/usr/local/go/bin:/usr/local/dotnet:/opt/chatos/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CloudStdioCallRequest {
    pub(crate) runtime_session_id: String,
    pub(crate) resource_id: String,
    #[serde(default)]
    pub(crate) invocation_id: Option<String>,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) plugin_artifact: Option<PluginMcpCloudRuntimeBundle>,
    #[serde(default)]
    pub(crate) plugin_workspace_write: bool,
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) params: Value,
    pub(crate) expires_at_unix: i64,
    pub(crate) timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CloudStdioCloseRequest {
    pub(crate) runtime_session_id: String,
    pub(crate) resource_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CloudStdioCancelRequest {
    pub(crate) runtime_session_id: String,
    pub(crate) resource_id: String,
    pub(crate) invocation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudStdioCallResponse {
    pub(crate) result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudStdioCloseResponse {
    pub(crate) closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudStdioCancelResponse {
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudStdioLaunchSpec {
    binding_identity: Option<String>,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    cwd: PathBuf,
    workspace: PathBuf,
    home: PathBuf,
    temp: PathBuf,
}

#[derive(Serialize)]
struct CloudStdioBindingFingerprint<'a> {
    command: &'a str,
    args: &'a [String],
    env: &'a BTreeMap<String, String>,
    cwd: Option<&'a str>,
    plugin_artifact: Option<&'a PluginMcpCloudRuntimeBundle>,
    plugin_workspace_write: bool,
}

struct ActiveBinding {
    key: String,
    fingerprint: String,
    config: McpStdioServer,
}

#[derive(Clone)]
pub(crate) struct CloudStdioService {
    config: ServerConfig,
    artifacts: CloudPluginArtifactStore,
    bindings: Arc<Mutex<HashMap<String, RegisteredBinding>>>,
    active_invocations: Arc<Mutex<HashMap<String, ActiveInvocation>>>,
}

#[derive(Clone)]
struct RegisteredBinding {
    request_fingerprint: String,
    fingerprint: String,
    config: McpStdioServer,
    expires_at_unix: i64,
    launch_spec_path: PathBuf,
}

#[derive(Clone)]
struct ActiveInvocation {
    binding_key: String,
    config: McpStdioServer,
    cancellation: CancellationToken,
    state: Arc<AtomicU8>,
    state_changed: Arc<Notify>,
}

struct ActiveInvocationGuard {
    active: ActiveInvocation,
}

impl Drop for ActiveInvocationGuard {
    fn drop(&mut self) {
        if self
            .active
            .state
            .compare_exchange(
                INVOCATION_RUNNING,
                INVOCATION_CANCELLED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            self.active.cancellation.cancel();
            invalidate_stdio_session(&self.active.config);
            self.active.state_changed.notify_one();
        }
    }
}

impl CloudStdioService {
    pub(crate) fn new(config: ServerConfig) -> Self {
        let artifacts = CloudPluginArtifactStore::new(config.state_dir.as_path());
        Self {
            config,
            artifacts,
            bindings: Arc::new(Mutex::new(HashMap::new())),
            active_invocations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn call(
        &self,
        request: CloudStdioCallRequest,
    ) -> Result<CloudStdioCallResponse, String> {
        validate_method(request.method.as_str(), &request.params)?;
        validate_expiry(request.expires_at_unix)?;
        let request_fingerprint = binding_request_fingerprint(&request)?;
        let active = if let Some(active) = self
            .active_binding(&request, request_fingerprint.as_str())
            .await?
        {
            active
        } else {
            let prepared = self.prepare_binding(&request, request_fingerprint).await?;
            let inserted = self.register_binding(&prepared).await?;
            if inserted {
                self.schedule_expiry(
                    prepared.key.clone(),
                    prepared.fingerprint.clone(),
                    request.expires_at_unix,
                );
            }
            ActiveBinding {
                key: prepared.key,
                fingerprint: prepared.fingerprint,
                config: prepared.config,
            }
        };
        let timeout = Duration::from_millis(
            request
                .timeout_ms
                .clamp(MIN_CALL_TIMEOUT_MS, MAX_CALL_TIMEOUT_MS),
        );
        let invocation = self
            .register_invocation(&request, active.key.as_str(), &active.config)
            .await?;
        let result = if let Some(invocation) = invocation {
            let guard = ActiveInvocationGuard {
                active: invocation.clone(),
            };
            let result = {
                let call = jsonrpc_stdio_call_with_timeout(
                    &active.config,
                    request.method.as_str(),
                    request.params,
                    Some(request.runtime_session_id.as_str()),
                    timeout,
                );
                tokio::pin!(call);
                tokio::select! {
                    biased;
                    _ = invocation.cancellation.cancelled() => None,
                    result = &mut call => Some(result),
                }
            };
            let result = match result {
                None => {
                    invalidate_stdio_session(&active.config);
                    mark_invocation_state(&invocation, INVOCATION_CANCELLED);
                    Err("cloud stdio MCP invocation was cancelled".to_string())
                }
                Some(result) => {
                    let completed = invocation
                        .state
                        .compare_exchange(
                            INVOCATION_RUNNING,
                            INVOCATION_COMPLETED,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok();
                    invocation.state_changed.notify_one();
                    if completed {
                        result
                    } else {
                        invalidate_stdio_session(&active.config);
                        Err("cloud stdio MCP invocation was cancelled".to_string())
                    }
                }
            };
            self.remove_invocation_if_matches(
                request
                    .invocation_id
                    .as_deref()
                    .expect("registered tool invocation has an id"),
                &invocation,
            )
            .await;
            drop(guard);
            result
        } else {
            jsonrpc_stdio_call_with_timeout(
                &active.config,
                request.method.as_str(),
                request.params,
                Some(request.runtime_session_id.as_str()),
                timeout,
            )
            .await
        };
        match result {
            Ok(result) => Ok(CloudStdioCallResponse { result }),
            Err(error) => {
                self.remove_binding_if_matches(active.key.as_str(), active.fingerprint.as_str())
                    .await;
                Err(format!("cloud stdio MCP invocation failed: {error}"))
            }
        }
    }

    pub(crate) async fn close(
        &self,
        request: CloudStdioCloseRequest,
    ) -> Result<CloudStdioCloseResponse, String> {
        let key = binding_key(
            request.runtime_session_id.as_str(),
            request.resource_id.as_str(),
        )?;
        self.cancel_binding_invocations(key.as_str()).await;
        let binding = self.bindings.lock().await.remove(key.as_str());
        if let Some(binding) = binding {
            invalidate_stdio_session(&binding.config);
            remove_launch_spec(binding.launch_spec_path.as_path());
            return Ok(CloudStdioCloseResponse { closed: true });
        }
        Ok(CloudStdioCloseResponse { closed: false })
    }

    pub(crate) async fn cancel(
        &self,
        request: CloudStdioCancelRequest,
    ) -> Result<CloudStdioCancelResponse, String> {
        let binding_key = binding_key(
            request.runtime_session_id.as_str(),
            request.resource_id.as_str(),
        )?;
        let invocation_id = validated_identity(request.invocation_id.as_str(), "invocation_id")?;
        let active = {
            let mut invocations = self.active_invocations.lock().await;
            invocations.retain(|_, invocation| {
                invocation.state.load(Ordering::SeqCst) == INVOCATION_RUNNING
            });
            invocations.get(invocation_id).cloned()
        };
        let Some(active) = active else {
            return Ok(CloudStdioCancelResponse {
                status: "invocation_not_found".to_string(),
            });
        };
        if active.binding_key != binding_key {
            return Err(
                "cloud stdio MCP invocation does not match its runtime binding".to_string(),
            );
        }
        active.cancellation.cancel();
        invalidate_stdio_session(&active.config);
        let status = wait_for_invocation_terminal(&active).await;
        self.remove_invocation_if_matches(invocation_id, &active)
            .await;
        Ok(CloudStdioCancelResponse {
            status: status.to_string(),
        })
    }

    async fn register_invocation(
        &self,
        request: &CloudStdioCallRequest,
        binding_key: &str,
        config: &McpStdioServer,
    ) -> Result<Option<ActiveInvocation>, String> {
        if request.method.trim() != "tools/call" {
            if request.invocation_id.is_some() {
                return Err(
                    "cloud stdio MCP invocation_id is only allowed for tools/call".to_string(),
                );
            }
            return Ok(None);
        }
        let invocation_id = request
            .invocation_id
            .as_deref()
            .ok_or_else(|| "cloud stdio MCP tools/call requires invocation_id".to_string())?;
        let invocation_id = validated_identity(invocation_id, "invocation_id")?;
        let active = ActiveInvocation {
            binding_key: binding_key.to_string(),
            config: config.clone(),
            cancellation: CancellationToken::new(),
            state: Arc::new(AtomicU8::new(INVOCATION_RUNNING)),
            state_changed: Arc::new(Notify::new()),
        };
        let mut invocations = self.active_invocations.lock().await;
        invocations
            .retain(|_, invocation| invocation.state.load(Ordering::SeqCst) == INVOCATION_RUNNING);
        if invocations.len() >= MAX_ACTIVE_INVOCATIONS {
            return Err("cloud stdio MCP active invocation capacity was reached".to_string());
        }
        if invocations.contains_key(invocation_id) {
            return Err("cloud stdio MCP invocation_id is already active".to_string());
        }
        invocations.insert(invocation_id.to_string(), active.clone());
        Ok(Some(active))
    }

    async fn remove_invocation_if_matches(&self, invocation_id: &str, expected: &ActiveInvocation) {
        let mut invocations = self.active_invocations.lock().await;
        if invocations
            .get(invocation_id)
            .is_some_and(|current| Arc::ptr_eq(&current.state, &expected.state))
        {
            invocations.remove(invocation_id);
        }
    }

    async fn cancel_binding_invocations(&self, binding_key: &str) {
        let active = {
            let mut invocations = self.active_invocations.lock().await;
            let ids = invocations
                .iter()
                .filter(|(_, invocation)| invocation.binding_key == binding_key)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| invocations.remove(id.as_str()))
                .collect::<Vec<_>>()
        };
        for invocation in active {
            invocation.cancellation.cancel();
            invalidate_stdio_session(&invocation.config);
        }
    }

    async fn prepare_binding(
        &self,
        request: &CloudStdioCallRequest,
        request_fingerprint: String,
    ) -> Result<PreparedBinding, String> {
        let key = binding_key(
            request.runtime_session_id.as_str(),
            request.resource_id.as_str(),
        )?;
        validate_arguments(request.args.as_slice())?;
        validate_environment(&request.env)?;
        let workspace = canonical_directory(self.config.workspace.as_path(), "workspace")?;
        let runtime_key = hex::encode(Sha256::digest(key.as_bytes()));
        let runtime_root = self.config.state_dir.join("cloud-stdio").join(runtime_key);
        let home = runtime_root.join("home");
        let cache = runtime_root.join("cache");
        let temp = runtime_root.join("tmp");
        for (label, path) in [("HOME", &home), ("cache", &cache), ("temp", &temp)] {
            std::fs::create_dir_all(path.as_path())
                .map_err(|error| format!("create cloud stdio {label} directory failed: {error}"))?;
        }
        let wrapper = std::env::current_exe()
            .map_err(|error| format!("resolve sandbox Agent executable failed: {error}"))?;
        let (binding_identity, command, args, cwd) = if let Some(bundle) =
            request.plugin_artifact.as_ref()
        {
            if !request.command.contains('/') {
                return Err("Plugin artifact mount requires a package-relative command".to_string());
            }
            let artifact = self
                .artifacts
                .materialize(bundle, request.command.as_str(), request.cwd.as_deref())
                .await?;
            let mut args = vec![
                PLUGIN_WRAPPER_MODE.to_string(),
                "--sandbox-id".to_string(),
                deterministic_sandbox_id(key.as_str(), bundle.bundle_sha256.as_str()),
                "--plugin-root".to_string(),
                artifact.plugin_root.to_string_lossy().into_owned(),
                "--state-root".to_string(),
                home.to_string_lossy().into_owned(),
                "--cache-root".to_string(),
                cache.to_string_lossy().into_owned(),
                "--temp-root".to_string(),
                temp.to_string_lossy().into_owned(),
                "--package-index".to_string(),
                artifact.package_index.to_string_lossy().into_owned(),
                "--cwd".to_string(),
                artifact.cwd.to_string_lossy().into_owned(),
            ];
            if request.plugin_workspace_write {
                args.push("--workspace-root".to_string());
                args.push(workspace.to_string_lossy().into_owned());
            }
            for name in request.env.keys() {
                args.push("--env".to_string());
                args.push(name.clone());
            }
            args.push("--".to_string());
            args.push(artifact.command.to_string_lossy().into_owned());
            args.extend(request.args.clone());
            (
                Some(bundle.bundle_sha256.clone()),
                wrapper.to_string_lossy().into_owned(),
                args,
                workspace.clone(),
            )
        } else {
            if request.plugin_workspace_write {
                return Err(
                    "Plugin workspace write binding requires an immutable artifact".to_string(),
                );
            }
            validate_command(request.command.as_str(), request.args.as_slice())?;
            (
                None,
                request.command.trim().to_string(),
                request.args.clone(),
                resolve_workspace_cwd(workspace.as_path(), request.cwd.as_deref())?,
            )
        };
        let launch = CloudStdioLaunchSpec {
            binding_identity,
            command,
            args,
            env: request.env.clone(),
            cwd,
            workspace: workspace.clone(),
            home,
            temp,
        };
        let launch_bytes = serde_json::to_vec(&launch)
            .map_err(|error| format!("serialize cloud stdio launch spec failed: {error}"))?;
        let fingerprint = hex::encode(Sha256::digest(launch_bytes.as_slice()));
        let launch_spec_path = runtime_root.join("launch.json");
        let config = McpStdioServer::new(
            format!("cloud-stdio-{}", request.resource_id.trim()),
            wrapper.to_string_lossy(),
        )
        .with_args([WRAPPER_MODE])
        .with_cwd(workspace.to_string_lossy())
        .with_env(HashMap::from([(
            WRAPPER_SPEC_PATH_ENV.to_string(),
            launch_spec_path.to_string_lossy().to_string(),
        )]))
        .with_user_id(key.clone());
        Ok(PreparedBinding {
            key,
            request_fingerprint,
            fingerprint,
            config,
            expires_at_unix: request.expires_at_unix,
            launch_spec_path,
            launch_spec_bytes: launch_bytes,
        })
    }

    async fn register_binding(&self, prepared: &PreparedBinding) -> Result<bool, String> {
        let mut bindings = self.bindings.lock().await;
        if let Some(existing) = bindings.get(prepared.key.as_str()) {
            if existing.request_fingerprint != prepared.request_fingerprint
                || existing.fingerprint != prepared.fingerprint
                || existing.expires_at_unix != prepared.expires_at_unix
            {
                return Err(
                    "cloud stdio MCP runtime binding changed during an active session".to_string(),
                );
            }
            return Ok(false);
        }
        write_launch_spec(prepared)?;
        bindings.insert(
            prepared.key.clone(),
            RegisteredBinding {
                request_fingerprint: prepared.request_fingerprint.clone(),
                fingerprint: prepared.fingerprint.clone(),
                config: prepared.config.clone(),
                expires_at_unix: prepared.expires_at_unix,
                launch_spec_path: prepared.launch_spec_path.clone(),
            },
        );
        Ok(true)
    }

    async fn active_binding(
        &self,
        request: &CloudStdioCallRequest,
        request_fingerprint: &str,
    ) -> Result<Option<ActiveBinding>, String> {
        let key = binding_key(
            request.runtime_session_id.as_str(),
            request.resource_id.as_str(),
        )?;
        let bindings = self.bindings.lock().await;
        let Some(binding) = bindings.get(key.as_str()) else {
            return Ok(None);
        };
        if binding.request_fingerprint != request_fingerprint
            || binding.expires_at_unix != request.expires_at_unix
        {
            return Err(
                "cloud stdio MCP runtime binding changed during an active session".to_string(),
            );
        }
        Ok(Some(ActiveBinding {
            key,
            fingerprint: binding.fingerprint.clone(),
            config: binding.config.clone(),
        }))
    }

    fn schedule_expiry(&self, key: String, fingerprint: String, expires_at_unix: i64) {
        let service = self.clone();
        tokio::spawn(async move {
            let now = chrono::Utc::now().timestamp();
            let wait_seconds = expires_at_unix.saturating_sub(now).max(0) as u64;
            tokio::time::sleep(Duration::from_secs(wait_seconds)).await;
            service
                .remove_binding_if_matches(key.as_str(), fingerprint.as_str())
                .await;
        });
    }

    async fn remove_binding_if_matches(&self, key: &str, fingerprint: &str) -> bool {
        let binding = {
            let mut bindings = self.bindings.lock().await;
            if bindings
                .get(key)
                .is_none_or(|binding| binding.fingerprint != fingerprint)
            {
                return false;
            }
            bindings.remove(key)
        };
        if let Some(binding) = binding {
            invalidate_stdio_session(&binding.config);
            remove_launch_spec(binding.launch_spec_path.as_path());
            return true;
        }
        false
    }
}

struct PreparedBinding {
    key: String,
    request_fingerprint: String,
    fingerprint: String,
    config: McpStdioServer,
    expires_at_unix: i64,
    launch_spec_path: PathBuf,
    launch_spec_bytes: Vec<u8>,
}

fn mark_invocation_state(invocation: &ActiveInvocation, state: u8) {
    let _ = invocation.state.compare_exchange(
        INVOCATION_RUNNING,
        state,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    invocation.state_changed.notify_one();
}

async fn wait_for_invocation_terminal(invocation: &ActiveInvocation) -> &'static str {
    let wait = async {
        loop {
            match invocation.state.load(Ordering::SeqCst) {
                INVOCATION_CANCELLED => return "cancelled",
                INVOCATION_COMPLETED => return "already_completed",
                _ => invocation.state_changed.notified().await,
            }
        }
    };
    tokio::time::timeout(CANCELLATION_ACK_TIMEOUT, wait)
        .await
        .unwrap_or("cancel_requested")
}

pub(crate) fn is_internal_cloud_stdio_wrapper() -> bool {
    std::env::args().nth(1).as_deref() == Some(WRAPPER_MODE)
}

pub(crate) fn run_internal_cloud_stdio_wrapper() -> Result<i32, String> {
    let path = std::env::var_os(WRAPPER_SPEC_PATH_ENV)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "cloud stdio wrapper launch spec path is missing".to_string())?;
    let metadata = std::fs::symlink_metadata(path.as_path())
        .map_err(|_| "cloud stdio wrapper launch spec is unavailable".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_WRAPPER_SPEC_BYTES
    {
        return Err("cloud stdio wrapper launch spec is invalid".to_string());
    }
    let bytes = std::fs::read(path.as_path())
        .map_err(|_| "cloud stdio wrapper launch spec is unavailable".to_string())?;
    let spec = serde_json::from_slice::<CloudStdioLaunchSpec>(bytes.as_slice())
        .map_err(|_| "cloud stdio wrapper launch spec is invalid".to_string())?;
    validate_launch_command(&spec)?;
    validate_arguments(spec.args.as_slice())?;
    validate_environment(&spec.env)?;
    validate_launch_paths(&spec)?;
    let mut command = std::process::Command::new(spec.command.as_str());
    command
        .args(&spec.args)
        .current_dir(spec.cwd.as_path())
        .env_clear()
        .env("PATH", SAFE_PATH)
        .env("HOME", spec.home.as_os_str())
        .env("TMPDIR", spec.temp.as_os_str())
        .env("TMP", spec.temp.as_os_str())
        .env("TEMP", spec.temp.as_os_str())
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("CHATOS_WORKSPACE", spec.workspace.as_os_str())
        .envs(&spec.env)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().to_string())
    }
    #[cfg(not(unix))]
    {
        let status = command.status().map_err(|error| error.to_string())?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests;
