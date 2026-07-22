// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp::{
    system_mcp_descriptor_for_record, system_mcp_provider_skills, system_mcp_tool_catalog,
    SystemMcpToolCatalog,
};
use chatos_mcp_runtime::{list_tools_http, list_tools_stdio, McpStdioServer};
use chatos_service_runtime::http_body::{read_response_json_limited, JSON_BODY_LIMIT_BYTES};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use serde::Deserialize;
use serde_json::Value;

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
            let url = record
                .runtime
                .url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "HTTP MCP is missing runtime.url".to_string())?;
            let headers = record
                .runtime
                .headers
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<HashMap<_, _>>();
            let tools = list_tools_http(
                url,
                (!headers.is_empty()).then_some(&headers),
                Some(config.user_service_request_timeout),
            )
            .await?;
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
}
