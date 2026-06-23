use std::collections::HashMap;
use std::error::Error as StdError;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::types::McpStdioServer;

const MCP_RPC_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn list_tools_http(
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Vec<Value>, String> {
    let response = jsonrpc_http_call(url, headers, "tools/list", json!({})).await?;
    extract_tools(&response)
}

pub async fn list_tools_stdio(cfg: &McpStdioServer) -> Result<Vec<Value>, String> {
    let response = jsonrpc_stdio_call(cfg, "tools/list", json!({}), None).await?;
    extract_tools(&response)
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
    let client = reqwest::Client::builder()
        .timeout(MCP_RPC_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| err.to_string())?;
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
    let body = response
        .text()
        .await
        .map_err(|err| format!("{method} {url} failed to read response body: {err}"))?;
    if !status.is_success() {
        let location_suffix = redirect_location
            .as_deref()
            .map(|location| format!("; location={location}"))
            .unwrap_or_default();
        return Err(format!(
            "{method} {url} failed after HTTP response: 外部 MCP 返回 HTTP {status}{location_suffix}; body={}",
            response_preview(body.as_str())
        ));
    }
    let value: Value = serde_json::from_str(body.as_str()).map_err(|err| {
        format!(
            "{method} {url} failed after HTTP response: 外部 MCP 返回的不是 JSON: {err}; body={}",
            response_preview(body.as_str())
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
    tokio::time::timeout(
        MCP_RPC_TIMEOUT,
        jsonrpc_stdio_call_inner(cfg, method, params),
    )
    .await
    .map_err(|_| {
        format!(
            "{method} stdio MCP command `{}` timed out after {}s",
            cfg.command,
            MCP_RPC_TIMEOUT.as_secs()
        )
    })?
}

async fn jsonrpc_stdio_call_inner(
    cfg: &McpStdioServer,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});

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
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|err| err.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let data = payload.to_string() + "\n";
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
    }

    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut reader = BufReader::new(stdout).lines();

    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("id").and_then(Value::as_str) == Some(id.as_str()) {
                        if value.get("error").is_some() {
                            return Err(value.to_string());
                        }
                        return Ok(value.get("result").cloned().unwrap_or(value));
                    }
                }
            }
            Ok(None) => break,
            Err(err) => return Err(err.to_string()),
        }
    }

    Err("no response from stdio server".to_string())
}
