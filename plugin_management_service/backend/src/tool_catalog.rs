// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use chatos_mcp::{
    system_mcp_descriptor_for_record, system_mcp_provider_skills, system_mcp_tool_catalog,
    SystemMcpToolCatalog,
};
use chatos_mcp_runtime::{extract_tools, list_tools_stdio, McpStdioServer};
use chatos_service_runtime::http_body::{
    read_response_bytes_limited, read_response_json_limited, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    McpProviderSkill, McpRecord, RUNTIME_KIND_HTTP, RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY,
    RUNTIME_KIND_LOCAL_CONNECTOR_HTTP, RUNTIME_KIND_LOCAL_CONNECTOR_STDIO,
    RUNTIME_KIND_STDIO_CLOUD,
};

#[derive(Debug, Default)]
pub(crate) struct LiveMcpDescriptor {
    pub skills: Vec<McpProviderSkill>,
    pub tools: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct TaskRunnerProviderDescriptor {
    #[serde(default)]
    skills: Vec<McpProviderSkill>,
    #[serde(default)]
    tools: Vec<Value>,
}

pub(crate) async fn live_mcp_descriptor(
    config: &AppConfig,
    record: &McpRecord,
) -> Result<Option<LiveMcpDescriptor>, String> {
    if system_mcp_descriptor_for_record(record).is_some() {
        return live_system_mcp_descriptor(config, record).await.map(Some);
    }
    match record.runtime.kind.as_str() {
        RUNTIME_KIND_HTTP => {
            let tools = list_external_http_tools(config, record).await?;
            Ok(Some(LiveMcpDescriptor {
                tools,
                ..LiveMcpDescriptor::default()
            }))
        }
        RUNTIME_KIND_STDIO_CLOUD => {
            let command = record
                .runtime
                .command
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "stdio MCP is missing runtime.command".to_string())?;
            let mut server = McpStdioServer::new(
                record
                    .runtime
                    .server_name
                    .as_deref()
                    .unwrap_or(record.name.as_str()),
                command,
            )
            .with_args(record.runtime.args.clone());
            if let Some(cwd) = record
                .runtime
                .cwd
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                server = server.with_cwd(cwd);
            }
            if !record.runtime.env.is_empty() {
                server = server.with_env(
                    record
                        .runtime
                        .env
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                );
            }
            Ok(Some(LiveMcpDescriptor {
                tools: list_tools_stdio(&server).await?,
                ..LiveMcpDescriptor::default()
            }))
        }
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
        | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
        | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY => Ok(None),
        _ => Ok(None),
    }
}

async fn list_external_http_tools(
    config: &AppConfig,
    record: &McpRecord,
) -> Result<Vec<Value>, String> {
    let endpoint = reqwest::Url::parse(
        record
            .runtime
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "HTTP MCP is missing runtime.url".to_string())?,
    )
    .map_err(|_| "HTTP MCP endpoint URL is invalid".to_string())?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(
            "HTTP MCP endpoint must use HTTPS without credentials or fragments".to_string(),
        );
    }
    let host = endpoint
        .host_str()
        .ok_or_else(|| "HTTP MCP endpoint has no host".to_string())?;
    let port = endpoint
        .port_or_known_default()
        .ok_or_else(|| "HTTP MCP endpoint has no usable port".to_string())?;
    let timeout = config.user_service_request_timeout;
    let addresses = tokio::time::timeout(
        Duration::from_secs(10).min(timeout),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| "HTTP MCP endpoint DNS resolution timed out".to_string())?
    .map_err(|_| "HTTP MCP endpoint DNS resolution failed".to_string())?
    .collect::<Vec<_>>();
    let mut addresses = addresses;
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("HTTP MCP endpoint must resolve only to public network addresses".to_string());
    }
    let headers = configured_external_headers(&record.runtime.headers)?;
    let http = reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10).min(timeout))
        .timeout(timeout)
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|_| "build HTTP MCP inspection client failed".to_string())?;
    let id = format!("mcp_inspection_{}", Uuid::new_v4().simple());
    let response = http
        .post(endpoint)
        .headers(headers)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .map_err(|_| "HTTP MCP tools/list request failed".to_string())?;
    let status = response.status();
    let bytes = read_response_bytes_limited(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("HTTP MCP tools/list response could not be read: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "HTTP MCP tools/list returned HTTP {}",
            status.as_u16()
        ));
    }
    let envelope = serde_json::from_slice::<Value>(bytes.as_slice())
        .map_err(|_| "HTTP MCP tools/list returned invalid JSON".to_string())?;
    if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || envelope.get("id").and_then(Value::as_str) != Some(id.as_str())
    {
        return Err("HTTP MCP tools/list returned a mismatched JSON-RPC response".to_string());
    }
    if envelope.get("error").is_some_and(|value| !value.is_null()) {
        return Err("HTTP MCP tools/list returned a JSON-RPC error".to_string());
    }
    extract_tools(&envelope)
}

fn configured_external_headers(
    configured: &std::collections::BTreeMap<String, String>,
) -> Result<HeaderMap, String> {
    if configured.len() > 64
        || configured
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > 32 * 1024
    {
        return Err("HTTP MCP headers exceed the supported limits".to_string());
    }
    let mut headers = HeaderMap::new();
    for (name, value) in configured {
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "HTTP MCP headers contain an invalid name".to_string())?;
        if external_header_is_managed_or_unsafe(&name) {
            return Err(format!(
                "HTTP MCP header {} is managed or unsafe",
                name.as_str()
            ));
        }
        let mut value = HeaderValue::from_str(value)
            .map_err(|_| "HTTP MCP headers contain an invalid value".to_string())?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

fn external_header_is_managed_or_unsafe(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
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
            | "x-local-connector-internal-scope"
            | "x-local-connector-internal-secret"
            | "x-local-connector-internal-token"
            | "x-project-service-internal-scope"
            | "x-project-service-internal-token"
            | "x-project-service-sync-secret"
            | "x-sandbox-client-key"
            | "x-sandbox-internal-scope"
            | "x-sandbox-internal-token"
    )
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

async fn live_system_mcp_descriptor(
    config: &AppConfig,
    record: &McpRecord,
) -> Result<LiveMcpDescriptor, String> {
    let descriptor = system_mcp_descriptor_for_record(record)
        .ok_or_else(|| format!("unknown system MCP: {}", record.id))?;
    if descriptor.key == chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService {
        return fetch_task_runner_descriptor(config).await;
    }
    let tools = match system_mcp_tool_catalog(descriptor.key)? {
        SystemMcpToolCatalog::Static(tools) => tools,
        SystemMcpToolCatalog::Dynamic => Vec::new(),
    };
    let skills = system_mcp_provider_skills(descriptor.key)
        .into_iter()
        .map(|skill| serde_json::from_value(serde_json::to_value(skill).unwrap_or(Value::Null)))
        .collect::<Result<Vec<McpProviderSkill>, _>>()
        .map_err(|error| format!("decode system MCP provider skills failed: {error}"))?;
    Ok(LiveMcpDescriptor { skills, tools })
}

async fn fetch_task_runner_descriptor(config: &AppConfig) -> Result<LiveMcpDescriptor, String> {
    let url = format!(
        "{}/api/mcp/provider-descriptor",
        config.task_runner_base_url.trim_end_matches('/')
    );
    let response = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
        .map_err(|err| format!("build Task Runner descriptor client failed: {err}"))?
        .get(url)
        .send()
        .await
        .map_err(|err| format!("load Task Runner MCP descriptor failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "load Task Runner MCP descriptor returned HTTP {}",
            response.status()
        ));
    }
    let descriptor =
        read_response_json_limited::<TaskRunnerProviderDescriptor>(response, JSON_BODY_LIMIT_BYTES)
            .await
            .map_err(|err| format!("decode Task Runner MCP descriptor failed: {err}"))?;
    Ok(LiveMcpDescriptor {
        skills: descriptor.skills,
        tools: descriptor.tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_static_system_mcp_has_real_tools() {
        for descriptor in chatos_mcp::system_mcp_catalog() {
            let catalog = system_mcp_tool_catalog(descriptor.key).expect("catalog");
            if let SystemMcpToolCatalog::Static(tools) = catalog {
                assert!(!tools.is_empty(), "{}", descriptor.server_name);
                assert!(tools.iter().all(|tool| tool.get("name").is_some()));
            }
        }
    }

    #[test]
    fn external_inspection_rejects_private_and_special_networks() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(value.parse().expect("test IP")), "{value}");
        }
        assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
        assert!(is_public_ip(
            "2606:4700:4700::1111".parse().expect("public IPv6")
        ));
    }
}
