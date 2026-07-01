// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chatos_builtin_tools::{
    path_with_bundled_tools, TerminalControllerContext, TerminalControllerStore,
};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct SandboxTerminalControllerStore;

#[derive(Debug, Clone)]
struct TerminalLogEntry {
    offset: i64,
    kind: String,
    content: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct TerminalSessionMeta {
    id: String,
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

struct TerminalSession {
    meta: Mutex<TerminalSessionMeta>,
    child: Mutex<Child>,
    logs: Mutex<Vec<TerminalLogEntry>>,
}

#[derive(Default)]
struct TerminalRuntimeState {
    sessions: RwLock<HashMap<String, Arc<TerminalSession>>>,
}

struct OutputCapture {
    text: String,
    char_count: usize,
    truncated: bool,
}

struct WaitResult {
    waited_ms: u64,
    busy: bool,
    timed_out: bool,
    finished_by: &'static str,
    exit_code: Option<i32>,
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
    ) -> Result<Value, String> {
        let project_root = canonicalize_existing(context.root.as_path())?;
        let target_path = resolve_target_path(project_root.as_path(), path.as_str())?;
        let shell = shell_path();

        let mut process = Command::new(shell.as_str());
        process
            .arg("-lc")
            .arg(command.as_str())
            .current_dir(&target_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        apply_bundled_tools_path(&mut process);
        process.env("CHATOS_SANDBOX", "1");

        let child = process.spawn().map_err(|err| err.to_string())?;
        let session =
            register_session(context.clone(), target_path.clone(), command.clone(), child).await?;
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
            terminals.push(json!({
                "terminal_id": meta.id,
                "terminal_name": derive_terminal_name(meta.cwd.as_str()),
                "status": meta.status,
                "cwd": meta.cwd,
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
            processes.push(json!({
                "terminal_id": meta.id,
                "process_id": meta.id,
                "terminal_name": derive_terminal_name(meta.cwd.as_str()),
                "status": meta.status,
                "process_status": if meta.status == "exited" { "exited" } else { "running" },
                "busy": meta.status != "exited",
                "has_session": true,
                "command": meta.command,
                "pid": Value::Null,
                "started_at": meta.started_at,
                "uptime_seconds": Value::Null,
                "cwd": meta.cwd,
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
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        refresh_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        let logs = session.logs.lock().await;
        let effective_limit = limit.clamp(1, 200) as usize;
        let selected = select_logs(&logs, offset, effective_limit);
        let output = collect_output_from_logs(selected.iter().filter_map(log_value_content), 1200);
        Ok(json!({
            "terminal_id": meta.id,
            "process_id": meta.id,
            "terminal_name": derive_terminal_name(meta.cwd.as_str()),
            "status": meta.status,
            "process_status": if meta.status == "exited" { "exited" } else { "running" },
            "busy": meta.status != "exited",
            "has_session": true,
            "command": meta.command,
            "pid": Value::Null,
            "started_at": meta.started_at,
            "uptime_seconds": Value::Null,
            "cwd": meta.cwd,
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
    ) -> Result<Value, String> {
        let poll = self
            .process_poll(context, terminal_id, offset, limit)
            .await?;
        let output = poll
            .get("logs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(log_value_content)
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
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        let result = wait_for_session(session.clone(), timeout_ms).await?;
        let output = collect_output(&session, context.max_output_chars).await;
        let meta = session.meta.lock().await.clone();
        Ok(json!({
            "terminal_id": meta.id,
            "process_id": meta.id,
            "terminal_name": derive_terminal_name(meta.cwd.as_str()),
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
    mut child: Child,
) -> Result<Arc<TerminalSession>, String> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let session_id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let session = Arc::new(TerminalSession {
        meta: Mutex::new(TerminalSessionMeta {
            id: session_id.clone(),
            cwd: target_path.to_string_lossy().to_string(),
            project_id: context.project_id.clone(),
            user_id: context.user_id.clone(),
            command,
            started_at: now.clone(),
            last_active_at: now.clone(),
            finished_at: None,
            status: "running".to_string(),
            exit_code: None,
        }),
        child: Mutex::new(child),
        logs: Mutex::new(Vec::new()),
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

fn spawn_stream_reader<R>(session: Arc<TerminalSession>, mut reader: R, kind: &'static str)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = vec![0_u8; 2048];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(count) => {
                    let chunk = String::from_utf8_lossy(&buf[..count]).to_string();
                    if !chunk.is_empty() {
                        append_log(session.clone(), kind, chunk).await;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

async fn append_log(session: Arc<TerminalSession>, kind: &str, content: String) {
    if content.is_empty() {
        return;
    }
    let now = now_rfc3339();
    {
        let mut logs = session.logs.lock().await;
        let offset = logs.last().map(|entry| entry.offset + 1).unwrap_or(0);
        logs.push(TerminalLogEntry {
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

async fn refresh_session_status(session: &Arc<TerminalSession>) -> Result<(), String> {
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
        mark_session_exited(session, status.code()).await;
    }
    Ok(())
}

async fn mark_session_exited(session: &Arc<TerminalSession>, exit_code: Option<i32>) {
    let mut meta = session.meta.lock().await;
    if meta.status == "exited" {
        return;
    }
    meta.status = "exited".to_string();
    meta.exit_code = exit_code;
    meta.finished_at = Some(now_rfc3339());
    meta.last_active_at = meta.finished_at.clone().unwrap_or_else(now_rfc3339);
}

async fn sessions_for_context(
    context: &TerminalControllerContext,
) -> Result<Vec<Arc<TerminalSession>>, String> {
    let root = canonicalize_existing(context.root.as_path())?;
    let sessions = terminal_state().sessions.read().await;
    let mut matched = Vec::new();
    for session in sessions.values() {
        let meta = session.meta.lock().await.clone();
        let same_user = match context.user_id.as_deref() {
            Some(user_id) => meta.user_id.as_deref() == Some(user_id),
            None => true,
        };
        let in_scope = if let Some(project_id) = context.project_id.as_deref() {
            meta.project_id.as_deref() == Some(project_id)
        } else {
            PathBuf::from(&meta.cwd).starts_with(&root)
        };
        if same_user && in_scope {
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
) -> Result<WaitResult, String> {
    let timeout = Duration::from_millis(timeout_ms.clamp(1_000, 600_000));
    let started = Instant::now();
    loop {
        refresh_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        if meta.status == "exited" {
            return Ok(WaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "exit",
                exit_code: meta.exit_code,
            });
        }
        if started.elapsed() >= timeout {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    let meta = session.meta.lock().await.clone();
    Ok(WaitResult {
        waited_ms: started.elapsed().as_millis() as u64,
        busy: meta.status != "exited",
        timed_out: true,
        finished_by: "timeout",
        exit_code: meta.exit_code,
    })
}

async fn collect_output(session: &Arc<TerminalSession>, max_chars: usize) -> OutputCapture {
    let logs = session.logs.lock().await;
    collect_output_from_logs(logs.iter().map(|entry| entry.content.as_str()), max_chars)
}

fn collect_output_from_logs<'a, I>(items: I, max_chars: usize) -> OutputCapture
where
    I: Iterator<Item = &'a str>,
{
    let full = items.collect::<Vec<_>>().join("");
    let char_count = full.chars().count();
    if char_count <= max_chars {
        return OutputCapture {
            text: full,
            char_count,
            truncated: false,
        };
    }
    let text = full
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect::<String>();
    OutputCapture {
        text,
        char_count,
        truncated: true,
    }
}

fn select_logs(logs: &[TerminalLogEntry], offset: Option<i64>, limit: usize) -> Vec<Value> {
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
    ordered.into_iter().map(log_to_value).collect()
}

fn take_recent_logs(logs: &[TerminalLogEntry], limit: usize) -> Vec<Value> {
    logs.iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(log_to_value)
        .collect()
}

fn log_to_value(entry: &TerminalLogEntry) -> Value {
    json!({
        "offset": entry.offset,
        "kind": entry.kind,
        "content": entry.content,
        "created_at": entry.created_at,
    })
}

fn log_value_content(value: &Value) -> Option<&str> {
    value.get("content").and_then(Value::as_str)
}

fn derive_terminal_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("terminal")
        .to_string()
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|err| err.to_string())
}

fn resolve_target_path(root: &Path, path_input: &str) -> Result<PathBuf, String> {
    let trimmed = path_input.trim();
    let joined = if trimmed.is_empty() || trimmed == "." {
        root.to_path_buf()
    } else {
        let path = PathBuf::from(trimmed);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    let canonical = canonicalize_existing(joined.as_path())?;
    if !canonical.starts_with(root) {
        return Err("target path escapes workspace root".to_string());
    }
    Ok(canonical)
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
