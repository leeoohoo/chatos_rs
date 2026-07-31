// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chatos_mcp_runtime::{
    invalidate_stdio_session, jsonrpc_stdio_call_with_timeout, McpStdioServer,
};
use chatos_plugin_management_sdk::PluginMcpCloudRuntimeBundle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::cloud_plugin_artifact::CloudPluginArtifactStore;
use crate::config::ServerConfig;

const WRAPPER_MODE: &str = "--internal-cloud-stdio-wrapper";
const PLUGIN_WRAPPER_MODE: &str = "--internal-plugin-stdio-wrapper";
const WRAPPER_SPEC_PATH_ENV: &str = "CHATOS_CLOUD_STDIO_LAUNCH_SPEC_PATH";
const MAX_WRAPPER_SPEC_BYTES: u64 = 512 * 1024;
const MAX_COMMAND_BYTES: usize = 256;
const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_ARGUMENTS_BYTES: usize = 128 * 1024;
const MAX_ENVIRONMENT_VARIABLES: usize = 128;
const MAX_ENVIRONMENT_BYTES: usize = 64 * 1024;
const MAX_IDENTITY_BYTES: usize = 256;
const MAX_SESSION_LIFETIME_SECONDS: i64 = 2 * 60 * 60 + 60;
const MIN_CALL_TIMEOUT_MS: u64 = 1_000;
const MAX_CALL_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const SAFE_PATH: &str = "/usr/local/go/bin:/usr/local/dotnet:/opt/chatos/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CloudStdioCallRequest {
    pub(crate) runtime_session_id: String,
    pub(crate) resource_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudStdioCallResponse {
    pub(crate) result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CloudStdioCloseResponse {
    pub(crate) closed: bool,
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
}

#[derive(Clone)]
struct RegisteredBinding {
    request_fingerprint: String,
    fingerprint: String,
    config: McpStdioServer,
    expires_at_unix: i64,
    launch_spec_path: PathBuf,
}

impl CloudStdioService {
    pub(crate) fn new(config: ServerConfig) -> Self {
        let artifacts = CloudPluginArtifactStore::new(config.state_dir.as_path());
        Self {
            config,
            artifacts,
            bindings: Arc::new(Mutex::new(HashMap::new())),
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
        let result = jsonrpc_stdio_call_with_timeout(
            &active.config,
            request.method.as_str(),
            request.params,
            Some(request.runtime_session_id.as_str()),
            timeout,
        )
        .await;
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
        let binding = self.bindings.lock().await.remove(key.as_str());
        if let Some(binding) = binding {
            invalidate_stdio_session(&binding.config);
            remove_launch_spec(binding.launch_spec_path.as_path());
            return Ok(CloudStdioCloseResponse { closed: true });
        }
        Ok(CloudStdioCloseResponse { closed: false })
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

fn binding_key(runtime_session_id: &str, resource_id: &str) -> Result<String, String> {
    let runtime_session_id = validated_identity(runtime_session_id, "runtime_session_id")?;
    let resource_id = validated_identity(resource_id, "resource_id")?;
    Ok(format!("{runtime_session_id}:{resource_id}"))
}

fn binding_request_fingerprint(request: &CloudStdioCallRequest) -> Result<String, String> {
    let payload = CloudStdioBindingFingerprint {
        command: request.command.as_str(),
        args: request.args.as_slice(),
        env: &request.env,
        cwd: request.cwd.as_deref(),
        plugin_artifact: request.plugin_artifact.as_ref(),
        plugin_workspace_write: request.plugin_workspace_write,
    };
    serde_json::to_vec(&payload)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|error| format!("serialize cloud stdio MCP binding failed: {error}"))
}

fn deterministic_sandbox_id(binding_key: &str, bundle_sha256: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"chatos.cloud-plugin-stdio-sandbox.v1\n");
    hasher.update(binding_key.as_bytes());
    hasher.update(b"\n");
    hasher.update(bundle_sha256.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

fn validated_identity<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!("cloud stdio MCP {field} is invalid"));
    }
    Ok(value)
}

fn validate_expiry(expires_at_unix: i64) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    if expires_at_unix <= now || expires_at_unix > now.saturating_add(MAX_SESSION_LIFETIME_SECONDS)
    {
        return Err("cloud stdio MCP session expiry is invalid".to_string());
    }
    Ok(())
}

fn validate_method(method: &str, params: &Value) -> Result<(), String> {
    if !matches!(method.trim(), "tools/list" | "tools/call") {
        return Err("cloud stdio MCP method is not allowed".to_string());
    }
    if !params.is_object() {
        return Err("cloud stdio MCP params must be an object".to_string());
    }
    if method.trim() == "tools/call"
        && params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_none_or(str::is_empty)
    {
        return Err("cloud stdio MCP tools/call.name is required".to_string());
    }
    Ok(())
}

fn validate_launch_command(spec: &CloudStdioLaunchSpec) -> Result<(), String> {
    let Some(identity) = spec.binding_identity.as_deref() else {
        return validate_command(spec.command.as_str(), spec.args.as_slice());
    };
    if identity.len() != 64
        || !identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || spec.args.first().map(String::as_str) != Some(PLUGIN_WRAPPER_MODE)
    {
        return Err("Plugin cloud stdio launch identity is invalid".to_string());
    }
    let expected = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("resolve sandbox Agent executable failed: {error}"))?;
    let command = Path::new(spec.command.as_str())
        .canonicalize()
        .map_err(|error| format!("resolve Plugin stdio wrapper failed: {error}"))?;
    if command != expected {
        return Err("Plugin cloud stdio wrapper identity changed".to_string());
    }
    Ok(())
}

fn validate_command(command: &str, args: &[String]) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty()
        || command.len() > MAX_COMMAND_BYTES
        || command.contains(['/', '\\', '\0'])
        || matches!(command, "." | "..")
    {
        return Err("cloud stdio MCP command must be a PATH-resolved executable name".to_string());
    }
    let shell = command.trim_end_matches(".exe").to_ascii_lowercase();
    let is_shell = matches!(
        shell.as_str(),
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish" | "cmd" | "powershell" | "pwsh"
    );
    let invokes_inline_command = args.iter().any(|arg| {
        matches!(
            arg.trim().to_ascii_lowercase().as_str(),
            "-c" | "/c" | "-command" | "-encodedcommand"
        )
    });
    if is_shell && invokes_inline_command {
        return Err("cloud stdio MCP shell inline command execution is forbidden".to_string());
    }
    Ok(())
}

fn validate_arguments(args: &[String]) -> Result<(), String> {
    if args.len() > MAX_ARGUMENTS
        || args
            .iter()
            .any(|arg| arg.len() > MAX_ARGUMENT_BYTES || arg.contains('\0'))
        || args.iter().map(String::len).sum::<usize>() > MAX_ARGUMENTS_BYTES
    {
        return Err("cloud stdio MCP arguments exceed the supported limits".to_string());
    }
    Ok(())
}

fn validate_environment(env: &BTreeMap<String, String>) -> Result<(), String> {
    if env.len() > MAX_ENVIRONMENT_VARIABLES
        || env
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > MAX_ENVIRONMENT_BYTES
    {
        return Err("cloud stdio MCP environment exceeds the supported limits".to_string());
    }
    for (name, value) in env {
        validate_environment_name(name)?;
        if value.contains('\0') {
            return Err("cloud stdio MCP environment contains an invalid value".to_string());
        }
    }
    Ok(())
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let normalized = name.to_ascii_uppercase();
    let controlled = matches!(
        normalized.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "CHATOS_WORKSPACE"
            | "CHATOS_SANDBOX_MCP_TOKEN"
            | "CHATOS_AGENT_TOKEN"
            | "CHATOS_CLOUD_STDIO_LAUNCH_SPEC_PATH"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || normalized.starts_with("LD_")
        || normalized.starts_with("DYLD_")
        || normalized.starts_with("XDG_")
        || normalized.starts_with("MCP_MANAGEMENT_")
        || normalized.starts_with("SANDBOX_MANAGER_");
    if !valid || controlled {
        return Err("cloud stdio MCP environment name is invalid or Host-controlled".to_string());
    }
    Ok(())
}

fn write_launch_spec(prepared: &PreparedBinding) -> Result<(), String> {
    if let Err(error) = std::fs::remove_file(prepared.launch_spec_path.as_path()) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("replace cloud stdio launch spec failed: {error}"));
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(prepared.launch_spec_path.as_path())
        .map_err(|error| format!("write cloud stdio launch spec failed: {error}"))?;
    use std::io::Write;
    let result = file
        .write_all(prepared.launch_spec_bytes.as_slice())
        .map_err(|error| format!("write cloud stdio launch spec failed: {error}"))
        .and_then(|_| {
            file.sync_all()
                .map_err(|error| format!("sync cloud stdio launch spec failed: {error}"))
        });
    if result.is_err() {
        drop(file);
        remove_launch_spec(prepared.launch_spec_path.as_path());
    }
    result
}

fn remove_launch_spec(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %path.display(), error = %error, "remove cloud stdio launch spec failed");
        }
    }
}

fn resolve_workspace_cwd(workspace: &Path, value: Option<&str>) -> Result<PathBuf, String> {
    let relative = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(".");
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("cloud stdio MCP cwd must remain relative to the workspace".to_string());
    }
    let resolved = canonical_directory(workspace.join(path).as_path(), "cwd")?;
    if !resolved.starts_with(workspace) {
        return Err("cloud stdio MCP cwd escapes the workspace".to_string());
    }
    Ok(resolved)
}

fn canonical_directory(path: &Path, field: &str) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("read cloud stdio MCP {field} failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "cloud stdio MCP {field} must be a non-symlink directory"
        ));
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize cloud stdio MCP {field} failed: {error}"))
}

fn validate_launch_paths(spec: &CloudStdioLaunchSpec) -> Result<(), String> {
    let workspace = canonical_directory(spec.workspace.as_path(), "workspace")?;
    let cwd = canonical_directory(spec.cwd.as_path(), "cwd")?;
    let home = canonical_directory(spec.home.as_path(), "HOME")?;
    let temp = canonical_directory(spec.temp.as_path(), "temp")?;
    if !cwd.starts_with(workspace.as_path())
        || !home.starts_with(temp.parent().unwrap_or(home.as_path()))
        || !temp.starts_with(home.parent().unwrap_or(temp.as_path()))
    {
        return Err("cloud stdio MCP launch paths do not match the sandbox binding".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (CloudStdioService, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let state_dir = temp.path().join("state");
        std::fs::create_dir_all(workspace.as_path()).unwrap();
        std::fs::create_dir_all(state_dir.as_path()).unwrap();
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            workspace,
            state_dir,
            auth_token: Some("secret".to_string()),
            project_id: Some("project-1".to_string()),
            user_id: Some("user-1".to_string()),
            max_file_bytes: 1024,
            max_write_bytes: 1024,
            search_limit: 10,
            terminal_idle_timeout_ms: 1_000,
            terminal_max_wait_ms: 1_000,
            terminal_max_output_chars: 1_000,
            disk_limit_bytes: None,
            extra_quota_roots: Vec::new(),
            permission_profile: "workspace_write".to_string(),
            command_sandbox_backend: "external".to_string(),
            additional_writable_roots: Vec::new(),
            host_home: None,
            effective_permissions: None,
        };
        (CloudStdioService::new(config), temp)
    }

    fn request(command: &str, args: Vec<String>) -> CloudStdioCallRequest {
        CloudStdioCallRequest {
            runtime_session_id: "mcp_session_1".to_string(),
            resource_id: "resource-1".to_string(),
            command: command.to_string(),
            args,
            env: BTreeMap::new(),
            cwd: None,
            plugin_artifact: None,
            plugin_workspace_write: false,
            method: "tools/list".to_string(),
            params: serde_json::json!({}),
            expires_at_unix: chrono::Utc::now().timestamp() + 60,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn command_rejects_absolute_paths_and_shell_eval() {
        assert!(validate_command("/usr/bin/node", &[]).is_err());
        assert!(validate_command("bash", &["-c".to_string(), "echo bad".to_string()]).is_err());
        assert!(validate_command("npx", &["-y".to_string(), "@example/mcp".to_string()]).is_ok());
    }

    #[test]
    fn environment_rejects_host_controlled_names() {
        for name in [
            "PATH",
            "CHATOS_SANDBOX_MCP_TOKEN",
            "LD_PRELOAD",
            "MCP_MANAGEMENT_INTERNAL_API_SECRET",
        ] {
            assert!(validate_environment(&BTreeMap::from([(
                name.to_string(),
                "secret".to_string(),
            )]))
            .is_err());
        }
        assert!(validate_environment(&BTreeMap::from([(
            "GITHUB_TOKEN".to_string(),
            "secret".to_string(),
        )]))
        .is_ok());
    }

    #[test]
    fn cwd_rejects_parent_traversal_and_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(workspace.join("nested")).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        assert_eq!(
            resolve_workspace_cwd(workspace.as_path(), Some("nested")).unwrap(),
            workspace.join("nested")
        );
        assert!(resolve_workspace_cwd(workspace.as_path(), Some("../outside")).is_err());
    }

    #[tokio::test]
    async fn active_session_rejects_runtime_binding_drift_and_can_close() {
        let (service, _temp) = service();
        let first_request = request("npx", vec!["-y".to_string()]);
        let first = service
            .prepare_binding(
                &first_request,
                binding_request_fingerprint(&first_request).unwrap(),
            )
            .await
            .unwrap();
        assert!(service.register_binding(&first).await.unwrap());
        assert!(first.launch_spec_path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(first.launch_spec_path.as_path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
        assert!(!service.register_binding(&first).await.unwrap());

        let changed_request = request("node", vec!["server.js".to_string()]);
        let changed = service
            .prepare_binding(
                &changed_request,
                binding_request_fingerprint(&changed_request).unwrap(),
            )
            .await
            .unwrap();
        assert!(service.register_binding(&changed).await.is_err());
        let closed = service
            .close(CloudStdioCloseRequest {
                runtime_session_id: "mcp_session_1".to_string(),
                resource_id: "resource-1".to_string(),
            })
            .await
            .unwrap();
        assert!(closed.closed);
        assert!(!first.launch_spec_path.exists());
    }
}
