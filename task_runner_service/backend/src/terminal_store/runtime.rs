use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use chatos_builtin_tools::TerminalControllerContext;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

use super::pathing::{canonicalize_existing, now_rfc3339};
use super::{TerminalLogEntry, TerminalRuntimeState, TerminalSession, TerminalSessionMeta};

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
        meta: tokio::sync::Mutex::new(TerminalSessionMeta {
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
        child: tokio::sync::Mutex::new(child),
        logs: tokio::sync::Mutex::new(Vec::new()),
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

pub(super) async fn append_log(session: Arc<TerminalSession>, kind: &str, content: String) {
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

pub(super) async fn refresh_session_status(session: &Arc<TerminalSession>) -> Result<(), String> {
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

pub(super) async fn mark_session_exited(session: &Arc<TerminalSession>, exit_code: Option<i32>) {
    let mut meta = session.meta.lock().await;
    if meta.status == "exited" {
        return;
    }
    meta.status = "exited".to_string();
    meta.exit_code = exit_code;
    meta.finished_at = Some(now_rfc3339());
    meta.last_active_at = meta.finished_at.clone().unwrap_or_else(now_rfc3339);
}

pub(super) async fn sessions_for_context(
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

pub(super) struct WaitResult {
    pub(super) waited_ms: u64,
    pub(super) busy: bool,
    pub(super) timed_out: bool,
    pub(super) finished_by: &'static str,
    pub(super) exit_code: Option<i32>,
}

pub(super) async fn wait_for_session(
    session: Arc<TerminalSession>,
    timeout_ms: u64,
) -> Result<WaitResult, String> {
    let timeout = Duration::from_millis(timeout_ms.max(1_000).min(600_000));
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
