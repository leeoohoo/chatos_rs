// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::types::{McpAsyncResultTransport, McpStdioServer};

const DEFAULT_MCP_RPC_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_TOOLS_LIST_SUCCESS_CACHE_TTL: Duration = Duration::from_secs(60);
const MCP_TOOLS_LIST_ERROR_CACHE_TTL: Duration = Duration::from_secs(10);
const MCP_HTTP_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MCP_HTTP_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;
static MCP_HTTP_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
static MCP_TOOLS_LIST_CACHE: OnceLock<Mutex<HashMap<String, ToolsListCacheEntry>>> =
    OnceLock::new();
mod internal_headers;
mod stdio;

pub use internal_headers::{headers_require_per_request_signing, prepare_http_headers};

#[cfg(test)]
use stdio::{ensure_stdio_response_line_within_limit, stdio_session_cache_key};
pub use stdio::{invalidate_stdio_session, jsonrpc_stdio_call, jsonrpc_stdio_call_with_timeout};
#[derive(Clone)]
struct ToolsListCacheEntry {
    expires_at: Instant,
    result: Result<Vec<Value>, String>,
}

pub async fn list_tools_http(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<Vec<Value>, String> {
    list_tools_http_with_client(url, headers, timeout, None).await
}

pub async fn list_tools_http_with_client(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    timeout: Option<Duration>,
    client: Option<&reqwest::Client>,
) -> Result<Vec<Value>, String> {
    let cache_key = tools_list_http_cache_key(url, headers, timeout);
    if let Some(cached) = cached_tools_list(cache_key.as_str()) {
        return cached;
    }
    let result = async {
        let response =
            jsonrpc_http_call_with_client(url, headers, "tools/list", json!({}), timeout, client)
                .await?;
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
    timeout: Option<Duration>,
) -> Result<Value, String> {
    jsonrpc_http_call_with_client(url, headers, method, params, timeout, None).await
}

pub async fn jsonrpc_http_call_with_client(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    method: &str,
    params: Value,
    timeout: Option<Duration>,
    client: Option<&reqwest::Client>,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    jsonrpc_http_call_with_id(url, headers, method, params, timeout, id.as_str(), client).await
}

pub async fn jsonrpc_http_tool_call_cancellable(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    params: Value,
    timeout: Option<Duration>,
    async_result_transport: McpAsyncResultTransport,
) -> Result<Value, String> {
    jsonrpc_http_tool_call_cancellable_with_client(
        url,
        headers,
        params,
        timeout,
        async_result_transport,
        None,
    )
    .await
}

pub async fn jsonrpc_http_tool_call_cancellable_with_client(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    params: Value,
    timeout: Option<Duration>,
    async_result_transport: McpAsyncResultTransport,
    client: Option<&reqwest::Client>,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    if async_result_transport == McpAsyncResultTransport::RabbitMq {
        return Err(
            "RabbitMQ MCP tools must use the unified tool call command channel".to_string(),
        );
    }
    let mut cancellation_guard =
        HttpCancellationGuard::new(url, headers, id.as_str(), timeout, client.cloned());
    let request_timeout = timeout.unwrap_or(DEFAULT_MCP_RPC_TIMEOUT);
    let result = match jsonrpc_http_call_with_id(
        url,
        headers,
        "tools/call",
        params,
        Some(request_timeout),
        id.as_str(),
        client,
    )
    .await
    {
        Ok(response) => Ok(response),
        Err(error) => Err(error),
    };
    cancellation_guard.disarm();
    result
}

async fn jsonrpc_http_call_with_id(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    method: &str,
    params: Value,
    timeout: Option<Duration>,
    id: &str,
    client: Option<&reqwest::Client>,
) -> Result<Value, String> {
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let default_client;
    let client = match client {
        Some(client) => client,
        None => {
            default_client = mcp_http_client()?;
            &default_client
        }
    };
    let request_timeout = timeout.unwrap_or(DEFAULT_MCP_RPC_TIMEOUT);
    let mut request = client.post(url).timeout(request_timeout).json(&payload);
    if let Some(headers) = headers {
        for (key, value) in prepare_http_headers(headers)? {
            request = request.header(key.as_str(), value.as_str());
        }
    }
    let response = request
        .send()
        .await
        .map_err(|err| format_http_send_error(method, url, request_timeout, &err))?;

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

struct HttpCancellationGuard {
    url: String,
    headers: Option<HashMap<String, String>>,
    request_id: String,
    timeout: Duration,
    client: Option<reqwest::Client>,
    armed: bool,
}

impl HttpCancellationGuard {
    fn new(
        url: &str,
        headers: Option<&HashMap<String, String>>,
        request_id: &str,
        timeout: Option<Duration>,
        client: Option<reqwest::Client>,
    ) -> Self {
        Self {
            url: url.to_string(),
            headers: headers.cloned(),
            request_id: request_id.to_string(),
            timeout: timeout
                .unwrap_or(DEFAULT_MCP_RPC_TIMEOUT)
                .min(Duration::from_secs(5)),
            client,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for HttpCancellationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let url = self.url.clone();
        let headers = self.headers.clone();
        let request_id = self.request_id.clone();
        let timeout = self.timeout;
        let client = self.client.clone();
        runtime.spawn(async move {
            if let Err(error) = send_http_cancel_notification(
                url.as_str(),
                headers.as_ref(),
                request_id.as_str(),
                timeout,
                client.as_ref(),
            )
            .await
            {
                tracing::debug!(
                    request_id = request_id.as_str(),
                    error = error.as_str(),
                    "MCP cancellation notification was not acknowledged"
                );
            }
        });
    }
}

async fn send_http_cancel_notification(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    request_id: &str,
    timeout: Duration,
    client: Option<&reqwest::Client>,
) -> Result<(), String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {
            "requestId": request_id,
            "reason": "agent runtime aborted the tool call"
        }
    });
    let default_client;
    let client = match client {
        Some(client) => client,
        None => {
            default_client = mcp_http_client()?;
            &default_client
        }
    };
    let mut request = client.post(url).timeout(timeout).json(&payload);
    if let Some(headers) = headers {
        for (key, value) in prepare_http_headers(headers)? {
            request = request.header(key.as_str(), value.as_str());
        }
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("send MCP cancellation notification failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "MCP cancellation notification returned HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok(())
}

fn mcp_http_client() -> Result<reqwest::Client, String> {
    MCP_HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
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

fn tools_list_http_cache_key(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    timeout: Option<Duration>,
) -> String {
    let mut parts = vec![format!("http:url={}", url.trim())];
    if let Some(timeout) = timeout {
        parts.push(format!("timeout_ms={}", timeout.as_millis()));
    }
    if let Some(headers) = headers {
        let mut entries = headers.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in entries {
            parts.push(format!(
                "header:{}:sha256={}",
                key.trim(),
                hex::encode(Sha256::digest(value.as_bytes()))
            ));
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
            parts.push(format!(
                "env:{}:sha256={}",
                key.trim(),
                hex::encode(Sha256::digest(value.as_bytes()))
            ));
        }
    }
    parts.join("\n")
}

fn format_http_send_error(
    method: &str,
    url: &str,
    timeout: Duration,
    err: &reqwest::Error,
) -> String {
    format!(
        "{method} {url} failed before HTTP response: {}; timeout={}s; source={}",
        classify_http_send_error(err),
        timeout.as_secs(),
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

#[cfg(test)]
mod tests;
