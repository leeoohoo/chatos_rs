// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_service::TOOL_RESULT_MAX_CHARS_META_KEY;
use chatos_sandbox_contract::{SandboxBackendKind, SandboxBackendReadinessStatus};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::approval::{
    approval_project_key_from_request, ApprovalDecision, CommandApprovalRequest,
    CommandApprovalService,
};
use crate::history::{
    command_history_entry_from_exec_result, CommandExecutionContext, CommandHistoryRecorder,
};
use crate::local_runtime::LocalDatabase;
use crate::mcp::tools::{
    code_maintainer_structured_result, normalize_request_project_relative_path,
    request_project_root,
};
use crate::relay::RelayRequest;
use crate::sandbox::process::{
    call_native_sandbox_mcp, destroy_native_sandbox_process, native_process_sandbox_capability,
    start_native_sandbox_process,
};
use crate::sandbox::types::{LocalSandboxResourceLimits, LocalSandboxRuntime};
use crate::workspace::paths::relative_to_workspace;
use crate::{
    local_now_rfc3339, LocalState, WorkspaceState, DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    MAX_TERMINAL_EXEC_TIMEOUT_MS,
};

const SESSION_ID_HEADER: &str = "x-mcp-management-session-id";
const SESSION_EXPIRES_AT_UNIX_HEADER: &str = "x-mcp-management-session-expires-at-unix";
const RUN_ID_HEADER: &str = "x-mcp-management-run-id";
const EXECUTION_GROUP_ID_HEADER: &str = "x-mcp-management-execution-group-id";
const SCOPE_GENERATION_HEADER: &str = "x-mcp-management-execution-scope-generation";
const PROJECT_ID_HEADER: &str = "x-local-connector-project-id";
const EXECUTION_SCOPE_REAPER_INTERVAL: Duration = Duration::from_secs(15);
const EXECUTION_SCOPE_ORPHAN_GRACE_SECONDS: i64 = 60;
const EXECUTION_SCOPE_TOMBSTONE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const FINALIZATION_PATCH_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
struct LocalGitExecutionWorkspace {
    execution_group_id: String,
    execution_branch_ref: String,
    snapshot_commit: String,
    integration_worktree: PathBuf,
    run_worktree: PathBuf,
    run_project_root: PathBuf,
}

#[derive(Debug)]
pub(crate) struct LocalExecutionScope {
    pub(crate) scope_id: String,
    generation: i64,
    git_workspace: Option<LocalGitExecutionWorkspace>,
    active_invocations: AtomicUsize,
    draining: std::sync::atomic::AtomicBool,
    releasing: std::sync::atomic::AtomicBool,
    expires_at_unix: std::sync::atomic::AtomicI64,
    lifecycle_lock: tokio::sync::Mutex<()>,
}

impl LocalExecutionScope {
    async fn begin_invocation(
        self: &Arc<Self>,
        runtime: &LocalSandboxRuntime,
    ) -> Result<LocalExecutionInvocationGuard> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.ensure_accepting_invocations()?;
        self.active_invocations.fetch_add(1, Ordering::AcqRel);
        Ok(LocalExecutionInvocationGuard {
            scope: self.clone(),
            runtime: runtime.clone(),
        })
    }

    fn ensure_accepting_invocations(&self) -> Result<()> {
        if self.draining.load(Ordering::Acquire) {
            return Err(anyhow!(
                "local execution scope is draining after run finalization"
            ));
        }
        Ok(())
    }

    fn renew_until(&self, expires_at_unix: i64) {
        self.expires_at_unix
            .fetch_max(expires_at_unix, Ordering::AcqRel);
    }

    fn ready_for_release(&self, now: i64) -> bool {
        if self.active_invocations.load(Ordering::Acquire) != 0 {
            return false;
        }
        self.draining.load(Ordering::Acquire)
            || self
                .expires_at_unix
                .load(Ordering::Acquire)
                .saturating_add(EXECUTION_SCOPE_ORPHAN_GRACE_SECONDS)
                <= now
    }
}

struct LocalExecutionInvocationGuard {
    scope: Arc<LocalExecutionScope>,
    runtime: LocalSandboxRuntime,
}

impl Drop for LocalExecutionInvocationGuard {
    fn drop(&mut self) {
        let previous = self.scope.active_invocations.fetch_sub(1, Ordering::AcqRel);
        if previous == 1 && self.scope.draining.load(Ordering::Acquire) {
            let runtime = self.runtime.clone();
            let scope = self.scope.clone();
            tokio::spawn(async move {
                release_drained_scope(&runtime, &scope).await;
            });
        }
    }
}

pub(crate) async fn call_local_execution_scope_tool(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
    project_root: &Path,
    tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
) -> Result<Value> {
    let scope =
        get_or_create_scope(request, state, http_client, runtime, database, project_root).await?;
    let _invocation = scope.begin_invocation(runtime).await?;
    let request_body = tool_call_body(request, tool_name, arguments, tool_result_max_chars);
    let response = call_scope_mcp(http_client, runtime, &scope, &request_body).await?;
    decode_tool_result(response, request_body.get("id"))
}

pub(crate) async fn finalize_local_execution_scope(
    request: &RelayRequest,
    state: &LocalState,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
) -> Result<Value> {
    let _creation = runtime.execution_scope_creation_lock.lock().await;
    let run_id = relay_header(request, RUN_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope finalize is missing run identity"))?;
    let owner_user_id = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local execution scope finalize is missing owner identity"))?;
    let project_id = relay_header(request, PROJECT_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope finalize is missing project identity"))?;
    let generation = required_scope_generation(request)?;
    let terminal_status = request
        .body
        .pointer("/params/status")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    if let Some(result) = database
        .execution_scope_finalization_result(owner_user_id, project_id, run_id, generation)
        .await?
    {
        if result.get("status").and_then(Value::as_str) != Some("conflict") {
            return Ok(json!({
                "jsonrpc": "2.0",
                "id": request.body.get("id").cloned().unwrap_or(Value::Null),
                "result": result,
            }));
        }
    }
    let matching = runtime
        .execution_scopes
        .read()
        .await
        .iter()
        .filter(|(key, scope)| {
            scope_key_matches_run(key, owner_user_id, project_id, run_id)
                && scope.generation == generation
        })
        .map(|(key, scope)| (key.clone(), scope.clone()))
        .collect::<Vec<_>>();
    for (_, scope) in &matching {
        {
            let _lifecycle = scope.lifecycle_lock.lock().await;
            scope.draining.store(true, Ordering::Release);
            scope.expires_at_unix.store(0, Ordering::Release);
            if scope.active_invocations.load(Ordering::Acquire) != 0 {
                scope.draining.store(false, Ordering::Release);
                return Err(anyhow!(
                    "local execution scope still has active tool invocations"
                ));
            }
        }
    }
    let git_workspace = matching
        .iter()
        .find_map(|(_, scope)| scope.git_workspace.clone())
        .or_else(|| None);
    let git_workspace = match git_workspace {
        Some(workspace) => Some(workspace),
        None => {
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == request.workspace_id)
                .ok_or_else(|| anyhow!("local execution scope workspace is unavailable"))?;
            let project_root = request_project_root(workspace, request)?;
            prepare_local_git_execution_workspace(request, project_root.as_path()).await?
        }
    };
    let result = if let Some(workspace) = git_workspace.as_ref() {
        finalize_local_git_execution_workspace(workspace, run_id, terminal_status == "succeeded")
            .await?
    } else {
        json!({
            "ok": true,
            "status": "no_changes",
            "execution_group_id": relay_header(request, EXECUTION_GROUP_ID_HEADER),
            "execution_branch_ref": Value::Null,
            "base_commit": Value::Null,
            "result_commit": Value::Null,
            "integrated_commit": Value::Null,
            "conflict_files": [],
            "files": [],
            "patch": "",
            "patch_truncated": false,
        })
    };
    database
        .persist_execution_scope_finalization_result(
            owner_user_id,
            project_id,
            run_id,
            generation,
            terminal_status,
            &result,
            Utc::now()
                .timestamp()
                .saturating_add(EXECUTION_SCOPE_TOMBSTONE_TTL_SECONDS),
        )
        .await?;
    let mut released = 0usize;
    for (key, scope) in matching {
        if try_release_scope(runtime, key.as_str(), &scope).await? {
            released = released.saturating_add(1);
        }
    }
    let mut result = result;
    if let Some(map) = result.as_object_mut() {
        map.insert("released_scopes".to_string(), json!(released));
    }
    Ok(json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or(Value::Null),
        "result": result,
    }))
}

fn scope_key_matches_run(key: &str, owner_user_id: &str, project_id: &str, run_id: &str) -> bool {
    let mut parts = key.split('\u{1f}');
    parts.next() == Some(owner_user_id)
        && parts.next() == Some(project_id)
        && parts.next() == Some(run_id)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn call_local_execution_scope_terminal_tool(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
    workspace: &WorkspaceState,
    tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let project_root = request_project_root(workspace, request)?;
    let mut arguments = arguments;
    let timeout_ms = arguments
        .get("timeout_ms")
        .or_else(|| arguments.get("max_wait_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);
    let normalized_path = if tool_name == "execute_command" {
        let path = arguments.get("path").and_then(Value::as_str).unwrap_or(".");
        let normalized = normalize_request_project_relative_path(workspace, request, path)?;
        if let Some(map) = arguments.as_object_mut() {
            map.insert("path".to_string(), Value::String(normalized.clone()));
        }
        Some(normalized)
    } else {
        None
    };
    if tool_name == "execute_command" {
        if let Some(command) = terminal_command_text(&arguments) {
            let cwd_label = normalized_path.clone().unwrap_or_else(|| ".".to_string());
            let project_key = approval_project_key_from_request(
                state,
                request,
                workspace,
                relative_to_workspace(workspace, project_root.as_path()),
            );
            let approval = CommandApprovalService::new(
                history_recorder.state_path.clone(),
                history_recorder.state.clone(),
            )
            .approve(CommandApprovalRequest {
                request_id: request.request_id.clone(),
                project_key,
                command: command.clone(),
                args: Vec::new(),
                redact_arguments_in_history: false,
                cwd: cwd_label.clone(),
                source: "local_mcp".to_string(),
                requested_permissions: None,
                session_id: relay_header(request, RUN_ID_HEADER).map(ToOwned::to_owned),
                action_audit: None,
            })
            .await?;
            if let ApprovalDecision::Denied { reason, .. } = approval {
                let body = terminal_approval_denied_body(
                    command.as_str(),
                    cwd_label.as_str(),
                    timeout_ms,
                    reason.as_str(),
                );
                history_recorder
                    .append(command_history_entry_from_exec_result(
                        state,
                        request,
                        &CommandExecutionContext::local_mcp(request, "execute_command"),
                        command.as_str(),
                        &[],
                        cwd_label.as_str(),
                        local_now_rfc3339(),
                        &body,
                    ))
                    .await;
                return Ok(mcp_text_result(body));
            }
        }
    }
    let result = call_local_execution_scope_tool(
        request,
        state,
        http_client,
        runtime,
        database,
        project_root.as_path(),
        tool_name,
        arguments,
        tool_result_max_chars,
    )
    .await?;
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
        .or(normalized_path.as_deref())
        .unwrap_or(".");
    let history_body = json!({
        "command": command,
        "args": [],
        "cwd": cwd_label,
        "success": structured.get("success").and_then(Value::as_bool).unwrap_or(false),
        "exit_code": structured.get("exit_code").and_then(Value::as_i64),
        "timed_out": structured.get("timed_out").and_then(Value::as_bool).unwrap_or(false),
        "stdout": structured.get("stdout").or_else(|| structured.get("output")).and_then(Value::as_str).unwrap_or_default(),
        "stderr": structured.get("stderr").and_then(Value::as_str).unwrap_or_default(),
    });
    history_recorder
        .append(command_history_entry_from_exec_result(
            state,
            request,
            &CommandExecutionContext::local_mcp(request, "execute_command"),
            command,
            &[],
            cwd_label,
            local_now_rfc3339(),
            &history_body,
        ))
        .await;
    Ok(result)
}

fn terminal_command_text(arguments: &Value) -> Option<String> {
    arguments
        .get("common")
        .and_then(Value::as_str)
        .or_else(|| arguments.get("command").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn terminal_approval_denied_body(command: &str, cwd: &str, timeout_ms: u64, reason: &str) -> Value {
    json!({
        "command": command,
        "args": [],
        "cwd": cwd,
        "success": false,
        "exit_code": Option::<i32>::None,
        "timed_out": false,
        "timeout_ms": timeout_ms,
        "stdout": "",
        "stderr": "",
        "error": reason,
        "approval_decision": "denied",
        "approval_reason": reason,
    })
}

fn mcp_text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "_structured_result": payload,
    })
}

async fn get_or_create_scope(
    request: &RelayRequest,
    state: &LocalState,
    _http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
    project_root: &Path,
) -> Result<Arc<LocalExecutionScope>> {
    let identity = execution_scope_identity(request, project_root)?;
    if identity.run_id.is_some()
        && database
            .execution_scope_is_terminal(
                identity.owner_user_id.as_str(),
                identity.project_id.as_str(),
                identity.run_id.as_deref().unwrap_or_default(),
                identity.generation,
            )
            .await?
    {
        return Err(anyhow!("local execution scope is terminal"));
    }
    if let Some(scope) = runtime
        .execution_scopes
        .read()
        .await
        .get(&identity.key)
        .cloned()
    {
        scope.ensure_accepting_invocations()?;
        scope.renew_until(identity.expires_at_unix);
        return Ok(scope);
    }

    let _creation = runtime.execution_scope_creation_lock.lock().await;
    if identity.run_id.is_some()
        && database
            .execution_scope_is_terminal(
                identity.owner_user_id.as_str(),
                identity.project_id.as_str(),
                identity.run_id.as_deref().unwrap_or_default(),
                identity.generation,
            )
            .await?
    {
        return Err(anyhow!("local execution scope is terminal"));
    }
    if let Some(scope) = runtime
        .execution_scopes
        .read()
        .await
        .get(&identity.key)
        .cloned()
    {
        scope.ensure_accepting_invocations()?;
        scope.renew_until(identity.expires_at_unix);
        return Ok(scope);
    }

    let git_workspace = prepare_local_git_execution_workspace(request, project_root).await?;
    let execution_project_root = git_workspace
        .as_ref()
        .map(|workspace| workspace.run_project_root.as_path())
        .unwrap_or(project_root);
    let mut policy = state.sandbox.effective_policy_defaults();
    policy.sandbox_mode = SandboxBackendKind::LocalProcess;
    let permissions = state.sandbox.effective_permissions(
        None,
        &policy,
        vec![execution_project_root.to_string_lossy().to_string()],
    );
    let limits = LocalSandboxResourceLimits::default();
    let capability = native_process_sandbox_capability().await;
    if capability.status != SandboxBackendReadinessStatus::Ready {
        return Err(anyhow!(capability.message));
    }
    start_native_sandbox_process(
        runtime,
        identity.scope_id.as_str(),
        execution_project_root,
        &policy,
        &permissions,
        &limits,
        identity.project_id.as_str(),
        identity.owner_user_id.as_str(),
    )
    .await?;
    let scope = Arc::new(LocalExecutionScope {
        scope_id: identity.scope_id,
        generation: identity.generation,
        git_workspace,
        active_invocations: AtomicUsize::new(0),
        draining: std::sync::atomic::AtomicBool::new(false),
        releasing: std::sync::atomic::AtomicBool::new(false),
        expires_at_unix: std::sync::atomic::AtomicI64::new(identity.expires_at_unix),
        lifecycle_lock: tokio::sync::Mutex::new(()),
    });
    runtime
        .execution_scopes
        .write()
        .await
        .insert(identity.key, scope.clone());
    Ok(scope)
}

async fn prepare_local_git_execution_workspace(
    request: &RelayRequest,
    project_root: &Path,
) -> Result<Option<LocalGitExecutionWorkspace>> {
    let Some(execution_group_id) = relay_header(request, EXECUTION_GROUP_ID_HEADER) else {
        return Ok(None);
    };
    let run_id = relay_header(request, RUN_ID_HEADER)
        .ok_or_else(|| anyhow!("local Git execution workspace is missing run identity"))?;
    let repository_root = ensure_local_git_repository(project_root).await?;
    let repository_root = repository_root
        .canonicalize()
        .context("canonicalize local Git repository root")?;
    let canonical_project_root = project_root
        .canonicalize()
        .context("canonicalize local project root")?;
    let project_relative_root = canonical_project_root
        .strip_prefix(repository_root.as_path())
        .context("local project root is outside its Git repository")?;
    let group_digest = short_digest(execution_group_id);
    let run_digest = short_digest(run_id);
    let group_root = repository_root
        .join(".chatos")
        .join("executions")
        .join(group_digest.as_str());
    ensure_local_chatos_git_exclude(repository_root.as_path()).await?;
    let integration_worktree = group_root.join("integration");
    let run_worktree = group_root.join("runs").join(run_digest.as_str());
    tokio::fs::create_dir_all(group_root.join("runs"))
        .await
        .context("create local ChatOS execution workspace directories")?;

    let snapshot_path = group_root.join("snapshot_commit");
    let snapshot_commit = match tokio::fs::read_to_string(snapshot_path.as_path()).await {
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => {
            let snapshot = create_dirty_worktree_snapshot(
                repository_root.as_path(),
                group_root.as_path(),
                execution_group_id,
            )
            .await?;
            tokio::fs::write(snapshot_path.as_path(), format!("{snapshot}\n"))
                .await
                .context("persist local execution snapshot commit")?;
            snapshot
        }
    };
    let execution_branch_ref = format!("chatos/executions/local-{group_digest}");
    ensure_git_worktree(
        repository_root.as_path(),
        integration_worktree.as_path(),
        execution_branch_ref.as_str(),
        snapshot_commit.as_str(),
    )
    .await?;
    let run_base_commit =
        git_stdout(integration_worktree.as_path(), &["rev-parse", "HEAD"]).await?;
    ensure_git_worktree(
        repository_root.as_path(),
        run_worktree.as_path(),
        format!("chatos/runs/local-{run_digest}").as_str(),
        run_base_commit.as_str(),
    )
    .await?;
    let run_project_root = run_worktree.join(project_relative_root);
    if !run_project_root.is_dir() {
        return Err(anyhow!(
            "local Run worktree does not contain project root {}",
            project_relative_root.display()
        ));
    }
    Ok(Some(LocalGitExecutionWorkspace {
        execution_group_id: execution_group_id.to_string(),
        execution_branch_ref,
        snapshot_commit: run_base_commit,
        integration_worktree,
        run_worktree,
        run_project_root,
    }))
}

async fn ensure_local_git_repository(project_root: &Path) -> Result<PathBuf> {
    if git_status(project_root, &["rev-parse", "--is-inside-work-tree"]).await? != 0 {
        git_stdout(project_root, &["init"]).await?;
        git_stdout_with_identity(
            project_root,
            &[
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "--no-verify",
                "-m",
                "Initialize ChatOS local execution repository",
            ],
        )
        .await?;
    }
    let repository_root =
        PathBuf::from(git_stdout(project_root, &["rev-parse", "--show-toplevel"]).await?);
    Ok(repository_root)
}

async fn ensure_local_chatos_git_exclude(repository_root: &Path) -> Result<()> {
    let git_dir = PathBuf::from(git_stdout(repository_root, &["rev-parse", "--git-dir"]).await?);
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        repository_root.join(git_dir)
    };
    let exclude_path = git_dir.join("info").join("exclude");
    let current = tokio::fs::read_to_string(exclude_path.as_path())
        .await
        .unwrap_or_default();
    if current.lines().any(|line| line.trim() == ".chatos/") {
        return Ok(());
    }
    if let Some(parent) = exclude_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("create local Git info directory")?;
    }
    let mut updated = current;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(".chatos/\n");
    tokio::fs::write(exclude_path, updated)
        .await
        .context("exclude local ChatOS execution workspace from Git status")
}

async fn create_dirty_worktree_snapshot(
    repository_root: &Path,
    group_root: &Path,
    execution_group_id: &str,
) -> Result<String> {
    let head = git_stdout(repository_root, &["rev-parse", "HEAD"]).await?;
    let head_tree = git_stdout(repository_root, &["rev-parse", "HEAD^{tree}"]).await?;
    let index_path = group_root.join("snapshot.index");
    let _ = tokio::fs::remove_file(index_path.as_path()).await;
    git_stdout_with_index(
        repository_root,
        index_path.as_path(),
        &["read-tree", "HEAD"],
    )
    .await?;
    git_stdout_with_index(
        repository_root,
        index_path.as_path(),
        &["add", "-A", "--", "."],
    )
    .await?;
    let tree =
        git_stdout_with_index(repository_root, index_path.as_path(), &["write-tree"]).await?;
    let snapshot = if tree == head_tree {
        head
    } else {
        git_stdout_with_identity_and_index(
            repository_root,
            index_path.as_path(),
            &[
                "commit-tree",
                tree.as_str(),
                "-p",
                head.as_str(),
                "-m",
                format!("ChatOS local execution snapshot {execution_group_id}").as_str(),
            ],
        )
        .await?
    };
    let _ = tokio::fs::remove_file(index_path).await;
    Ok(snapshot)
}

async fn ensure_git_worktree(
    repository_root: &Path,
    worktree: &Path,
    branch_ref: &str,
    start_commit: &str,
) -> Result<()> {
    if worktree.join(".git").exists() {
        return Ok(());
    }
    if worktree.exists() {
        let mut entries = tokio::fs::read_dir(worktree)
            .await
            .context("inspect existing local Git worktree directory")?;
        if entries
            .next_entry()
            .await
            .context("read existing local Git worktree directory")?
            .is_some()
        {
            return Err(anyhow!(
                "local Git worktree path is occupied: {}",
                worktree.display()
            ));
        }
    }
    let branch_exists = git_status(
        repository_root,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            format!("refs/heads/{branch_ref}").as_str(),
        ],
    )
    .await?
        == 0;
    let worktree_text = worktree.to_string_lossy().to_string();
    if branch_exists {
        git_stdout(
            repository_root,
            &["worktree", "add", worktree_text.as_str(), branch_ref],
        )
        .await?;
    } else {
        git_stdout(
            repository_root,
            &[
                "worktree",
                "add",
                "-b",
                branch_ref,
                worktree_text.as_str(),
                start_commit,
            ],
        )
        .await?;
    }
    Ok(())
}

async fn finalize_local_git_execution_workspace(
    workspace: &LocalGitExecutionWorkspace,
    run_id: &str,
    should_integrate: bool,
) -> Result<Value> {
    git_stdout(workspace.run_worktree.as_path(), &["add", "-A"]).await?;
    let status = git_stdout(
        workspace.run_worktree.as_path(),
        &["status", "--porcelain=v1"],
    )
    .await?;
    let changed = !status.trim().is_empty();
    if changed {
        git_stdout_with_identity(
            workspace.run_worktree.as_path(),
            &["commit", "-m", format!("ChatOS Run {run_id}").as_str()],
        )
        .await?;
    }
    let result_commit =
        git_stdout(workspace.run_worktree.as_path(), &["rev-parse", "HEAD"]).await?;
    let patch = git_stdout(
        workspace.run_worktree.as_path(),
        &[
            "diff",
            "--binary",
            workspace.snapshot_commit.as_str(),
            result_commit.as_str(),
        ],
    )
    .await?;
    let (patch, patch_truncated) = truncate_text_bytes(patch, FINALIZATION_PATCH_MAX_BYTES);
    let files = git_stdout(
        workspace.run_worktree.as_path(),
        &[
            "diff",
            "--name-status",
            workspace.snapshot_commit.as_str(),
            result_commit.as_str(),
        ],
    )
    .await?
    .lines()
    .filter_map(parse_git_name_status)
    .collect::<Vec<_>>();
    let has_result_changes = !files.is_empty();
    let current_integration_commit = git_stdout(
        workspace.integration_worktree.as_path(),
        &["rev-parse", "HEAD"],
    )
    .await?;
    if !should_integrate || !has_result_changes {
        return Ok(json!({
            "ok": true,
            "status": "no_changes",
            "execution_group_id": workspace.execution_group_id,
            "execution_branch_ref": workspace.execution_branch_ref,
            "base_commit": workspace.snapshot_commit,
            "result_commit": result_commit,
            "integrated_commit": current_integration_commit,
            "conflict_files": [],
            "files": files,
            "patch": patch,
            "patch_truncated": patch_truncated,
        }));
    }
    if git_status(
        workspace.integration_worktree.as_path(),
        &[
            "merge-base",
            "--is-ancestor",
            result_commit.as_str(),
            "HEAD",
        ],
    )
    .await?
        == 0
    {
        return Ok(json!({
            "ok": true,
            "status": "succeeded",
            "execution_group_id": workspace.execution_group_id,
            "execution_branch_ref": workspace.execution_branch_ref,
            "base_commit": workspace.snapshot_commit,
            "result_commit": result_commit,
            "integrated_commit": current_integration_commit,
            "conflict_files": [],
            "files": files,
            "patch": patch,
            "patch_truncated": patch_truncated,
        }));
    }
    let cherry_pick_status = git_status_with_identity(
        workspace.integration_worktree.as_path(),
        &["cherry-pick", result_commit.as_str()],
    )
    .await?;
    if cherry_pick_status == 0 {
        let integrated_commit = git_stdout(
            workspace.integration_worktree.as_path(),
            &["rev-parse", "HEAD"],
        )
        .await?;
        return Ok(json!({
            "ok": true,
            "status": "succeeded",
            "execution_group_id": workspace.execution_group_id,
            "execution_branch_ref": workspace.execution_branch_ref,
            "base_commit": workspace.snapshot_commit,
            "result_commit": result_commit,
            "integrated_commit": integrated_commit,
            "conflict_files": [],
            "files": files,
            "patch": patch,
            "patch_truncated": patch_truncated,
        }));
    }
    let conflict_files = git_stdout(
        workspace.integration_worktree.as_path(),
        &["diff", "--name-only", "--diff-filter=U"],
    )
    .await?
    .lines()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned)
    .collect::<Vec<_>>();
    let _ = git_status(
        workspace.integration_worktree.as_path(),
        &["cherry-pick", "--abort"],
    )
    .await;
    if conflict_files.is_empty() {
        return Err(anyhow!(
            "local execution integration failed without Git conflicts"
        ));
    }
    Ok(json!({
        "ok": true,
        "status": "conflict",
        "execution_group_id": workspace.execution_group_id,
        "execution_branch_ref": workspace.execution_branch_ref,
        "base_commit": workspace.snapshot_commit,
        "result_commit": result_commit,
        "integrated_commit": Value::Null,
        "conflict_files": conflict_files,
        "files": files,
        "message": "Local Run changes conflict with the execution integration branch",
        "patch": patch,
        "patch_truncated": patch_truncated,
    }))
}

fn parse_git_name_status(line: &str) -> Option<Value> {
    let mut parts = line.split('\t');
    let status = parts.next()?.trim();
    let first_path = parts.next()?.trim();
    if status.is_empty() || first_path.is_empty() {
        return None;
    }
    let second_path = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Some(if let Some(path) = second_path {
        json!({
            "status": status.chars().next().unwrap_or('M').to_string(),
            "path": path,
            "old_path": first_path,
        })
    } else {
        json!({
            "status": status.chars().next().unwrap_or('M').to_string(),
            "path": first_path,
            "old_path": Value::Null,
        })
    })
}

fn short_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))[..24].to_string()
}

fn truncate_text_bytes(mut value: String, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    (value, true)
}

async fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String> {
    git_stdout_command(cwd, args, None, false).await
}

async fn git_stdout_with_identity(cwd: &Path, args: &[&str]) -> Result<String> {
    git_stdout_command(cwd, args, None, true).await
}

async fn git_stdout_with_index(cwd: &Path, index: &Path, args: &[&str]) -> Result<String> {
    git_stdout_command(cwd, args, Some(index), false).await
}

async fn git_stdout_with_identity_and_index(
    cwd: &Path,
    index: &Path,
    args: &[&str],
) -> Result<String> {
    git_stdout_command(cwd, args, Some(index), true).await
}

async fn git_stdout_command(
    cwd: &Path,
    args: &[&str],
    index: Option<&Path>,
    identity: bool,
) -> Result<String> {
    let mut command = tokio::process::Command::new("git");
    command.current_dir(cwd).args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    if identity {
        command
            .env("GIT_AUTHOR_NAME", "ChatOS")
            .env("GIT_AUTHOR_EMAIL", "chatos@local")
            .env("GIT_COMMITTER_NAME", "ChatOS")
            .env("GIT_COMMITTER_EMAIL", "chatos@local");
    }
    let output = command
        .output()
        .await
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(output.stderr.as_slice()).trim()
        ));
    }
    Ok(String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string())
}

async fn git_status(cwd: &Path, args: &[&str]) -> Result<i32> {
    git_status_command(cwd, args, false).await
}

async fn git_status_with_identity(cwd: &Path, args: &[&str]) -> Result<i32> {
    git_status_command(cwd, args, true).await
}

async fn git_status_command(cwd: &Path, args: &[&str], identity: bool) -> Result<i32> {
    let mut command = tokio::process::Command::new("git");
    command.current_dir(cwd).args(args);
    if identity {
        command
            .env("GIT_AUTHOR_NAME", "ChatOS")
            .env("GIT_AUTHOR_EMAIL", "chatos@local")
            .env("GIT_COMMITTER_NAME", "ChatOS")
            .env("GIT_COMMITTER_EMAIL", "chatos@local");
    }
    let output = command
        .output()
        .await
        .with_context(|| format!("run git {}", args.join(" ")))?;
    Ok(output.status.code().unwrap_or(1))
}

struct ExecutionScopeIdentity {
    key: String,
    scope_id: String,
    owner_user_id: String,
    project_id: String,
    run_id: Option<String>,
    generation: i64,
    expires_at_unix: i64,
}

fn execution_scope_identity(
    request: &RelayRequest,
    project_root: &Path,
) -> Result<ExecutionScopeIdentity> {
    let owner_user_id = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local execution scope is missing owner identity"))?
        .to_string();
    let project_id = relay_header(request, PROJECT_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope is missing project identity"))?
        .to_string();
    let session_id = relay_header(request, SESSION_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope is missing runtime session identity"))?;
    let run_id = relay_header(request, RUN_ID_HEADER).map(ToOwned::to_owned);
    let run_or_session = run_id.as_deref().unwrap_or(session_id);
    let generation = if run_id.is_some() {
        required_scope_generation(request)?
    } else {
        1
    };
    let expires_at_unix = relay_header(request, SESSION_EXPIRES_AT_UNIX_HEADER)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > Utc::now().timestamp())
        .ok_or_else(|| anyhow!("local execution scope has an invalid session expiry"))?;
    let canonical_root = project_root
        .canonicalize()
        .context("canonicalize local execution scope project root")?;
    let key = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        owner_user_id, project_id, run_or_session, generation
    );
    let digest_material = format!(
        "{}\u{1f}{}\u{1f}{}",
        key,
        generation,
        canonical_root.to_string_lossy()
    );
    let digest = hex::encode(Sha256::digest(digest_material.as_bytes()));
    Ok(ExecutionScopeIdentity {
        key,
        scope_id: format!("mcp-scope-{}", &digest[..24]),
        owner_user_id,
        project_id,
        run_id,
        generation,
        expires_at_unix,
    })
}

fn required_scope_generation(request: &RelayRequest) -> Result<i64> {
    relay_header(request, SCOPE_GENERATION_HEADER)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow!("local execution scope is missing a valid generation"))
}

fn tool_call_body(
    request: &RelayRequest,
    tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
) -> Value {
    let mut params = json!({ "name": tool_name, "arguments": arguments });
    if let Some(max_chars) = tool_result_max_chars {
        params["_meta"] = json!({ TOOL_RESULT_MAX_CHARS_META_KEY: max_chars });
    }
    json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or_else(|| json!(request.request_id)),
        "method": "tools/call",
        "params": params,
    })
}

async fn call_scope_mcp(
    _http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    scope: &LocalExecutionScope,
    request: &Value,
) -> Result<Value> {
    call_native_sandbox_mcp(runtime, scope.scope_id.as_str(), request).await
}

fn decode_tool_result(response: Value, expected_id: Option<&Value>) -> Result<Value> {
    if response.get("id") != expected_id {
        return Err(anyhow!(
            "local execution agent response id does not match the invocation"
        ));
    }
    if let Some(error) = response.get("error") {
        return Err(anyhow!("local execution agent tool call failed: {error}"));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow!("local execution agent response is missing result"))
}

fn relay_header<'a>(request: &'a RelayRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn spawn_local_execution_scope_reaper(runtime: LocalSandboxRuntime) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(EXECUTION_SCOPE_REAPER_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            reap_expired_execution_scopes(&runtime).await;
        }
    })
}

async fn reap_expired_execution_scopes(runtime: &LocalSandboxRuntime) {
    let now = Utc::now().timestamp();
    let expired = runtime
        .execution_scopes
        .read()
        .await
        .iter()
        .filter(|(_, scope)| scope.ready_for_release(now))
        .map(|(key, scope)| (key.clone(), scope.clone()))
        .collect::<Vec<_>>();
    for (key, scope) in expired {
        let result = try_release_scope(runtime, key.as_str(), &scope).await;
        if let Err(error) = result {
            crate::tracing_stdout(
                format!(
                    "release expired local MCP execution scope {} failed: {error:#}",
                    scope.scope_id
                )
                .as_str(),
            );
        }
    }
}

async fn release_scope(runtime: &LocalSandboxRuntime, scope: &LocalExecutionScope) -> Result<()> {
    destroy_native_sandbox_process(runtime, scope.scope_id.as_str()).await
}

async fn release_drained_scope(runtime: &LocalSandboxRuntime, scope: &Arc<LocalExecutionScope>) {
    let key = runtime
        .execution_scopes
        .read()
        .await
        .iter()
        .find(|(_, current)| Arc::ptr_eq(current, scope))
        .map(|(key, _)| key.clone());
    if let Some(key) = key {
        if let Err(error) = try_release_scope(runtime, key.as_str(), scope).await {
            crate::tracing_stdout(
                format!(
                    "release drained local MCP execution scope {} failed: {error:#}",
                    scope.scope_id
                )
                .as_str(),
            );
        }
    }
}

async fn try_release_scope(
    runtime: &LocalSandboxRuntime,
    key: &str,
    scope: &Arc<LocalExecutionScope>,
) -> Result<bool> {
    let _lifecycle = scope.lifecycle_lock.lock().await;
    if scope.active_invocations.load(Ordering::Acquire) != 0 {
        return Ok(false);
    }
    scope.draining.store(true, Ordering::Release);
    if scope
        .releasing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(false);
    }
    if scope.active_invocations.load(Ordering::Acquire) != 0 {
        scope.releasing.store(false, Ordering::Release);
        return Ok(false);
    }
    if let Err(error) = release_scope(runtime, scope).await {
        scope.releasing.store(false, Ordering::Release);
        return Err(error);
    }
    let mut scopes = runtime.execution_scopes.write().await;
    if scopes
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, scope))
    {
        scopes.remove(key);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::process::Command;

    fn request(_root: &Path, run_id: Option<&str>) -> RelayRequest {
        let mut headers = BTreeMap::from([
            (PROJECT_ID_HEADER.to_string(), "project-1".to_string()),
            (SESSION_ID_HEADER.to_string(), "session-1".to_string()),
            (
                SESSION_EXPIRES_AT_UNIX_HEADER.to_string(),
                (Utc::now().timestamp() + 300).to_string(),
            ),
        ]);
        if let Some(run_id) = run_id {
            headers.insert(RUN_ID_HEADER.to_string(), run_id.to_string());
            headers.insert(SCOPE_GENERATION_HEADER.to_string(), "1".to_string());
        }
        RelayRequest {
            _message_type: "mcp".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/mcp".to_string()),
            headers,
            body: json!({}),
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        }
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(output.stderr.as_slice())
        );
        String::from_utf8_lossy(output.stdout.as_slice())
            .trim()
            .to_string()
    }

    fn git_repository() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        git(root.path(), &["init", "-b", "main"]);
        git(root.path(), &["config", "user.name", "Test"]);
        git(root.path(), &["config", "user.email", "test@example.com"]);
        fs::write(root.path().join("game.txt"), "base\n").unwrap();
        git(root.path(), &["add", "game.txt"]);
        git(root.path(), &["commit", "-m", "initial"]);
        root
    }

    fn execution_request(root: &Path, group_id: &str, run_id: &str) -> RelayRequest {
        let mut request = request(root, Some(run_id));
        request
            .headers
            .insert(EXECUTION_GROUP_ID_HEADER.to_string(), group_id.to_string());
        request
    }

    #[test]
    fn run_identity_is_stable_across_runtime_sessions() {
        let root = tempfile::tempdir().unwrap();
        let first = request(root.path(), Some("run-1"));
        let mut second = request(root.path(), Some("run-1"));
        second
            .headers
            .insert(SESSION_ID_HEADER.to_string(), "session-2".to_string());
        assert_eq!(
            execution_scope_identity(&first, root.path()).unwrap().key,
            execution_scope_identity(&second, root.path()).unwrap().key
        );
    }

    #[test]
    fn session_identity_is_used_when_run_is_absent() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("probe.txt"), "ok").unwrap();
        let first = request(root.path(), None);
        let mut second = request(root.path(), None);
        second
            .headers
            .insert(SESSION_ID_HEADER.to_string(), "session-2".to_string());
        assert_ne!(
            execution_scope_identity(&first, root.path()).unwrap().key,
            execution_scope_identity(&second, root.path()).unwrap().key
        );
    }

    #[test]
    fn generation_separates_recreated_run_scopes() {
        let root = tempfile::tempdir().unwrap();
        let first = request(root.path(), Some("run-1"));
        let mut recreated = request(root.path(), Some("run-1"));
        recreated
            .headers
            .insert(SCOPE_GENERATION_HEADER.to_string(), "2".to_string());
        let first = execution_scope_identity(&first, root.path()).unwrap();
        let recreated = execution_scope_identity(&recreated, root.path()).unwrap();
        assert_ne!(first.key, recreated.key);
        assert_ne!(first.scope_id, recreated.scope_id);
    }

    #[tokio::test]
    async fn draining_scope_rejects_new_invocations_without_leaking_a_reference() {
        let runtime = LocalSandboxRuntime::default();
        let scope = Arc::new(LocalExecutionScope {
            scope_id: "scope-1".to_string(),
            generation: 1,
            git_workspace: None,
            active_invocations: AtomicUsize::new(0),
            draining: std::sync::atomic::AtomicBool::new(true),
            releasing: std::sync::atomic::AtomicBool::new(false),
            expires_at_unix: std::sync::atomic::AtomicI64::new(0),
            lifecycle_lock: tokio::sync::Mutex::new(()),
        });
        assert!(scope.begin_invocation(&runtime).await.is_err());
        assert_eq!(scope.active_invocations.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn local_git_workspace_snapshots_dirty_and_untracked_files_without_touching_index() {
        let root = git_repository();
        fs::write(root.path().join("game.txt"), "dirty\n").unwrap();
        fs::write(root.path().join("notes.txt"), "untracked\n").unwrap();
        fs::write(root.path().join("staged.txt"), "staged\n").unwrap();
        git(root.path(), &["add", "staged.txt"]);
        let staged_before = git(root.path(), &["diff", "--cached", "--name-only"]);
        let request = execution_request(root.path(), "group-dirty", "run-dirty");

        let workspace = prepare_local_git_execution_workspace(&request, root.path())
            .await
            .unwrap()
            .expect("Git execution workspace");

        assert_eq!(
            fs::read_to_string(workspace.run_project_root.join("game.txt")).unwrap(),
            "dirty\n"
        );
        assert_eq!(
            fs::read_to_string(workspace.run_project_root.join("notes.txt")).unwrap(),
            "untracked\n"
        );
        assert_eq!(
            git(root.path(), &["diff", "--cached", "--name-only"]),
            staged_before
        );
        assert_eq!(
            fs::read_to_string(root.path().join("game.txt")).unwrap(),
            "dirty\n"
        );
    }

    #[tokio::test]
    async fn local_execution_group_initializes_an_empty_project_repository() {
        let root = tempfile::tempdir().unwrap();
        let request = execution_request(root.path(), "group-empty", "run-empty");

        let workspace = prepare_local_git_execution_workspace(&request, root.path())
            .await
            .unwrap()
            .expect("Git execution workspace");

        assert!(root.path().join(".git").is_dir());
        assert!(workspace.run_project_root.is_dir());
        assert!(!git(root.path(), &["rev-parse", "HEAD"]).is_empty());
    }

    #[tokio::test]
    async fn local_git_runs_integrate_serially_and_report_same_line_conflict() {
        let root = git_repository();
        let first_request = execution_request(root.path(), "group-conflict", "run-first");
        let second_request = execution_request(root.path(), "group-conflict", "run-second");
        let first = prepare_local_git_execution_workspace(&first_request, root.path())
            .await
            .unwrap()
            .expect("first Git workspace");
        let second = prepare_local_git_execution_workspace(&second_request, root.path())
            .await
            .unwrap()
            .expect("second Git workspace");
        fs::write(first.run_project_root.join("game.txt"), "first\n").unwrap();
        fs::write(second.run_project_root.join("game.txt"), "second\n").unwrap();

        let first_result = finalize_local_git_execution_workspace(&first, "run-first", true)
            .await
            .unwrap();
        assert_eq!(first_result["status"], "succeeded");
        let second_result = finalize_local_git_execution_workspace(&second, "run-second", true)
            .await
            .unwrap();
        assert_eq!(second_result["status"], "conflict");
        assert_eq!(second_result["conflict_files"], json!(["game.txt"]));
        let second_commit = second_result["result_commit"].as_str().unwrap();
        git(
            first.integration_worktree.as_path(),
            &[
                "merge",
                "-s",
                "ours",
                second_commit,
                "-m",
                "resolve conflict",
            ],
        );
        let retried = finalize_local_git_execution_workspace(&second, "run-second", true)
            .await
            .unwrap();
        assert_eq!(retried["status"], "succeeded");
        assert_eq!(
            fs::read_to_string(first.integration_worktree.join("game.txt")).unwrap(),
            "first\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("game.txt")).unwrap(),
            "base\n"
        );
    }
}
