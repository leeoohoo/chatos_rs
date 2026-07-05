// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::process_isolation;
use crate::types::McpStdioServer;

const MCP_RPC_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_TOOLS_LIST_SUCCESS_CACHE_TTL: Duration = Duration::from_secs(60);
const MCP_TOOLS_LIST_ERROR_CACHE_TTL: Duration = Duration::from_secs(10);
const MCP_STDIO_SESSION_MAX: usize = 32;
const MCP_STDIO_SESSION_IDLE_TTL: Duration = Duration::from_secs(10 * 60);
const MCP_HTTP_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MCP_HTTP_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;
const MCP_STDIO_RESPONSE_LINE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
static MCP_HTTP_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
static MCP_TOOLS_LIST_CACHE: OnceLock<Mutex<HashMap<String, ToolsListCacheEntry>>> =
    OnceLock::new();
static MCP_STDIO_SESSIONS: OnceLock<Mutex<HashMap<String, StdioSessionEntry>>> = OnceLock::new();
static MCP_STDIO_SESSION_START_LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    OnceLock::new();

#[derive(Clone)]
struct ToolsListCacheEntry {
    expires_at: Instant,
    result: Result<Vec<Value>, String>,
}

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

pub async fn list_tools_http(
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Vec<Value>, String> {
    let cache_key = tools_list_http_cache_key(url, headers);
    if let Some(cached) = cached_tools_list(cache_key.as_str()) {
        return cached;
    }
    let result = async {
        let response = jsonrpc_http_call(url, headers, "tools/list", json!({})).await?;
        extract_tools(&response)
    }
    .await;
    store_tools_list_cache(cache_key, result.clone());
    result
}

pub async fn list_tools_stdio(cfg: &McpStdioServer) -> Result<Vec<Value>, String> {
    let cache_key = tools_list_stdio_cache_key(cfg);
    if let Some(cached) = cached_tools_list(cache_key.as_str()) {
        return cached;
    }
    let result = async {
        let response = jsonrpc_stdio_call(cfg, "tools/list", json!({}), None).await?;
        extract_tools(&response)
    }
    .await;
    store_tools_list_cache(cache_key, result.clone());
    result
}

pub fn extract_tools(response: &Value) -> Result<Vec<Value>, String> {
    response
        .get("tools")
        .or_else(|| {
            response
                .get("result")
                .and_then(|result| result.get("tools"))
        })
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "tools not found in response".to_string())
}

pub async fn jsonrpc_http_call(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let client = mcp_http_client()?;
    let mut request = client.post(url).json(&payload);
    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key.as_str(), value.as_str());
        }
    }
    let response = request
        .send()
        .await
        .map_err(|err| format_http_send_error(method, url, &err))?;

    let status = response.status();
    let redirect_location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    if !status.is_success() {
        let body = read_http_response_body_limited(response, MCP_HTTP_ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|body| String::from_utf8_lossy(body.as_slice()).into_owned())
            .unwrap_or_else(|err| err);
        let location_suffix = redirect_location
            .as_deref()
            .map(|location| format!("; location={location}"))
            .unwrap_or_default();
        return Err(format!(
            "{method} {url} failed after HTTP response: 外部 MCP 返回 HTTP {status}{location_suffix}; body={}",
            response_preview(body.as_str())
        ));
    }
    let body = read_http_response_body_limited(response, MCP_HTTP_RESPONSE_LIMIT_BYTES)
        .await
        .map_err(|err| format!("{method} {url} failed after HTTP response: {err}"))?;
    let value: Value = serde_json::from_slice(body.as_slice()).map_err(|err| {
        let body_text = String::from_utf8_lossy(body.as_slice());
        format!(
            "{method} {url} failed after HTTP response: 外部 MCP 返回的不是 JSON: {err}; body={}",
            response_preview(body_text.as_ref())
        )
    })?;
    if value.get("error").is_some() {
        return Err(format!(
            "{method} {url} returned JSON-RPC error: {}",
            response_preview(value.to_string().as_str())
        ));
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

fn mcp_http_client() -> Result<reqwest::Client, String> {
    MCP_HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(MCP_RPC_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|err| err.to_string())
        })
        .clone()
}

async fn read_http_response_body_limited(
    mut response: reqwest::Response,
    limit_bytes: usize,
) -> Result<Vec<u8>, String> {
    if let Some(content_length) = response.content_length() {
        ensure_http_response_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|err| err.to_string())? {
        let next_len = body.len().saturating_add(chunk.len());
        ensure_http_response_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

fn ensure_http_response_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "MCP HTTP response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

fn cached_tools_list(cache_key: &str) -> Option<Result<Vec<Value>, String>> {
    let cache = MCP_TOOLS_LIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().ok()?;
    let entry = guard.get(cache_key)?;
    if Instant::now() < entry.expires_at {
        return Some(entry.result.clone());
    }
    guard.remove(cache_key);
    None
}

fn store_tools_list_cache(cache_key: String, result: Result<Vec<Value>, String>) {
    let ttl = if result.is_ok() {
        MCP_TOOLS_LIST_SUCCESS_CACHE_TTL
    } else {
        MCP_TOOLS_LIST_ERROR_CACHE_TTL
    };
    let Some(expires_at) = Instant::now().checked_add(ttl) else {
        return;
    };
    let cache = MCP_TOOLS_LIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(cache_key, ToolsListCacheEntry { expires_at, result });
    }
}

fn tools_list_http_cache_key(url: &str, headers: Option<&HashMap<String, String>>) -> String {
    let mut parts = vec![format!("http:url={}", url.trim())];
    if let Some(headers) = headers {
        let mut entries = headers.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in entries {
            parts.push(format!("header:{}={}", key.trim(), value.trim()));
        }
    }
    parts.join("\n")
}

fn tools_list_stdio_cache_key(cfg: &McpStdioServer) -> String {
    let mut parts = vec![format!("stdio:command={}", cfg.command.trim())];
    if let Some(user_id) = &cfg.user_id {
        parts.push(format!("user_id={}", user_id.trim()));
    }
    if let Some(args) = &cfg.args {
        for arg in args {
            parts.push(format!("arg={arg}"));
        }
    }
    if let Some(cwd) = &cfg.cwd {
        parts.push(format!("cwd={}", cwd.trim()));
    }
    if let Some(env) = &cfg.env {
        let mut entries = env.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in entries {
            parts.push(format!("env:{}={}", key.trim(), value.trim()));
        }
    }
    parts.join("\n")
}

fn stdio_session_cache_key(cfg: &McpStdioServer) -> String {
    format!(
        "stdio-session:name={}\n{}",
        cfg.name.trim(),
        tools_list_stdio_cache_key(cfg)
    )
}

fn format_http_send_error(method: &str, url: &str, err: &reqwest::Error) -> String {
    format!(
        "{method} {url} failed before HTTP response: {}; timeout={}s; source={}",
        classify_http_send_error(err),
        MCP_RPC_TIMEOUT.as_secs(),
        error_chain(err)
    )
}

fn classify_http_send_error(err: &reqwest::Error) -> &'static str {
    let chain = error_chain(err).to_ascii_lowercase();
    if err.is_timeout()
        || chain.contains("timed out")
        || chain.contains("operation timed out")
        || chain.contains("deadline has elapsed")
    {
        return "请求超时，外部 MCP 没有在超时时间内返回 HTTP 响应";
    }
    if chain.contains("dns")
        || chain.contains("failed to lookup address information")
        || chain.contains("name or service not known")
        || chain.contains("no address associated with hostname")
    {
        return "DNS 解析失败，外部 MCP 域名无法解析";
    }
    if chain.contains("connection refused") {
        return "连接被拒绝，目标主机可达但端口未监听或被防火墙拒绝";
    }
    if chain.contains("network is unreachable") || chain.contains("no route to host") {
        return "网络不可达，本机到外部 MCP 地址没有可用路由";
    }
    if chain.contains("connection reset") {
        return "连接被重置，外部 MCP 或中间网关主动断开连接";
    }
    if chain.contains("certificate")
        || chain.contains("tls")
        || chain.contains("ssl")
        || chain.contains("invalid peer certificate")
    {
        return "TLS/证书握手失败，外部 MCP 的 HTTPS 证书或 TLS 链路不可用";
    }
    if err.is_connect() {
        return "网络连接失败，未能连接到外部 MCP 服务";
    }
    if err.is_request() {
        return "请求发送失败，请求参数或 URL 可能无效";
    }
    if err.is_body() {
        return "请求体发送失败，连接在上传请求时中断";
    }
    "网络请求失败，未收到外部 MCP 的 HTTP 响应"
}

fn error_chain(err: &reqwest::Error) -> String {
    let mut messages = vec![err.to_string()];
    let mut source = err.source();
    while let Some(item) = source {
        messages.push(item.to_string());
        source = item.source();
    }
    messages.join(" | caused by: ")
}

fn response_preview(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    if trimmed.chars().count() <= 300 {
        return trimmed.to_string();
    }
    let preview = trimmed.chars().take(300).collect::<String>();
    format!("{preview}... [truncated]")
}

pub async fn jsonrpc_stdio_call(
    cfg: &McpStdioServer,
    method: &str,
    params: Value,
    _conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session_key = stdio_session_cache_key(cfg);
    tokio::time::timeout(
        MCP_RPC_TIMEOUT,
        jsonrpc_stdio_call_with_session(cfg, session_key.clone(), method, params),
    )
    .await
    .map_err(|_| {
        remove_stdio_session(session_key.as_str());
        format!(
            "{method} stdio MCP command `{}` timed out after {}s",
            cfg.command,
            MCP_RPC_TIMEOUT.as_secs()
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

fn remove_stdio_session(session_key: &str) {
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
    let isolation =
        process_isolation::resolve_required_filesystem_view_for_user(cfg.user_id.as_deref())?;
    let fs_view_enabled = process_isolation::filesystem_view_enabled(isolation.as_ref())?;
    let cwd = cfg.cwd.as_deref().map(Path::new);
    if let Some(cwd) = cwd {
        process_isolation::prepare_workspace_for_user(cwd, isolation.as_ref())?;
    }

    let mut cmd = if isolation.is_some() {
        let (command, mut args) = process_isolation::terminal_helper_command(
            &cfg.command,
            isolation.as_ref(),
            cwd,
            None,
        )?;
        if let Some(config_args) = &cfg.args {
            args.extend(config_args.iter().map(OsString::from));
        }
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        cmd
    } else {
        let mut cmd = tokio::process::Command::new(&cfg.command);
        if let Some(args) = &cfg.args {
            cmd.args(args);
        }
        cmd
    };

    if let Some(env) = &cfg.env {
        cmd.envs(env);
    }
    if let Some(cwd) = &cfg.cwd {
        if isolation.is_none() || !fs_view_enabled {
            cmd.current_dir(cwd);
        }
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

fn ensure_stdio_response_line_within_limit(
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::*;

    #[test]
    fn http_response_body_limit_accepts_boundary_size() {
        assert!(ensure_http_response_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn http_response_body_limit_rejects_oversized_body() {
        let err = ensure_http_response_body_within_limit(1025, 1024)
            .expect_err("oversized body should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }

    #[test]
    fn stdio_response_line_limit_accepts_boundary_size() {
        assert!(ensure_stdio_response_line_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn stdio_response_line_limit_rejects_oversized_line() {
        let err = ensure_stdio_response_line_within_limit(1025, 1024)
            .expect_err("oversized line should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }

    #[test]
    fn http_tools_list_cache_key_sorts_headers() {
        let headers_a = HashMap::from([
            ("X-Zed".to_string(), "last".to_string()),
            ("X-Alpha".to_string(), "first".to_string()),
        ]);
        let headers_b = HashMap::from([
            ("X-Alpha".to_string(), "first".to_string()),
            ("X-Zed".to_string(), "last".to_string()),
        ]);

        assert_eq!(
            tools_list_http_cache_key("https://example.test/mcp", Some(&headers_a)),
            tools_list_http_cache_key("https://example.test/mcp", Some(&headers_b))
        );
    }

    #[test]
    fn stdio_tools_list_cache_key_includes_config_shape() {
        let base = McpStdioServer {
            name: "demo".to_string(),
            command: "node".to_string(),
            args: Some(vec!["server.js".to_string()]),
            cwd: Some("/workspace".to_string()),
            env: Some(HashMap::from([("TOKEN".to_string(), "one".to_string())])),
            user_id: None,
        };
        let mut changed = base.clone();
        changed.args = Some(vec!["other.js".to_string()]);

        assert_ne!(
            tools_list_stdio_cache_key(&base),
            tools_list_stdio_cache_key(&changed)
        );
    }

    #[test]
    fn stdio_tools_list_cache_key_includes_user_id() {
        let mut first = McpStdioServer {
            name: "demo".to_string(),
            command: "node".to_string(),
            args: Some(vec!["server.js".to_string()]),
            cwd: Some("/workspace".to_string()),
            env: None,
            user_id: Some("user-a".to_string()),
        };
        let mut second = first.clone();
        second.user_id = Some("user-b".to_string());

        assert_ne!(
            tools_list_stdio_cache_key(&first),
            tools_list_stdio_cache_key(&second)
        );

        first.user_id = Some("user-b".to_string());
        assert_eq!(
            tools_list_stdio_cache_key(&first),
            tools_list_stdio_cache_key(&second)
        );
    }

    #[test]
    fn stdio_session_cache_key_includes_server_name() {
        let mut first = McpStdioServer {
            name: "alpha".to_string(),
            command: "node".to_string(),
            args: Some(vec!["server.js".to_string()]),
            cwd: Some("/workspace".to_string()),
            env: None,
            user_id: None,
        };
        let mut second = first.clone();
        second.name = "beta".to_string();

        assert_ne!(
            stdio_session_cache_key(&first),
            stdio_session_cache_key(&second)
        );

        first.name = "beta".to_string();
        assert_eq!(
            stdio_session_cache_key(&first),
            stdio_session_cache_key(&second)
        );
    }

    #[test]
    fn tools_list_cache_returns_fresh_entries_and_drops_expired_entries() {
        let key = format!("test-cache-key-{}", uuid::Uuid::new_v4());
        let result = Ok(vec![json!({"name": "demo_tool"})]);
        store_tools_list_cache(key.clone(), result.clone());
        assert_eq!(cached_tools_list(key.as_str()), Some(result));

        let cache = MCP_TOOLS_LIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = cache.lock().expect("cache lock");
        guard.insert(
            key.clone(),
            ToolsListCacheEntry {
                expires_at: Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("expired instant"),
                result: Ok(vec![json!({"name": "expired_tool"})]),
            },
        );
        drop(guard);

        assert!(cached_tools_list(key.as_str()).is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdio_jsonrpc_reuses_session_for_same_config() {
        let count_file = std::env::temp_dir().join(format!(
            "chatos_mcp_stdio_session_count_{}",
            uuid::Uuid::new_v4()
        ));
        let script = r#"
count=$(cat "$COUNT_FILE" 2>/dev/null || echo 0)
echo $((count + 1)) > "$COUNT_FILE"
while IFS= read -r line; do
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  printf '{"jsonrpc":"2.0","id":"%s","result":{"ok":true}}\n' "$id"
done
"#;
        let cfg = McpStdioServer {
            name: format!("session-reuse-{}", uuid::Uuid::new_v4()),
            command: "sh".to_string(),
            args: Some(vec!["-c".to_string(), script.to_string()]),
            cwd: None,
            env: Some(HashMap::from([(
                "COUNT_FILE".to_string(),
                count_file.to_string_lossy().to_string(),
            )])),
            user_id: None,
        };

        let first = jsonrpc_stdio_call(&cfg, "demo/one", json!({}), None)
            .await
            .expect("first stdio response");
        let second = jsonrpc_stdio_call(&cfg, "demo/two", json!({}), None)
            .await
            .expect("second stdio response");
        assert_eq!(first.pointer("/ok"), Some(&Value::Bool(true)));
        assert_eq!(second.pointer("/ok"), Some(&Value::Bool(true)));
        assert_eq!(
            std::fs::read_to_string(&count_file)
                .expect("count file")
                .trim(),
            "1"
        );

        remove_stdio_session(stdio_session_cache_key(&cfg).as_str());
        let _ = std::fs::remove_file(count_file);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdio_jsonrpc_deduplicates_concurrent_cold_start() {
        let count_file = std::env::temp_dir().join(format!(
            "chatos_mcp_stdio_cold_start_count_{}",
            uuid::Uuid::new_v4()
        ));
        let script = r#"
count=$(cat "$COUNT_FILE" 2>/dev/null || echo 0)
echo $((count + 1)) > "$COUNT_FILE"
while IFS= read -r line; do
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  printf '{"jsonrpc":"2.0","id":"%s","result":{"ok":true}}\n' "$id"
done
"#;
        let cfg = McpStdioServer {
            name: format!("cold-start-{}", uuid::Uuid::new_v4()),
            command: "sh".to_string(),
            args: Some(vec!["-c".to_string(), script.to_string()]),
            cwd: None,
            env: Some(HashMap::from([(
                "COUNT_FILE".to_string(),
                count_file.to_string_lossy().to_string(),
            )])),
            user_id: None,
        };

        let mut handles = Vec::new();
        for index in 0..8 {
            let cfg = cfg.clone();
            handles.push(tokio::spawn(async move {
                jsonrpc_stdio_call(&cfg, "demo/concurrent", json!({ "index": index }), None).await
            }));
        }

        for handle in handles {
            let value = handle.await.expect("join stdio request").expect("response");
            assert_eq!(value.pointer("/ok"), Some(&Value::Bool(true)));
        }

        assert_eq!(
            std::fs::read_to_string(&count_file)
                .expect("count file")
                .trim(),
            "1"
        );

        remove_stdio_session(stdio_session_cache_key(&cfg).as_str());
        let _ = std::fs::remove_file(count_file);
    }
}
