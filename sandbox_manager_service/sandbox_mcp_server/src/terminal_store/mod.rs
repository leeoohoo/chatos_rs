// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chatos_builtin_tools::{
    path_with_bundled_tools, terminal_process_list_entry, terminal_process_list_response,
    terminal_process_log_response, terminal_process_poll_response, terminal_process_wait_response,
    terminal_recent_logs_entry, terminal_recent_logs_response, TerminalControllerContext,
    TerminalControllerStore, TerminalProcessPollDetails, TerminalProcessSnapshot,
    TerminalProcessWaitResponse, TerminalRecentLogsEntry,
};
use chatos_terminal_runtime::{
    read_output_chunks, wait_for_terminal_session, TerminalLogBuffer, TerminalSessionMeta,
    TerminalWaitResult,
};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

mod logs;

use crate::command_sandbox::{
    CommandSandboxCleanup, CommandSandboxConfig, PreparedSandboxCommand, SpawnedSandboxCommand,
};
use crate::quota::WorkspaceQuota;
use logs::{
    append_log, collect_output, collect_output_from_logs, derive_terminal_name, log_value_content,
    select_logs, take_recent_logs,
};

#[derive(Debug, Clone)]
pub struct SandboxTerminalControllerStore {
    workspace_quota: WorkspaceQuota,
    command_sandbox: CommandSandboxConfig,
}

impl SandboxTerminalControllerStore {
    pub(crate) fn new(
        workspace_quota: WorkspaceQuota,
        command_sandbox: CommandSandboxConfig,
    ) -> Self {
        Self {
            workspace_quota,
            command_sandbox,
        }
    }
}

struct TerminalSession {
    meta: Mutex<TerminalSessionMeta>,
    child: Mutex<Child>,
    logs: Mutex<TerminalLogBuffer>,
    cleanup: Mutex<Option<CommandSandboxCleanup>>,
}

#[derive(Default)]
struct TerminalRuntimeState {
    sessions: RwLock<HashMap<String, Arc<TerminalSession>>>,
}

static TERMINAL_STATE: OnceLock<Arc<TerminalRuntimeState>> = OnceLock::new();

fn terminal_state() -> &'static Arc<TerminalRuntimeState> {
    TERMINAL_STATE.get_or_init(|| Arc::new(TerminalRuntimeState::default()))
}

#[async_trait]
impl TerminalControllerStore for SandboxTerminalControllerStore {
    async fn execute_command(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
        permissions: chatos_builtin_tools::TerminalCommandPermissions,
    ) -> Result<Value, String> {
        self.workspace_quota.check().await?;
        let project_root = canonicalize_existing(context.root.as_path())?;
        let target_path = resolve_target_path(project_root.as_path(), path.as_str())?;
        let shell = shell_path();

        let mut prepared = PreparedSandboxCommand::new(
            &self.command_sandbox,
            shell.as_str(),
            command.as_str(),
            target_path.as_path(),
            &permissions,
        )?;
        prepared
            .command_mut()
            .current_dir(&target_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        apply_bundled_tools_path(prepared.command_mut());
        prepared.command_mut().env("CHATOS_SANDBOX", "1");

        let spawned = prepared.spawn()?;
        let session = register_session(
            context.clone(),
            target_path.clone(),
            command.clone(),
            spawned,
        )
        .await?;
        start_status_monitor(session.clone());
        start_workspace_quota_monitor(session.clone(), self.workspace_quota.clone());
        append_log(session.clone(), "command", format!("{command}\n")).await;
        let session_id = session.meta.lock().await.id.clone();

        if background {
            return Ok(json!({
                "project_root": project_root.to_string_lossy(),
                "terminal_id": session_id,
                "process_id": session_id,
                "path": target_path.to_string_lossy(),
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
            }));
        }

        let wait_result = wait_for_session(session.clone(), context.max_wait_ms).await?;
        let output = collect_output(&session, context.max_output_chars).await;
        Ok(json!({
            "project_root": project_root.to_string_lossy(),
            "terminal_id": session_id.clone(),
            "process_id": session_id,
            "path": target_path.to_string_lossy(),
            "common": command,
            "background": false,
            "busy": wait_result.busy,
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

    async fn get_recent_logs(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> Result<Value, String> {
        let sessions = sessions_for_context(&context).await?;
        let total = sessions.len();
        let mut terminals = Vec::new();
        for session in sessions.into_iter().take(terminal_limit) {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            let logs = session.logs.lock().await;
            let recent = take_recent_logs(&logs, per_terminal_limit.max(1) as usize);
            terminals.push(terminal_recent_logs_entry(TerminalRecentLogsEntry {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(meta.cwd.as_str()),
                status: meta.status,
                cwd: meta.cwd,
                project_id: meta.project_id,
                last_active_at: meta.last_active_at,
                log_count: logs.len(),
                logs: recent,
            }));
        }
        Ok(terminal_recent_logs_response(
            terminals,
            total,
            per_terminal_limit,
            terminal_limit,
        ))
    }

    async fn process_list(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        let sessions = sessions_for_context(&context).await?;
        let mut processes = Vec::new();
        for session in sessions {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            if !include_exited && meta.status == "exited" {
                continue;
            }
            let output = collect_output(&session, 1200).await;
            let is_exited = meta.status == "exited";
            processes.push(terminal_process_list_entry(TerminalProcessSnapshot {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(meta.cwd.as_str()),
                status: meta.status,
                process_status: if is_exited { "exited" } else { "running" }.to_string(),
                busy: !is_exited,
                command: meta.command,
                started_at: meta.started_at,
                cwd: meta.cwd,
                project_id: meta.project_id,
                last_active_at: meta.last_active_at,
                output_preview: output.text,
                output_tail_chars: output.char_count,
                exit_code: meta.exit_code,
            }));
            if processes.len() >= limit {
                break;
            }
        }
        Ok(terminal_process_list_response(
            processes,
            include_exited,
            limit,
        ))
    }

    async fn process_poll(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        refresh_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        let logs = session.logs.lock().await;
        let effective_limit = limit.clamp(1, 200) as usize;
        let selected = select_logs(&logs, offset, effective_limit);
        let output = collect_output_from_logs(selected.iter().filter_map(log_value_content), 1200);
        let is_exited = meta.status == "exited";
        Ok(terminal_process_poll_response(
            TerminalProcessSnapshot {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(meta.cwd.as_str()),
                status: meta.status,
                process_status: if is_exited { "exited" } else { "running" }.to_string(),
                busy: !is_exited,
                command: meta.command,
                started_at: meta.started_at,
                cwd: meta.cwd,
                project_id: meta.project_id,
                last_active_at: meta.last_active_at,
                output_preview: output.text,
                output_tail_chars: output.char_count,
                exit_code: meta.exit_code,
            },
            TerminalProcessPollDetails {
                offset,
                limit: effective_limit,
                has_more: offset.is_some() && logs.len() > selected.len(),
                logs: selected,
            },
        ))
    }

    async fn process_log(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        let poll = self
            .process_poll(context, terminal_id, offset, limit)
            .await?;
        Ok(terminal_process_log_response(&poll, offset, limit))
    }

    async fn process_wait(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        let result = wait_for_session(session.clone(), timeout_ms).await?;
        let output = collect_output(&session, context.max_output_chars).await;
        let meta = session.meta.lock().await.clone();
        let is_exited = meta.status == "exited";
        Ok(terminal_process_wait_response(
            TerminalProcessWaitResponse {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(meta.cwd.as_str()),
                status: meta.status,
                wait_status: if result.timed_out {
                    "timeout"
                } else if is_exited {
                    "exited"
                } else {
                    "running"
                }
                .to_string(),
                busy: result.busy,
                exited: is_exited,
                completed: !result.timed_out,
                timed_out: result.timed_out,
                finished_by: result.finished_by.to_string(),
                exit_code: result.exit_code,
                timeout_ms,
                waited_ms: result.waited_ms,
                output: output.text,
                output_chars: output.char_count,
                truncated: output.truncated,
            },
        ))
    }

    async fn process_write(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        refresh_session_status(&session).await?;
        let mut child = session.child.lock().await;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "terminal stdin is unavailable".to_string())?;
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
        drop(child);
        let mut content = data.clone();
        if submit {
            content.push('\n');
        }
        append_log(session.clone(), "input", content).await;
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
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        {
            let mut child = session.child.lock().await;
            child.kill().await.map_err(|err| err.to_string())?;
            let _ = child.wait().await;
        }
        mark_session_exited(&session, None).await;
        append_log(session.clone(), "system", "[terminal killed]\n".to_string()).await;
        Ok(json!({
            "ok": true,
            "terminal_id": terminal_id,
            "killed": true,
        }))
    }
}

fn start_workspace_quota_monitor(session: Arc<TerminalSession>, quota: WorkspaceQuota) {
    if !quota.is_enabled() {
        return;
    }
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_millis(250)).await;
            if session.meta.lock().await.status == "exited" {
                return;
            }
            let Err(err) = quota.check().await else {
                continue;
            };
            {
                let mut child = session.child.lock().await;
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            mark_session_exited(&session, None).await;
            append_log(
                session.clone(),
                "system",
                format!("[workspace quota terminated process: {err}]\n"),
            )
            .await;
            return;
        }
    });
}

fn shell_path() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|value| Path::new(value).exists())
        .or_else(|| {
            ["/bin/bash", "/usr/bin/bash", "/bin/sh", "/usr/bin/sh"]
                .into_iter()
                .find(|path| Path::new(path).exists())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "/bin/sh".to_string())
}

fn apply_bundled_tools_path(command: &mut Command) {
    if let Some(path) = path_with_bundled_tools(std::env::var_os("PATH")) {
        command.env("PATH", path);
    }
}

async fn register_session(
    context: TerminalControllerContext,
    target_path: PathBuf,
    command: String,
    spawned: SpawnedSandboxCommand,
) -> Result<Arc<TerminalSession>, String> {
    let SpawnedSandboxCommand { mut child, cleanup } = spawned;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let session_id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let session = Arc::new(TerminalSession {
        meta: Mutex::new(TerminalSessionMeta::new(
            session_id.clone(),
            target_path.to_string_lossy().to_string(),
            context.project_id.clone(),
            context.user_id.clone(),
            command,
            now,
        )),
        child: Mutex::new(child),
        logs: Mutex::new(TerminalLogBuffer::default()),
        cleanup: Mutex::new(Some(cleanup)),
    });

    if let Some(stdout) = stdout {
        spawn_stream_reader(session.clone(), stdout, "stdout");
    }
    if let Some(stderr) = stderr {
        spawn_stream_reader(session.clone(), stderr, "stderr");
    }
    terminal_state()
        .sessions
        .write()
        .await
        .insert(session_id, session.clone());
    Ok(session)
}

fn start_status_monitor(session: Arc<TerminalSession>) {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_millis(100)).await;
            if refresh_session_status(&session).await.is_err()
                || session.meta.lock().await.status == "exited"
            {
                return;
            }
        }
    });
}

fn spawn_stream_reader<R>(session: Arc<TerminalSession>, mut reader: R, kind: &'static str)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let _ = read_output_chunks(&mut reader, move |chunk| {
            let session = session.clone();
            async move {
                append_log(session, kind, chunk).await;
            }
        })
        .await;
    });
}

async fn refresh_session_status(session: &Arc<TerminalSession>) -> Result<(), String> {
    {
        let meta = session.meta.lock().await;
        if meta.is_exited() {
            return Ok(());
        }
    }
    let status = {
        let mut child = session.child.lock().await;
        child.try_wait().map_err(|err| err.to_string())?
    };
    if let Some(status) = status {
        mark_session_exited(session, status.code()).await;
    }
    Ok(())
}

async fn mark_session_exited(session: &Arc<TerminalSession>, exit_code: Option<i32>) {
    {
        let mut meta = session.meta.lock().await;
        meta.mark_exited(exit_code, now_rfc3339());
    }
    if let Some(cleanup) = session.cleanup.lock().await.take() {
        cleanup.run();
    }
}

async fn sessions_for_context(
    context: &TerminalControllerContext,
) -> Result<Vec<Arc<TerminalSession>>, String> {
    let root = canonicalize_existing(context.root.as_path())?;
    let sessions = terminal_state().sessions.read().await;
    let mut matched = Vec::new();
    for session in sessions.values() {
        let meta = session.meta.lock().await.clone();
        if meta.matches_scope(
            root.as_path(),
            context.project_id.as_deref(),
            context.user_id.as_deref(),
        ) {
            matched.push(session.clone());
        }
    }
    matched.sort_by(|left, right| {
        let left_meta = left.meta.try_lock();
        let right_meta = right.meta.try_lock();
        match (left_meta, right_meta) {
            (Ok(left), Ok(right)) => right.last_active_at.cmp(&left.last_active_at),
            _ => std::cmp::Ordering::Equal,
        }
    });
    Ok(matched)
}

async fn session_for_context(
    context: &TerminalControllerContext,
    terminal_id: &str,
) -> Result<Arc<TerminalSession>, String> {
    let sessions = sessions_for_context(context).await?;
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

async fn wait_for_session(
    session: Arc<TerminalSession>,
    timeout_ms: u64,
) -> Result<TerminalWaitResult, String> {
    wait_for_terminal_session(timeout_ms, || {
        let session = session.clone();
        async move {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            Ok(meta)
        }
    })
    .await
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    chatos_terminal_runtime::canonicalize_existing(path).map_err(|err| err.to_string())
}

fn resolve_target_path(root: &Path, path_input: &str) -> Result<PathBuf, String> {
    chatos_terminal_runtime::resolve_target_path(root, path_input).map_err(|err| err.to_string())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
