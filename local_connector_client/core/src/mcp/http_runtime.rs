// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{anyhow, Result};
use chatos_mcp_runtime::jsonrpc_http_call;
use reqwest::header::{HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay::RelayRequest;

const INLINE_HTTP_RUNTIME_HEADER: &str = "x-local-connector-inline-mcp-runtime";
const RESOURCE_HEADER: &str = "x-plugin-management-resource-id";
const MAX_INLINE_HEADERS: usize = 64;
const MAX_INLINE_HEADER_BYTES: usize = 32 * 1024;

#[derive(Debug, Deserialize)]
struct InlineHttpRuntime {
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    timeout_ms: u64,
}

pub(super) fn is_inline_http_mcp_request(request: &RelayRequest) -> bool {
    header_text(request, INLINE_HTTP_RUNTIME_HEADER).is_some()
}

pub(super) async fn handle_inline_http_mcp_body(request: &RelayRequest) -> Result<Value> {
    required_header(request, RESOURCE_HEADER)?;
    let method = request
        .body
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if !matches!(method, "tools/list" | "tools/call") {
        return Err(anyhow!("unsupported HTTP MCP JSON-RPC method: {method}"));
    }
    let runtime =
        decode_inline_http_runtime(required_header(request, INLINE_HTTP_RUNTIME_HEADER)?)?;
    let params = request
        .body
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = jsonrpc_http_call(
        runtime.url.as_str(),
        Some(&runtime.headers.into_iter().collect::<HashMap<_, _>>()),
        method,
        params,
        Some(Duration::from_millis(
            runtime.timeout_ms.clamp(300, 120_000),
        )),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    Ok(json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or(Value::Null),
        "result": result,
    }))
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
mod tests {
    use super::*;

    #[test]
    fn accepts_https_and_loopback_http_runtimes() {
        for url in ["https://mcp.example.com/rpc", "http://127.0.0.1:39000/mcp"] {
            validate_inline_http_runtime(&InlineHttpRuntime {
                url: url.to_string(),
                headers: BTreeMap::new(),
                timeout_ms: 30_000,
            })
            .expect("valid local HTTP execution runtime");
        }
    }

    #[test]
    fn rejects_plaintext_remote_http_and_managed_headers() {
        let remote = InlineHttpRuntime {
            url: "http://mcp.example.com/rpc".to_string(),
            headers: BTreeMap::new(),
            timeout_ms: 30_000,
        };
        assert!(validate_inline_http_runtime(&remote).is_err());

        let managed = InlineHttpRuntime {
            url: "https://mcp.example.com/rpc".to_string(),
            headers: BTreeMap::from([("host".to_string(), "attacker.example.com".to_string())]),
            timeout_ms: 30_000,
        };
        assert!(validate_inline_http_runtime(&managed).is_err());
    }
}
