// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::types::McpStdioServer;

use super::{tools_list_stdio_cache_key, DEFAULT_MCP_RPC_TIMEOUT};
const MCP_STDIO_SESSION_MAX: usize = 32;
const MCP_STDIO_SESSION_IDLE_TTL: Duration = Duration::from_secs(10 * 60);
const MCP_STDIO_RESPONSE_LINE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
static MCP_STDIO_SESSIONS: OnceLock<Mutex<HashMap<String, StdioSessionEntry>>> = OnceLock::new();
static MCP_STDIO_SESSION_START_LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    OnceLock::new();

struct StdioRpcSession {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

#[derive(Clone)]
struct StdioSessionEntry {
    session: Arc<AsyncMutex<StdioRpcSession>>,
    created_at: Instant,
    last_used_at: Instant,
}

pub(super) fn stdio_session_cache_key(cfg: &McpStdioServer) -> String {
    format!(
        "stdio-session:name={}\n{}",
        cfg.name.trim(),
        tools_list_stdio_cache_key(cfg)
    )
}

pub async fn jsonrpc_stdio_call(
    cfg: &McpStdioServer,
    method: &str,
    params: Value,
    _conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session_key = stdio_session_cache_key(cfg);
    tokio::time::timeout(
        DEFAULT_MCP_RPC_TIMEOUT,
        jsonrpc_stdio_call_with_session(cfg, session_key.clone(), method, params),
    )
    .await
    .map_err(|_| {
        remove_stdio_session(session_key.as_str());
        format!(
            "{method} stdio MCP command `{}` timed out after {}s",
            cfg.command,
            DEFAULT_MCP_RPC_TIMEOUT.as_secs()
        )
    })?
}

async fn jsonrpc_stdio_call_with_session(
    cfg: &McpStdioServer,
    session_key: String,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});

    let session = get_stdio_session(cfg, session_key.as_str()).await?;
    let mut guard = session.lock().await;
    let result = guard.send_request(id.as_str(), &payload).await;
    if result.is_err() || guard.is_finished().await {
        drop(guard);
        remove_stdio_session(session_key.as_str());
    }
    result
}

async fn get_stdio_session(
    cfg: &McpStdioServer,
    session_key: &str,
) -> Result<Arc<AsyncMutex<StdioRpcSession>>, String> {
    if let Some(session) = lookup_stdio_session(session_key) {
        return Ok(session);
    }

    let start_lock = stdio_session_start_lock(session_key)?;
    let start_guard = start_lock.lock().await;
    let result = async {
        if let Some(session) = lookup_stdio_session(session_key) {
            return Ok(session);
        }

        ensure_stdio_session_capacity()?;
        let session = Arc::new(AsyncMutex::new(spawn_stdio_session(cfg).await?));
        insert_stdio_session(session_key, session)
    }
    .await;
    drop(start_guard);
    maybe_remove_stdio_session_start_lock(session_key, &start_lock);
    result
}

fn lookup_stdio_session(session_key: &str) -> Option<Arc<AsyncMutex<StdioRpcSession>>> {
    let now = Instant::now();
    let sessions = MCP_STDIO_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = sessions.lock().ok()?;
    prune_idle_stdio_sessions_locked(&mut guard, now);
    let entry = guard.get_mut(session_key)?;
    let idle_for = now.saturating_duration_since(entry.last_used_at);
    entry.last_used_at = now;
    debug!(
        session_key = %session_key,
        session_age_ms = entry.created_at.elapsed().as_millis(),
        idle_ms = idle_for.as_millis(),
        "reusing stdio MCP session"
    );
    Some(entry.session.clone())
}

fn insert_stdio_session(
    session_key: &str,
    session: Arc<AsyncMutex<StdioRpcSession>>,
) -> Result<Arc<AsyncMutex<StdioRpcSession>>, String> {
    let now = Instant::now();
    let sessions = MCP_STDIO_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = sessions.lock().map_err(|err| err.to_string())?;
    ensure_stdio_session_capacity_locked(&mut guard, now)?;
    info!(
        session_key = %session_key,
        active_sessions = guard.len() + 1,
        max_sessions = MCP_STDIO_SESSION_MAX,
        "spawned stdio MCP session"
    );
    guard.insert(
        session_key.to_string(),
        StdioSessionEntry {
            session: session.clone(),
            created_at: now,
            last_used_at: now,
        },
    );
    Ok(session)
}

fn ensure_stdio_session_capacity() -> Result<(), String> {
    let now = Instant::now();
    let sessions = MCP_STDIO_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = sessions.lock().map_err(|err| err.to_string())?;
    ensure_stdio_session_capacity_locked(&mut guard, now)
}

fn ensure_stdio_session_capacity_locked(
    guard: &mut HashMap<String, StdioSessionEntry>,
    now: Instant,
) -> Result<(), String> {
    prune_idle_stdio_sessions_locked(guard, now);
    while guard.len() >= MCP_STDIO_SESSION_MAX {
        let Some(evict_key) = least_recently_used_idle_stdio_session_key(guard) else {
            return Err(format!(
                "stdio MCP session pool exhausted: active_sessions={}, max_sessions={}",
                guard.len(),
                MCP_STDIO_SESSION_MAX
            ));
        };
        guard.remove(evict_key.as_str());
        info!(
            session_key = %evict_key,
            active_sessions = guard.len(),
            max_sessions = MCP_STDIO_SESSION_MAX,
            "evicted idle stdio MCP session for capacity"
        );
    }
    Ok(())
}

fn prune_idle_stdio_sessions_locked(
    guard: &mut HashMap<String, StdioSessionEntry>,
    now: Instant,
) -> usize {
    let before = guard.len();
    guard.retain(|session_key, entry| {
        let idle_for = now.saturating_duration_since(entry.last_used_at);
        let can_evict = Arc::strong_count(&entry.session) == 1;
        let keep = idle_for < MCP_STDIO_SESSION_IDLE_TTL || !can_evict;
        if !keep {
            info!(
                session_key = %session_key,
                idle_ms = idle_for.as_millis(),
                "evicted idle stdio MCP session"
            );
        }
        keep
    });
    before.saturating_sub(guard.len())
}

fn least_recently_used_idle_stdio_session_key(
    guard: &HashMap<String, StdioSessionEntry>,
) -> Option<String> {
    guard
        .iter()
        .filter(|(_, entry)| Arc::strong_count(&entry.session) == 1)
        .min_by_key(|(_, entry)| entry.last_used_at)
        .map(|(key, _)| key.clone())
}

pub(super) fn remove_stdio_session(session_key: &str) {
    let sessions = MCP_STDIO_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = sessions.lock() {
        if guard.remove(session_key).is_some() {
            warn!(
                session_key = %session_key,
                active_sessions = guard.len(),
                "removed stdio MCP session"
            );
        }
    }
}

fn stdio_session_start_lock(session_key: &str) -> Result<Arc<AsyncMutex<()>>, String> {
    let locks = MCP_STDIO_SESSION_START_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = locks.lock().map_err(|err| err.to_string())?;
    Ok(guard
        .entry(session_key.to_string())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone())
}

fn maybe_remove_stdio_session_start_lock(session_key: &str, start_lock: &Arc<AsyncMutex<()>>) {
    let locks = MCP_STDIO_SESSION_START_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = locks.lock() {
        let should_remove = guard
            .get(session_key)
            .map(|current| Arc::ptr_eq(current, start_lock) && Arc::strong_count(current) <= 2)
            .unwrap_or(false);
        if should_remove {
            guard.remove(session_key);
        }
    }
}

async fn spawn_stdio_session(cfg: &McpStdioServer) -> Result<StdioRpcSession, String> {
    let mut cmd = build_stdio_command(cfg)?;
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|err| err.to_string())?;
    let stdin = child.stdin.take().ok_or("missing stdin")?;
    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let reader = BufReader::new(stdout);

    Ok(StdioRpcSession {
        child,
        stdin,
        reader,
    })
}

fn build_stdio_command(cfg: &McpStdioServer) -> Result<tokio::process::Command, String> {
    let mut cmd = tokio::process::Command::new(&cfg.command);
    if let Some(args) = &cfg.args {
        cmd.args(args);
    }

    if let Some(env) = &cfg.env {
        cmd.envs(env);
    }
    if let Some(cwd) = &cfg.cwd {
        cmd.current_dir(cwd);
    }
    Ok(cmd)
}

impl StdioRpcSession {
    async fn send_request(&mut self, id: &str, payload: &Value) -> Result<Value, String> {
        let data = payload.to_string() + "\n";
        self.stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
        self.stdin.flush().await.map_err(|err| err.to_string())?;

        loop {
            match read_stdio_response_line_limited(
                &mut self.reader,
                MCP_STDIO_RESPONSE_LINE_LIMIT_BYTES,
            )
            .await
            {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        if value.get("id").and_then(Value::as_str) == Some(id) {
                            if value.get("error").is_some() {
                                return Err(value.to_string());
                            }
                            return Ok(value.get("result").cloned().unwrap_or(value));
                        }
                    }
                }
                Ok(None) => break,
                Err(err) => return Err(err),
            }
        }

        Err("no response from stdio server".to_string())
    }

    async fn is_finished(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

async fn read_stdio_response_line_limited<R>(
    reader: &mut R,
    limit_bytes: usize,
) -> Result<Option<String>, String>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|err| err.to_string())?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            break;
        }

        let take_len = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(available.len());
        let next_len = line.len().saturating_add(take_len);
        ensure_stdio_response_line_within_limit(next_len, limit_bytes)?;
        line.extend_from_slice(&available[..take_len]);
        reader.consume(take_len);
        if line.last().copied() == Some(b'\n') {
            break;
        }
    }

    while matches!(line.last().copied(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    String::from_utf8(line)
        .map(Some)
        .map_err(|err| format!("stdio MCP response was not UTF-8: {err}"))
}

pub(super) fn ensure_stdio_response_line_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "stdio MCP response line exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}
