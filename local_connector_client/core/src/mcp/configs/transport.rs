// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_runtime::{
    extract_tools, jsonrpc_http_call, jsonrpc_stdio_call, parse_tool_definition, McpStdioServer,
};
use serde_json::{json, Value};

use crate::mcp::manifest::{LocalMcpManifestRecord, LocalMcpTransport};

const DEFAULT_MAX_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

pub(crate) async fn test_manifest_record(record: &LocalMcpManifestRecord) -> Result<Vec<Value>> {
    let tools = match record.transport {
        LocalMcpTransport::Stdio => {
            let server = stdio_server_for_manifest(record)?;
            let response = jsonrpc_stdio_call(&server, "tools/list", json!({}), None)
                .await
                .map_err(anyhow::Error::msg)?;
            extract_tools(&response).map_err(anyhow::Error::msg)?
        }
        LocalMcpTransport::Http => {
            let config = record
                .http
                .as_ref()
                .ok_or_else(|| anyhow!("local HTTP MCP config is missing"))?;
            validate_loopback_http_url(config.url.as_str())?;
            let headers = config
                .headers
                .clone()
                .into_iter()
                .collect::<HashMap<_, _>>();
            let response = jsonrpc_http_call(
                config.url.as_str(),
                Some(&headers),
                "tools/list",
                json!({}),
                Some(Duration::from_millis(config.timeout_ms.clamp(300, 120_000))),
            )
            .await
            .map_err(anyhow::Error::msg)?;
            extract_tools(&response).map_err(anyhow::Error::msg)?
        }
    };
    sanitize_tools(tools)
}

pub(crate) fn stdio_server_for_manifest(record: &LocalMcpManifestRecord) -> Result<McpStdioServer> {
    let config = record
        .stdio
        .as_ref()
        .ok_or_else(|| anyhow!("local stdio MCP config is missing"))?;
    let mut server = McpStdioServer::new(record.internal_name.clone(), config.command.clone())
        .with_args(config.args.clone())
        .with_user_id(format!("{}:{}", record.owner_user_id, record.manifest_id));
    if !config.env.is_empty() {
        server = server.with_env(config.env.clone().into_iter().collect());
    }
    Ok(server)
}

pub(crate) fn validate_loopback_http_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("parse local MCP HTTP URL")?;
    if url.scheme() != "http" {
        return Err(anyhow!(
            "local HTTP MCP only supports http:// loopback URLs"
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("local HTTP MCP URL is missing host"))?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|ip| ip.is_loopback());
    if !loopback {
        return Err(anyhow!("local HTTP MCP URL must use a loopback host"));
    }
    Ok(())
}

fn sanitize_tools(tools: Vec<Value>) -> Result<Vec<Value>> {
    let tools = tools
        .into_iter()
        .filter(|tool| parse_tool_definition(tool).is_some())
        .take(200)
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return Err(anyhow!("MCP tools/list returned no valid tools"));
    }
    let max_bytes = crate::config::optional_env("LOCAL_CONNECTOR_MCP_MAX_TOOL_SNAPSHOT_BYTES")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_TOOL_SNAPSHOT_BYTES)
        .clamp(16 * 1024, 4 * 1024 * 1024);
    if serde_json::to_vec(&tools)?.len() > max_bytes {
        return Err(anyhow!("MCP tool snapshot exceeds {max_bytes} bytes"));
    }
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::validate_loopback_http_url;

    #[test]
    fn local_http_mcp_only_accepts_loopback_http_urls() {
        assert!(validate_loopback_http_url("http://127.0.0.1:3000/mcp").is_ok());
        assert!(validate_loopback_http_url("http://localhost:3000/mcp").is_ok());
        assert!(validate_loopback_http_url("http://10.0.0.8:3000/mcp").is_err());
        assert!(validate_loopback_http_url("https://localhost:3000/mcp").is_err());
        assert!(validate_loopback_http_url("not-a-url").is_err());
    }
}
