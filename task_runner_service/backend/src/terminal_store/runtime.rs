// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use chatos_mcp::TerminalControllerContext;
use chatos_terminal_runtime::{read_output_chunks, wait_for_terminal_session, TerminalWaitResult};
use tokio::io::AsyncRead;
use tokio::process::Child;
use uuid::Uuid;

use super::pathing::{canonicalize_existing, now_rfc3339};
use super::{TerminalRuntimeState, TerminalSession, TerminalSessionMeta};

static TERMINAL_STATE: OnceLock<Arc<TerminalRuntimeState>> = OnceLock::new();

pub(super) fn terminal_state() -> &'static Arc<TerminalRuntimeState> {
    TERMINAL_STATE.get_or_init(|| Arc::new(TerminalRuntimeState::default()))
}

pub(super) async fn register_session(
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
        meta: tokio::sync::Mutex::new(TerminalSessionMeta::new(
            session_id.clone(),
            target_path.to_string_lossy().to_string(),
            context.project_id.clone(),
            context.user_id.clone(),
            command,
            now,
        )),
        child: tokio::sync::Mutex::new(child),
        logs: tokio::sync::Mutex::new(chatos_terminal_runtime::TerminalLogBuffer::default()),
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
        let _ = read_output_chunks(&mut reader, move |chunk| {
            let session = session.clone();
            async move {
                append_log(session, kind, chunk).await;
            }
        })
        .await;
    });
}

pub(super) async fn append_log(session: Arc<TerminalSession>, kind: &str, content: String) {
    if content.is_empty() {
        return;
    }
    let now = now_rfc3339();
    {
        let mut logs = session.logs.lock().await;
        logs.append(kind, content, now.clone());
    }
    let mut meta = session.meta.lock().await;
    meta.record_activity(now);
}

pub(super) async fn refresh_session_status(session: &Arc<TerminalSession>) -> Result<(), String> {
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

pub(super) async fn mark_session_exited(session: &Arc<TerminalSession>, exit_code: Option<i32>) {
    let mut meta = session.meta.lock().await;
    meta.mark_exited(exit_code, now_rfc3339());
}

pub(super) async fn sessions_for_context(
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
        let left_fut = left.meta.try_lock();
        let right_fut = right.meta.try_lock();
        match (left_fut, right_fut) {
            (Ok(left), Ok(right)) => right.last_active_at.cmp(&left.last_active_at),
            _ => std::cmp::Ordering::Equal,
        }
    });
    Ok(matched)
}

pub(super) async fn session_for_context(
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

pub(super) async fn wait_for_session(
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
