// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chatos_mcp_runtime::{jsonrpc_http_call, jsonrpc_stdio_call};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::configs::{stdio_server_for_manifest, validate_loopback_http_url};
use super::manifest::{manifest_for_execution, LocalMcpTransport};

const MANIFEST_HEADER: &str = "x-local-connector-mcp-manifest-id";
const RESOURCE_HEADER: &str = "x-plugin-management-resource-id";

pub(crate) fn is_user_mcp_request(request: &RelayRequest) -> bool {
    header_text(request, MANIFEST_HEADER).is_some()
}

pub(crate) async fn handle_user_mcp_body(
    request: &RelayRequest,
    state: &LocalState,
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
    let manifest_id = required_header(request, MANIFEST_HEADER)?;
    let plugin_mcp_id = required_header(request, RESOURCE_HEADER)?;
    let manifest =
        manifest_for_execution(state, owner_user_id, device_id, manifest_id, plugin_mcp_id)?;
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
    let result = match manifest.transport {
        LocalMcpTransport::Stdio => {
            let server = stdio_server_for_manifest(manifest)?;
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
            jsonrpc_http_call(
                config.url.as_str(),
                Some(&headers),
                method,
                params,
                Some(Duration::from_millis(config.timeout_ms.clamp(300, 120_000))),
            )
            .await
            .map_err(anyhow::Error::msg)?
        }
    };
    Ok(json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or(Value::Null),
        "result": result,
    }))
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
