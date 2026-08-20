// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chatos_mcp_runtime::{jsonrpc_http_call, jsonrpc_stdio_call};
use reqwest::header::{HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::local_runtime::LocalDatabase;
use crate::relay::RelayRequest;

use super::configs::{stdio_server_for_manifest, validate_loopback_http_url};
use super::manifest::LocalMcpTransport;
use super::repository::load_execution_manifest;

const MANIFEST_HEADER: &str = "x-local-connector-mcp-manifest-id";
const INLINE_HTTP_RUNTIME_HEADER: &str = "x-local-connector-inline-mcp-runtime";
const RESOURCE_HEADER: &str = "x-plugin-management-resource-id";
const MAX_INLINE_HEADERS: usize = 64;
const MAX_INLINE_HEADER_BYTES: usize = 32 * 1024;

#[derive(Debug, Deserialize)]
struct InlineHttpRuntime {
    url: String,
    #[serde(default)]
    headers: std::collections::BTreeMap<String, String>,
    timeout_ms: u64,
}

pub(crate) fn is_user_mcp_request(request: &RelayRequest) -> bool {
    header_text(request, MANIFEST_HEADER).is_some()
        || header_text(request, INLINE_HTTP_RUNTIME_HEADER).is_some()
}

pub(crate) async fn handle_user_mcp_body(
    request: &RelayRequest,
    database: &LocalDatabase,
) -> Result<Value> {
    let owner_user_id = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Local Connector MCP owner user id is required"))?;
    let device_id = request
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Local Connector MCP device id is required"))?;
    let plugin_mcp_id = required_header(request, RESOURCE_HEADER)?;
    let method = request
        .body
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if !matches!(method, "tools/list" | "tools/call") {
        return Err(anyhow!("unsupported user MCP JSON-RPC method: {method}"));
    }
    let params = request
        .body
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let manifest_id = header_text(request, MANIFEST_HEADER);
    let inline_http = header_text(request, INLINE_HTTP_RUNTIME_HEADER);
    if manifest_id.is_some() == inline_http.is_some() {
        return Err(anyhow!(
            "Local Connector MCP request must contain exactly one runtime binding"
        ));
    }
    let result = if let Some(manifest_id) = manifest_id {
        let manifest = load_execution_manifest(
            database,
            owner_user_id,
            device_id,
            manifest_id,
            plugin_mcp_id,
        )
        .await?;
        match manifest.transport {
            LocalMcpTransport::Stdio => {
                let server = stdio_server_for_manifest(&manifest)?;
                jsonrpc_stdio_call(&server, method, params, None)
                    .await
                    .map_err(anyhow::Error::msg)?
            }
            LocalMcpTransport::Http => {
                let config = manifest
                    .http
                    .as_ref()
                    .ok_or_else(|| anyhow!("local HTTP MCP config is missing"))?;
                validate_loopback_http_url(config.url.as_str())?;
                let headers = config
                    .headers
                    .clone()
                    .into_iter()
                    .collect::<HashMap<_, _>>();
                call_http_runtime(
                    config.url.as_str(),
                    &headers,
                    config.timeout_ms,
                    method,
                    params,
                )
                .await?
            }
        }
    } else {
        let inline_http = decode_inline_http_runtime(inline_http.unwrap())?;
        call_http_runtime(
            inline_http.url.as_str(),
            &inline_http.headers.into_iter().collect(),
            inline_http.timeout_ms,
            method,
            params,
        )
        .await?
    };
    Ok(json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or(Value::Null),
        "result": result,
    }))
}

async fn call_http_runtime(
    url: &str,
    headers: &HashMap<String, String>,
    timeout_ms: u64,
    method: &str,
    params: Value,
) -> Result<Value> {
    jsonrpc_http_call(
        url,
        Some(headers),
        method,
        params,
        Some(Duration::from_millis(timeout_ms.clamp(300, 120_000))),
    )
    .await
    .map_err(anyhow::Error::msg)
}

fn decode_inline_http_runtime(value: &str) -> Result<InlineHttpRuntime> {
    let decoded = urlencoding::decode(value)
        .map_err(|_| anyhow!("inline HTTP MCP runtime encoding is invalid"))?;
    let runtime = serde_json::from_str::<InlineHttpRuntime>(decoded.as_ref())
        .map_err(|_| anyhow!("inline HTTP MCP runtime is invalid"))?;
    validate_inline_http_runtime(&runtime)?;
    Ok(runtime)
}

fn validate_inline_http_runtime(runtime: &InlineHttpRuntime) -> Result<()> {
    let url = reqwest::Url::parse(runtime.url.trim())
        .map_err(|_| anyhow!("inline HTTP MCP URL is invalid"))?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(anyhow!(
            "inline HTTP MCP URL cannot contain credentials or a fragment"
        ));
    }
    let allowed_url = match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .ok()
                    .is_some_and(|ip| ip.is_loopback())
        }),
        _ => false,
    };
    if !allowed_url {
        return Err(anyhow!(
            "inline HTTP MCP must use HTTPS, or HTTP with a loopback host"
        ));
    }
    if runtime.headers.len() > MAX_INLINE_HEADERS {
        return Err(anyhow!("inline HTTP MCP has too many headers"));
    }
    let header_bytes = runtime
        .headers
        .iter()
        .map(|(name, value)| name.len().saturating_add(value.len()))
        .sum::<usize>();
    if header_bytes > MAX_INLINE_HEADER_BYTES {
        return Err(anyhow!("inline HTTP MCP headers are too large"));
    }
    for (name, value) in &runtime.headers {
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| anyhow!("inline HTTP MCP contains an invalid header name"))?;
        HeaderValue::from_str(value)
            .map_err(|_| anyhow!("inline HTTP MCP contains an invalid header value"))?;
        if managed_or_unsafe_header(name.as_str()) {
            return Err(anyhow!(
                "inline HTTP MCP contains a managed header: {}",
                name.as_str()
            ));
        }
    }
    Ok(())
}

fn managed_or_unsafe_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | INLINE_HTTP_RUNTIME_HEADER
            | MANIFEST_HEADER
            | RESOURCE_HEADER
    )
}

fn required_header<'a>(request: &'a RelayRequest, name: &str) -> Result<&'a str> {
    header_text(request, name)
        .ok_or_else(|| anyhow!("required MCP relay header is missing: {name}"))
}

fn header_text<'a>(request: &'a RelayRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "user_runtime_tests.rs"]
mod tests;
