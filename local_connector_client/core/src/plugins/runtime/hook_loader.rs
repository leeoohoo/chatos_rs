// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use chatos_mcp_runtime::McpStdioServer;
use chatos_plugin_management_sdk::{
    normalized_plugin_hook_set_sha256, parse_plugin_hook_set, plugin_component_descriptors,
    plugin_hook_snapshot_sha256, PluginComponentKind, PluginHook, PluginHookEntrypoint,
    PluginHookEvent, PluginHookEventContext, PluginHookFailurePolicy, PluginHookSet,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::mcp_adapter::load_verified_manifest;
use super::stdio_sandbox::PluginStdioSandboxLauncher;
use crate::plugins::{ActivePluginInstallation, PluginInstaller};

const MAX_HOOK_SET_BYTES: u64 = 512 * 1024;
const MAX_HOOK_COMMAND_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HOOK_INPUT_BYTES: usize = 64 * 1024;
const DISPATCH_HOOK_EVENT_OPERATION: &str = "dispatch_hook_event";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHookSetSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub relative_source_path: String,
    pub content_sha256: String,
    pub hook_set_sha256: String,
    pub command_sha256_by_hook: BTreeMap<String, String>,
    pub hook_set: PluginHookSet,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHookExecutionRecord {
    pub hook_id: String,
    pub event: PluginHookEvent,
    pub failure_policy: PluginHookFailurePolicy,
    pub matched: bool,
    pub succeeded: bool,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub output_truncated: bool,
    pub workspace_write: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_write_approved: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHookDispatchResult {
    pub event: PluginHookEvent,
    pub snapshot_sha256: String,
    pub blocking_failure: bool,
    pub executions: Vec<PluginHookExecutionRecord>,
}

#[derive(Debug, Clone)]
pub struct PluginHookLoader {
    installer: PluginInstaller,
}

#[derive(Debug, Clone)]
pub(super) enum PluginHookWorkspaceWriteDecision {
    Approved(PathBuf),
    Denied(String),
}

impl PluginHookLoader {
    pub fn new(installer: PluginInstaller) -> Self {
        Self { installer }
    }

    pub fn load(
        &self,
        plugin_id: &str,
        component_key: &str,
        expected_content_sha256: &str,
        permission_snapshot: &BTreeSet<String>,
    ) -> Result<PluginHookSetSnapshot> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        let manifest = load_verified_manifest(&installation)?;
        let hook = manifest
            .hooks
            .iter()
            .find(|hook| hook.component_key == component_key)
            .context("Plugin Hook set is not present in the active Manifest")?;
        validate_hook_inventory(&installation, &manifest, hook)?;
        validate_required_permissions(&installation, component_key, permission_snapshot)?;
        let (raw, content_sha256) = read_verified_package_text(
            &installation,
            hook.source.path.as_str(),
            expected_content_sha256,
            MAX_HOOK_SET_BYTES,
            "Plugin Hook set source",
        )?;
        let hook_set = parse_plugin_hook_set(raw.as_str()).context("parse Plugin Hook set")?;
        if hook_set.hooks.iter().any(|hook| hook.workspace_write) {
            if !permission_snapshot.contains("workspace.write") {
                bail!(
                    "Plugin Hook workspace.write permission is missing from the prepared snapshot"
                );
            }
            if !cfg!(target_os = "macos") {
                bail!("writable workspace Plugin Hooks are not yet supported by this platform sandbox");
            }
        }
        let hook_set_sha256 =
            normalized_plugin_hook_set_sha256(&hook_set).context("hash Plugin Hook set")?;
        let mut command_sha256_by_hook = BTreeMap::new();
        for definition in &hook_set.hooks {
            let command = definition.entrypoint.command().path.as_str();
            let (_, command_sha256) = read_verified_package_bytes(
                &installation,
                command,
                installation
                    .version
                    .package_file_sha256
                    .get(command.trim_start_matches("./"))
                    .context("Plugin Hook command is not covered by package checksums")?,
                MAX_HOOK_COMMAND_BYTES,
                "Plugin Hook command",
            )?;
            validate_executable(installation.installation_path.join(command).as_path())?;
            command_sha256_by_hook.insert(definition.id.clone(), command_sha256);
        }
        let snapshot_sha256 = plugin_hook_snapshot_sha256(
            plugin_id,
            installation.version.release_id.as_str(),
            component_key,
            hook.source.path.as_str(),
            content_sha256.as_str(),
            hook_set_sha256.as_str(),
            &command_sha256_by_hook,
        )
        .context("hash Plugin Hook snapshot")?;
        Ok(PluginHookSetSnapshot {
            plugin_id: plugin_id.to_string(),
            release_id: installation.version.release_id,
            version: installation.version.version,
            artifact_sha256: installation.version.artifact_sha256,
            component_key: component_key.to_string(),
            relative_source_path: hook.source.path.clone(),
            content_sha256,
            hook_set_sha256,
            command_sha256_by_hook,
            hook_set,
            snapshot_sha256,
        })
    }

    pub const fn operation(&self) -> &'static str {
        DISPATCH_HOOK_EVENT_OPERATION
    }

    pub(super) fn matching_workspace_write_hook_ids(
        &self,
        snapshot: &PluginHookSetSnapshot,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
    ) -> Result<Vec<String>> {
        validate_event_context(context)?;
        Ok(snapshot
            .hook_set
            .hooks
            .iter()
            .filter(|hook| {
                hook.workspace_write
                    && hook.events.contains(&event)
                    && hook.matcher.matches(context)
            })
            .map(|hook| hook.id.clone())
            .collect())
    }

    pub(super) async fn dispatch(
        &self,
        snapshot: &PluginHookSetSnapshot,
        permission_snapshot: &BTreeSet<String>,
        run_id: &str,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
        workspace_write_decisions: &BTreeMap<String, PluginHookWorkspaceWriteDecision>,
    ) -> Result<PluginHookDispatchResult> {
        validate_event_context(context)?;
        let active = self.load(
            snapshot.plugin_id.as_str(),
            snapshot.component_key.as_str(),
            snapshot.content_sha256.as_str(),
            permission_snapshot,
        )?;
        if &active != snapshot {
            bail!("Plugin Hook snapshot changed after prepare");
        }
        let installation = self
            .installer
            .active_installation(snapshot.plugin_id.as_str())?
            .context("Plugin is not installed and active")?;
        let mut executions = Vec::new();
        let mut blocking_failure = false;
        for hook in &snapshot.hook_set.hooks {
            if !hook.events.contains(&event) {
                continue;
            }
            if !hook.matcher.matches(context) {
                executions.push(unmatched_execution(
                    hook.id.as_str(),
                    event,
                    hook.failure_policy,
                    hook.workspace_write,
                ));
                continue;
            }
            let record = if hook.workspace_write {
                match workspace_write_decisions.get(hook.id.as_str()) {
                    Some(PluginHookWorkspaceWriteDecision::Approved(workspace_root)) => {
                        self.execute_command(
                            &installation,
                            snapshot,
                            hook,
                            run_id,
                            event,
                            context,
                            Some(workspace_root.as_path()),
                        )
                        .await
                    }
                    Some(PluginHookWorkspaceWriteDecision::Denied(reason)) => {
                        workspace_write_denied_execution(
                            hook.id.as_str(),
                            event,
                            hook.failure_policy,
                            reason.as_str(),
                        )
                    }
                    None => workspace_write_denied_execution(
                        hook.id.as_str(),
                        event,
                        hook.failure_policy,
                        "Plugin Hook workspace-write approval was not supplied",
                    ),
                }
            } else {
                self.execute_command(&installation, snapshot, hook, run_id, event, context, None)
                    .await
            };
            if !record.succeeded && hook.failure_policy == PluginHookFailurePolicy::FailRun {
                blocking_failure = true;
            }
            executions.push(record);
        }
        Ok(PluginHookDispatchResult {
            event,
            snapshot_sha256: snapshot.snapshot_sha256.clone(),
            blocking_failure,
            executions,
        })
    }

    async fn execute_command(
        &self,
        installation: &ActivePluginInstallation,
        snapshot: &PluginHookSetSnapshot,
        hook: &chatos_plugin_management_sdk::PluginHookDefinition,
        run_id: &str,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
        workspace_root: Option<&Path>,
    ) -> PluginHookExecutionRecord {
        let started = Instant::now();
        match self
            .run_command(
                installation,
                snapshot,
                hook,
                run_id,
                event,
                context,
                workspace_root,
            )
            .await
        {
            Ok(output) => PluginHookExecutionRecord {
                hook_id: hook.id.clone(),
                event,
                failure_policy: hook.failure_policy,
                matched: true,
                succeeded: output.exit_code == Some(0) && !output.timed_out,
                timed_out: output.timed_out,
                exit_code: output.exit_code,
                duration_ms: elapsed_millis(started),
                stdout_bytes: output.stdout.total_bytes,
                stderr_bytes: output.stderr.total_bytes,
                stdout_sha256: output.stdout.sha256,
                stderr_sha256: output.stderr.sha256,
                output_truncated: output.stdout.truncated || output.stderr.truncated,
                workspace_write: hook.workspace_write,
                workspace_write_approved: hook.workspace_write.then_some(true),
                error: output.error,
            },
            Err(error) => PluginHookExecutionRecord {
                hook_id: hook.id.clone(),
                event,
                failure_policy: hook.failure_policy,
                matched: true,
                succeeded: false,
                timed_out: false,
                exit_code: None,
                duration_ms: elapsed_millis(started),
                stdout_bytes: 0,
                stderr_bytes: 0,
                stdout_sha256: sha256_bytes(&[]),
                stderr_sha256: sha256_bytes(&[]),
                output_truncated: false,
                workspace_write: hook.workspace_write,
                workspace_write_approved: hook.workspace_write.then_some(true),
                error: Some(sanitize_error(error.to_string().as_str())),
            },
        }
    }

    async fn run_command(
        &self,
        installation: &ActivePluginInstallation,
        snapshot: &PluginHookSetSnapshot,
        hook: &chatos_plugin_management_sdk::PluginHookDefinition,
        run_id: &str,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
        workspace_root: Option<&Path>,
    ) -> Result<HookCommandOutput> {
        let (command, args) = match &hook.entrypoint {
            PluginHookEntrypoint::Command { command, args } => (command, args),
        };
        let command_path = installation.installation_path.join(command.path.as_str());
        let server = McpStdioServer::new(
            format!("plugin-hook-{}", hook.id),
            command_path.to_string_lossy(),
        )
        .with_args(args.clone())
        .with_cwd(installation.installation_path.to_string_lossy());
        let launcher = PluginStdioSandboxLauncher::discover()?;
        let (wrapped, _sandbox_runtime) = if hook.workspace_write {
            let workspace_root =
                workspace_root.context("Plugin Hook workspace-write approval is unavailable")?;
            launcher.prepare_with_workspace_write(
                self.installer.plugin_root(),
                installation.installation_path.as_path(),
                &server,
                Vec::<String>::new(),
                workspace_root,
            )?
        } else {
            launcher.prepare(
                self.installer.plugin_root(),
                installation.installation_path.as_path(),
                &server,
                Vec::<String>::new(),
            )?
        };
        let input = serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "event": event,
            "runId": run_id,
            "pluginId": snapshot.plugin_id,
            "releaseId": snapshot.release_id,
            "componentKey": snapshot.component_key,
            "hookId": hook.id,
            "hookSnapshotSha256": snapshot.snapshot_sha256,
            "context": context,
        }))?;
        if input.len() > MAX_HOOK_INPUT_BYTES {
            bail!("Plugin Hook input exceeds its size limit");
        }
        let mut command = tokio::process::Command::new(wrapped.command.as_str());
        command
            .args(wrapped.args.unwrap_or_default())
            .current_dir(
                wrapped
                    .cwd
                    .as_deref()
                    .context("Plugin Hook sandbox cwd is unavailable")?,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .context("start sandboxed Plugin Hook command")?;
        let mut stdin = child
            .stdin
            .take()
            .context("Plugin Hook stdin is unavailable")?;
        stdin.write_all(input.as_slice()).await?;
        stdin.write_all(b"\n").await?;
        stdin.shutdown().await?;
        let stdout = child
            .stdout
            .take()
            .context("Plugin Hook stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("Plugin Hook stderr is unavailable")?;
        let stdout_reader = tokio::spawn(read_bounded(stdout, hook.max_output_bytes));
        let stderr_reader = tokio::spawn(read_bounded(stderr, hook.max_output_bytes));
        let wait = tokio::time::timeout(Duration::from_millis(hook.timeout_ms), child.wait()).await;
        let (exit_code, timed_out, error) = match wait {
            Ok(Ok(status)) => (status.code(), false, None),
            Ok(Err(error)) => (
                None,
                false,
                Some(sanitize_error(error.to_string().as_str())),
            ),
            Err(_) => {
                terminate_process_tree(&mut child).await;
                (
                    None,
                    true,
                    Some(format!(
                        "Plugin Hook timed out after {} ms",
                        hook.timeout_ms
                    )),
                )
            }
        };
        let stdout = stdout_reader
            .await
            .map_err(|_| anyhow!("Plugin Hook stdout reader failed"))??;
        let stderr = stderr_reader
            .await
            .map_err(|_| anyhow!("Plugin Hook stderr reader failed"))??;
        Ok(HookCommandOutput {
            exit_code,
            timed_out,
            stdout,
            stderr,
            error,
        })
    }
}

#[derive(Debug)]
struct HookCommandOutput {
    exit_code: Option<i32>,
    timed_out: bool,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
    error: Option<String>,
}

#[derive(Debug)]
struct BoundedOutput {
    total_bytes: usize,
    sha256: String,
    truncated: bool,
}

async fn read_bounded(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<BoundedOutput> {
    let mut total_bytes = 0usize;
    let mut retained = 0usize;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read);
        hasher.update(&buffer[..read]);
        let keep = read.min(limit.saturating_sub(retained));
        if keep > 0 {
            retained += keep;
        }
    }
    Ok(BoundedOutput {
        total_bytes,
        sha256: hex::encode(hasher.finalize()),
        truncated: total_bytes > limit,
    })
}

fn read_verified_package_text(
    installation: &ActivePluginInstallation,
    relative_path: &str,
    expected_content_sha256: &str,
    max_bytes: u64,
    label: &str,
) -> Result<(String, String)> {
    let (bytes, actual_sha256) = read_verified_package_bytes(
        installation,
        relative_path,
        expected_content_sha256,
        max_bytes,
        label,
    )?;
    let raw = String::from_utf8(bytes).with_context(|| format!("{label} is not UTF-8"))?;
    if raw.contains('\0') {
        bail!("{label} contains NUL bytes");
    }
    Ok((raw, actual_sha256))
}

fn read_verified_package_bytes(
    installation: &ActivePluginInstallation,
    relative_path: &str,
    expected_content_sha256: &str,
    max_bytes: u64,
    label: &str,
) -> Result<(Vec<u8>, String)> {
    let package_path = relative_path.trim_start_matches("./");
    let expected_package_sha256 = installation
        .version
        .package_file_sha256
        .get(package_path)
        .with_context(|| format!("{label} is not covered by package checksums"))?;
    let path = installation.installation_path.join(relative_path);
    let metadata =
        fs::symlink_metadata(path.as_path()).with_context(|| format!("read {label} metadata"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        bail!("{label} is missing, unsafe, or exceeds its size limit");
    }
    let bytes = fs::read(path.as_path()).with_context(|| format!("read {label}"))?;
    if bytes.len() as u64 > max_bytes {
        bail!("{label} exceeds its size limit");
    }
    let actual_sha256 = sha256_bytes(bytes.as_slice());
    if actual_sha256 != *expected_package_sha256 || actual_sha256 != expected_content_sha256 {
        bail!("{label} does not match the immutable component snapshot");
    }
    Ok((bytes, actual_sha256))
}

fn validate_hook_inventory(
    installation: &ActivePluginInstallation,
    manifest: &chatos_plugin_management_sdk::PluginManifest,
    hook: &PluginHook,
) -> Result<()> {
    let descriptor = plugin_component_descriptors(manifest)
        .into_iter()
        .find(|component| component.component_key == hook.component_key)
        .context("Plugin Hook component descriptor is unavailable")?;
    if descriptor.kind != PluginComponentKind::HookSet
        || descriptor.runtime_kind != "hook_set"
        || descriptor.entrypoint.as_ref() != Some(&hook.source)
    {
        bail!("Plugin Hook descriptor does not match its signed Manifest");
    }
    let installed = installation
        .version
        .inventory
        .components
        .iter()
        .find(|component| component.component_key == hook.component_key)
        .context("Plugin Hook is missing from the signed installation inventory")?;
    if installed != &descriptor {
        bail!("Plugin Hook inventory does not match the active signed Manifest");
    }
    Ok(())
}

fn validate_required_permissions(
    installation: &ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    for requirement in installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.required
                && (requirement.components.is_empty()
                    || requirement
                        .components
                        .iter()
                        .any(|key| key == component_key))
        })
    {
        if !permission_snapshot.contains(requirement.permission.as_str()) {
            bail!(
                "Plugin Hook required permission is missing from the prepared snapshot: {}",
                requirement.permission
            );
        }
    }
    Ok(())
}

fn validate_executable(path: &std::path::Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("read Plugin Hook command metadata")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("Plugin Hook command is not a regular non-symlink file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("Plugin Hook command is not executable");
        }
    }
    Ok(())
}

fn validate_event_context(context: &PluginHookEventContext) -> Result<()> {
    for (field, value) in [
        ("agentKey", context.agent_key.as_deref()),
        ("toolName", context.tool_name.as_deref()),
        ("toolKind", context.tool_kind.as_deref()),
        ("componentKey", context.component_key.as_deref()),
    ] {
        if value.is_some_and(|value| {
            value.is_empty()
                || value.len() > 256
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        }) {
            bail!("Plugin Hook event context {field} is invalid");
        }
    }
    if context.summary_sha256.as_deref().is_some_and(|value| {
        value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    }) {
        bail!("Plugin Hook event context summarySha256 is invalid");
    }
    Ok(())
}

fn unmatched_execution(
    hook_id: &str,
    event: PluginHookEvent,
    failure_policy: PluginHookFailurePolicy,
    workspace_write: bool,
) -> PluginHookExecutionRecord {
    PluginHookExecutionRecord {
        hook_id: hook_id.to_string(),
        event,
        failure_policy,
        matched: false,
        succeeded: true,
        timed_out: false,
        exit_code: None,
        duration_ms: 0,
        stdout_bytes: 0,
        stderr_bytes: 0,
        stdout_sha256: sha256_bytes(&[]),
        stderr_sha256: sha256_bytes(&[]),
        output_truncated: false,
        workspace_write,
        workspace_write_approved: None,
        error: None,
    }
}

fn workspace_write_denied_execution(
    hook_id: &str,
    event: PluginHookEvent,
    failure_policy: PluginHookFailurePolicy,
    reason: &str,
) -> PluginHookExecutionRecord {
    PluginHookExecutionRecord {
        hook_id: hook_id.to_string(),
        event,
        failure_policy,
        matched: true,
        succeeded: false,
        timed_out: false,
        exit_code: None,
        duration_ms: 0,
        stdout_bytes: 0,
        stderr_bytes: 0,
        stdout_sha256: sha256_bytes(&[]),
        stderr_sha256: sha256_bytes(&[]),
        output_truncated: false,
        workspace_write: true,
        workspace_write_approved: Some(false),
        error: Some(sanitize_error(reason)),
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut tokio::process::Command) {
    use std::os::unix::process::CommandExt;

    command.as_std_mut().process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut tokio::process::Command) {}

#[cfg(unix)]
async fn terminate_process_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
}

#[cfg(not(unix))]
async fn terminate_process_tree(child: &mut tokio::process::Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn sanitize_error(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(1024)
        .collect()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_matcher_never_evaluates_expressions() {
        let set = parse_plugin_hook_set(
            r#"{"hooks":[{"id":"audit","events":["PostToolUse"],"matcher":{"toolNames":["browser_snapshot"],"outcomes":["succeeded"]},"entrypoint":{"type":"command","command":"./scripts/audit"}}]}"#,
        )
        .expect("Hook set");
        let matcher = &set.hooks[0].matcher;
        assert!(matcher.matches(&PluginHookEventContext {
            tool_name: Some("browser_snapshot".to_string()),
            outcome: Some(chatos_plugin_management_sdk::PluginHookOutcome::Succeeded),
            ..PluginHookEventContext::default()
        }));
        assert!(!matcher.matches(&PluginHookEventContext {
            tool_name: Some("terminal_exec".to_string()),
            outcome: Some(chatos_plugin_management_sdk::PluginHookOutcome::Succeeded),
            ..PluginHookEventContext::default()
        }));
    }
}
