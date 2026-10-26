// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalPluginHookBundleSpec, LocalPluginHookDefinition};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};

const SANDBOX_EXECUTABLE: &str = "/usr/bin/sandbox-exec";
const SHELL_EXECUTABLE: &str = "/bin/sh";
const MAXIMUM_STDOUT_BYTES: usize = 512 * 1_024;
const MAXIMUM_STDERR_BYTES: usize = 64 * 1_024;
const TERMINATION_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct LocalHookInvocation {
    pub bundle: LocalPluginHookBundleSpec,
    pub hook: LocalPluginHookDefinition,
    pub stdin_json: Vec<u8>,
    pub workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationResult {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_overflow: bool,
    pub stderr_overflow: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
}

#[async_trait]
pub trait LocalHookRunner: Send + Sync {
    async fn run(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String>;
}

#[derive(Debug, Clone)]
pub struct MacOsSeatbeltHookRunner {
    data_root: PathBuf,
}

impl MacOsSeatbeltHookRunner {
    pub fn new(data_root: PathBuf) -> Result<Self, String> {
        if !data_root.is_absolute() {
            return Err("Hook data root must be absolute".to_string());
        }
        Ok(Self { data_root })
    }

    fn plugin_data_path(&self, bundle: &LocalPluginHookBundleSpec) -> PathBuf {
        let mut digest = Sha256::new();
        for value in [
            bundle.owner_user_id.as_str(),
            bundle.plugin_id.as_str(),
            bundle.release_id.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        self.data_root.join(format!("{:x}", digest.finalize()))
    }
}

#[async_trait]
impl LocalHookRunner for MacOsSeatbeltHookRunner {
    async fn run(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String> {
        invocation.bundle.validate()?;
        if !invocation.bundle.hooks.contains(&invocation.hook) {
            return Err("Hook invocation is not part of its trusted bundle".to_string());
        }
        if invocation.stdin_json.len() > 256 * 1_024 {
            return Err("Hook stdin exceeds 262144 bytes".to_string());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = invocation;
            return Err(
                "macOS Seatbelt Hook execution is unavailable on this platform".to_string(),
            );
        }
        #[cfg(target_os = "macos")]
        self.run_macos(invocation).await
    }
}

#[cfg(target_os = "macos")]
impl MacOsSeatbeltHookRunner {
    async fn run_macos(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String> {
        use std::os::unix::{fs::PermissionsExt, process::CommandExt};

        let installation_root = canonical_directory(
            Path::new(&invocation.bundle.installation_root),
            "Hook installation root",
        )?;
        let workspace_root = invocation
            .workspace_root
            .as_deref()
            .map(|path| canonical_directory(path, "Hook workspace root"))
            .transpose()?;
        let configured_data_root = self.plugin_data_path(&invocation.bundle);
        std::fs::create_dir_all(&configured_data_root)
            .map_err(|error| format!("create Hook data directory failed: {error}"))?;
        std::fs::set_permissions(
            &configured_data_root,
            std::fs::Permissions::from_mode(0o700),
        )
        .map_err(|error| format!("protect Hook data directory failed: {error}"))?;
        let data_root = canonical_directory(&configured_data_root, "Hook data root")?;
        let temporary_root = data_root.join("tmp");
        std::fs::create_dir_all(&temporary_root)
            .map_err(|error| format!("create Hook temporary directory failed: {error}"))?;

        let profile = seatbelt_profile(
            &installation_root,
            &data_root,
            workspace_root.as_deref(),
            &invocation.bundle.permissions,
        );
        let environment = hook_environment(
            &installation_root,
            &data_root,
            &temporary_root,
            workspace_root.as_deref(),
        );
        let mut command = Command::new(SANDBOX_EXECUTABLE);
        command
            .arg("-p")
            .arg(profile)
            .arg(SHELL_EXECUTABLE)
            .arg("-c")
            .arg(&invocation.hook.command)
            .current_dir(workspace_root.as_deref().unwrap_or(&installation_root))
            .env_clear()
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| format!("start sandboxed Hook failed: {error}"))?;
        let process_id = child
            .id()
            .ok_or_else(|| "sandboxed Hook has no process id".to_string())?;
        let mut group_guard = ProcessGroupGuard::new(process_id);
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "sandboxed Hook has no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "sandboxed Hook has no stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "sandboxed Hook has no stderr".to_string())?;
        let stdin_json = invocation.stdin_json;
        let stdin_task = tokio::spawn(async move {
            let result = stdin.write_all(&stdin_json).await;
            drop(stdin);
            result
        });
        let stdout_task = tokio::spawn(read_bounded(stdout, MAXIMUM_STDOUT_BYTES));
        let stderr_task = tokio::spawn(read_bounded(stderr, MAXIMUM_STDERR_BYTES));
        let started = Instant::now();
        let deadline = Duration::from_millis(invocation.hook.timeout_ms);
        let (status, timed_out) = match timeout(deadline, child.wait()).await {
            Ok(result) => (
                result.map_err(|error| format!("wait for sandboxed Hook failed: {error}"))?,
                false,
            ),
            Err(_) => {
                terminate_process_group(process_id);
                let status = match timeout(TERMINATION_GRACE, child.wait()).await {
                    Ok(result) => {
                        result.map_err(|error| format!("reap timed out Hook failed: {error}"))?
                    }
                    Err(_) => {
                        kill_process_group(process_id);
                        child
                            .wait()
                            .await
                            .map_err(|error| format!("reap killed Hook failed: {error}"))?
                    }
                };
                (status, true)
            }
        };
        group_guard.disarm();
        let _ = stdin_task.await;
        let (stdout, stdout_overflow) = stdout_task
            .await
            .map_err(|error| format!("join Hook stdout reader failed: {error}"))??;
        let (stderr, stderr_overflow) = stderr_task
            .await
            .map_err(|error| format!("join Hook stderr reader failed: {error}"))??;
        Ok(LocalHookInvocationResult {
            exit_code: status.code(),
            stdout,
            stderr,
            stdout_overflow,
            stderr_overflow,
            timed_out,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }
}

async fn read_bounded<R>(mut reader: R, limit: usize) -> Result<(Vec<u8>, bool), String>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::with_capacity(limit.min(16 * 1_024));
    let mut overflow = false;
    let mut chunk = [0_u8; 8 * 1_024];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|error| format!("read Hook output failed: {error}"))?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&chunk[..count.min(remaining)]);
        overflow |= count > remaining;
    }
    Ok((output, overflow))
}

fn hook_environment(
    installation_root: &Path,
    data_root: &Path,
    temporary_root: &Path,
    workspace_root: Option<&Path>,
) -> BTreeMap<String, String> {
    let project = workspace_root.unwrap_or(installation_root);
    BTreeMap::from([
        (
            "PATH".to_string(),
            "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin".to_string(),
        ),
        ("HOME".to_string(), data_root.display().to_string()),
        ("TMPDIR".to_string(), temporary_root.display().to_string()),
        (
            "PLUGIN_ROOT".to_string(),
            installation_root.display().to_string(),
        ),
        ("PLUGIN_DATA".to_string(), data_root.display().to_string()),
        (
            "CLAUDE_PLUGIN_ROOT".to_string(),
            installation_root.display().to_string(),
        ),
        (
            "CLAUDE_PLUGIN_DATA".to_string(),
            data_root.display().to_string(),
        ),
        (
            "CLAUDE_PROJECT_DIR".to_string(),
            project.display().to_string(),
        ),
        (
            "CHATOS_PLUGIN_SANDBOX".to_string(),
            "macos-seatbelt-hook-v1".to_string(),
        ),
    ])
}

#[cfg(target_os = "macos")]
fn canonical_directory(path: &Path, name: &str) -> Result<PathBuf, String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|error| format!("{name} is unavailable: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{name} is not a directory"));
    }
    Ok(canonical)
}

#[cfg(target_os = "macos")]
fn seatbelt_profile(
    installation_root: &Path,
    data_root: &Path,
    workspace_root: Option<&Path>,
    permissions: &[String],
) -> String {
    let has = |permission: &str| permissions.iter().any(|value| value == permission);
    let mut rules = vec![
        "(version 1)".to_string(),
        "(deny default)".to_string(),
        "(import \"system.sb\")".to_string(),
        format!(
            "(allow process-exec (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") {})",
            seatbelt_literal(installation_root)
        ),
        "(allow process-fork)".to_string(),
        "(allow signal (target same-sandbox))".to_string(),
        "(allow sysctl-read)".to_string(),
        "(allow mach-lookup (global-name \"com.apple.cfprefsd.agent\") (global-name \"com.apple.system.logger\") (global-name \"com.apple.system.notification_center\") (global-name \"com.apple.securityd\") (global-name \"com.apple.system.opendirectoryd.libinfo\"))".to_string(),
        "(allow file-read-metadata)".to_string(),
        format!(
            "(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") (subpath \"/Library/Apple\") {})",
            seatbelt_literal(installation_root)
        ),
        format!(
            "(allow file-read* file-write* {})",
            seatbelt_literal(data_root)
        ),
    ];
    if let Some(workspace_root) = workspace_root {
        if has("workspace.write") {
            rules.push(format!(
                "(allow file-read* file-write* {})",
                seatbelt_literal(workspace_root)
            ));
        } else if has("workspace.read") {
            rules.push(format!(
                "(allow file-read* {})",
                seatbelt_literal(workspace_root)
            ));
        }
    }
    if has("network.client") {
        rules.push("(allow network-outbound)".to_string());
    }
    rules.join("\n")
}

#[cfg(target_os = "macos")]
fn seatbelt_literal(path: &Path) -> String {
    let escaped = path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("(subpath \"{escaped}\")")
}

#[cfg(target_os = "macos")]
struct ProcessGroupGuard {
    process_id: u32,
    armed: bool,
}

#[cfg(target_os = "macos")]
impl ProcessGroupGuard {
    fn new(process_id: u32) -> Self {
        Self {
            process_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(target_os = "macos")]
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.armed {
            kill_process_group(self.process_id);
        }
    }
}

#[cfg(target_os = "macos")]
fn terminate_process_group(process_id: u32) {
    unsafe {
        libc::kill(-(process_id as i32), libc::SIGTERM);
    }
}

#[cfg(target_os = "macos")]
fn kill_process_group(process_id: u32) {
    unsafe {
        libc::kill(-(process_id as i32), libc::SIGKILL);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture(command: &str, timeout_ms: u64) -> (TempDir, LocalHookInvocation) {
        let root = tempfile::tempdir().expect("root");
        let installation = root.path().join("plugin");
        std::fs::create_dir_all(&installation).expect("plugin root");
        let hook = LocalPluginHookDefinition {
            hook_id: "hook-1".to_string(),
            event: "SessionStart".to_string(),
            matcher: None,
            command: command.to_string(),
            timeout_ms,
            additional_context_limit: 65_536,
        };
        let bundle = LocalPluginHookBundleSpec {
            bundle_id: "bundle-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            component_key: "hooks-1".to_string(),
            artifact_sha256: "a".repeat(64),
            content_sha256: "b".repeat(64),
            normalized_hook_sha256: "c".repeat(64),
            command_sha256: "d".repeat(64),
            permission_snapshot_sha256: "e".repeat(64),
            runner_policy_version: 1,
            installation_root: installation.display().to_string(),
            permissions: vec!["process.spawn".to_string()],
            hooks: vec![hook.clone()],
            enabled: true,
            trusted_at_unix_ms: 1,
        };
        (
            root,
            LocalHookInvocation {
                bundle,
                hook,
                stdin_json: br#"{"session_id":"session-1"}"#.to_vec(),
                workspace_root: None,
            },
        )
    }

    #[tokio::test]
    async fn seatbelt_runner_returns_only_stdout_protocol_data() {
        let (root, invocation) = fixture(
            r#"printf '%s' '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"mode loaded"}}'; printf 'diagnostic' >&2"#,
            2_000,
        );
        let runner = MacOsSeatbeltHookRunner::new(root.path().join("data")).expect("runner");
        let result = runner.run(invocation).await.expect("run");
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert_eq!(result.stderr, b"diagnostic");
        let output: serde_json::Value = serde_json::from_slice(&result.stdout).expect("json");
        assert_eq!(
            output["hookSpecificOutput"]["additionalContext"],
            "mode loaded"
        );
    }

    #[tokio::test]
    async fn timeout_terminates_the_hook_process_group() {
        let (root, invocation) = fixture("sleep 30", 100);
        let runner = MacOsSeatbeltHookRunner::new(root.path().join("data")).expect("runner");
        let started = Instant::now();
        let result = runner.run(invocation).await.expect("run");
        assert!(result.timed_out);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn node_hook_can_write_plugin_data_but_not_workspace_or_network() {
        let (root, mut invocation) = fixture("node \"$PLUGIN_ROOT/security-probe.js\"", 5_000);
        let installation = PathBuf::from(&invocation.bundle.installation_root);
        let workspace = root.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::write(workspace.join("readable.txt"), "workspace-visible")
            .expect("workspace fixture");
        std::fs::write(
            installation.join("security-probe.js"),
            r#"
const fs = require('fs');
const net = require('net');
const path = require('path');
const outcome = {
  workspaceRead: fs.readFileSync(path.join(process.env.CLAUDE_PROJECT_DIR, 'readable.txt'), 'utf8'),
  pluginDataWrite: false,
  workspaceWriteBlocked: false,
  networkError: null,
};
fs.writeFileSync(path.join(process.env.PLUGIN_DATA, 'written.txt'), 'plugin-data-writable');
outcome.pluginDataWrite = true;
try {
  fs.writeFileSync(path.join(process.env.CLAUDE_PROJECT_DIR, 'forbidden.txt'), 'must-not-exist');
} catch (_) {
  outcome.workspaceWriteBlocked = true;
}
let finished = false;
function finish(networkError) {
  if (finished) return;
  finished = true;
  outcome.networkError = networkError;
  process.stdout.write(JSON.stringify(outcome));
}
const socket = net.connect({host: '127.0.0.1', port: 9});
socket.on('connect', () => { socket.destroy(); finish('connected'); });
socket.on('error', (error) => finish(error.code || String(error)));
setTimeout(() => { socket.destroy(); finish('timeout'); }, 1000);
"#,
        )
        .expect("security probe");
        invocation.bundle.permissions =
            vec!["process.spawn".to_string(), "workspace.read".to_string()];
        invocation.hook = LocalPluginHookDefinition {
            hook_id: "hook-1".to_string(),
            event: "SessionStart".to_string(),
            matcher: None,
            command: "node \"$PLUGIN_ROOT/security-probe.js\"".to_string(),
            timeout_ms: 5_000,
            additional_context_limit: 65_536,
        };
        invocation.bundle.hooks = vec![invocation.hook.clone()];
        invocation.workspace_root = Some(workspace.clone());

        let data_root = root.path().join("data");
        let runner = MacOsSeatbeltHookRunner::new(data_root.clone()).expect("runner");
        let result = runner.run(invocation).await.expect("run");

        assert_eq!(
            result.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let outcome: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("probe json");
        assert_eq!(outcome["workspaceRead"], "workspace-visible");
        assert_eq!(outcome["pluginDataWrite"], true);
        assert_eq!(outcome["workspaceWriteBlocked"], true);
        assert!(matches!(
            outcome["networkError"].as_str(),
            Some("EPERM" | "EACCES")
        ));
        assert!(!workspace.join("forbidden.txt").exists());
        let plugin_data = std::fs::read_dir(&data_root)
            .expect("data root")
            .next()
            .expect("plugin data directory")
            .expect("plugin data entry")
            .path();
        assert_eq!(
            std::fs::read_to_string(plugin_data.join("written.txt")).expect("plugin data"),
            "plugin-data-writable"
        );
    }
}
