// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_runtime::{
    builtin_kind_by_kind_name, list_tools_http, list_tools_stdio,
    local_command_approval_tool_definitions, project_environment_tool_definitions,
    project_runtime_environment_info_tool_definitions, McpStdioServer,
};
use chatos_service_runtime::http_body::{read_response_json_limited, JSON_BODY_LIMIT_BYTES};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use serde::Deserialize;
use serde_json::Value;

use crate::config::AppConfig;
use crate::models::{
    McpProviderSkill, McpRecord, RUNTIME_KIND_BUILTIN, RUNTIME_KIND_HTTP,
    RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY, RUNTIME_KIND_LOCAL_CONNECTOR_HTTP,
    RUNTIME_KIND_LOCAL_CONNECTOR_STDIO, RUNTIME_KIND_STDIO_CLOUD, RUNTIME_KIND_SYSTEM_ROUTED,
};

const SANDBOX_IMAGES_SERVER_NAME: &str = "sandbox_images";
const PROJECT_ENVIRONMENT_SERVER_NAME: &str = "project_environment";
const PROJECT_RUNTIME_ENVIRONMENT_SERVER_NAME: &str = "project_runtime_environment";
const LOCAL_COMMAND_APPROVAL_SERVER_NAME: &str = "local_connector_approval";
const TASK_RUNNER_SERVER_NAME: &str = "task_runner_service";

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

pub(crate) fn system_routed_tool_catalog(server_name: &str) -> Result<Option<Vec<Value>>, String> {
    match server_name.trim() {
        SANDBOX_IMAGES_SERVER_NAME => chatos_sandbox_image_mcp::list_tools()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .map(Some)
            .ok_or_else(|| "Sandbox Images tool registry returned no tools array".to_string()),
        PROJECT_ENVIRONMENT_SERVER_NAME => Ok(Some(project_environment_tool_definitions())),
        PROJECT_RUNTIME_ENVIRONMENT_SERVER_NAME => {
            Ok(Some(project_runtime_environment_info_tool_definitions()))
        }
        LOCAL_COMMAND_APPROVAL_SERVER_NAME => Ok(Some(local_command_approval_tool_definitions())),
        TASK_RUNNER_SERVER_NAME => Ok(None),
        _ => Ok(None),
    }
}

pub(crate) async fn live_mcp_descriptor(
    config: &AppConfig,
    record: &McpRecord,
) -> Result<Option<LiveMcpDescriptor>, String> {
    match record.runtime.kind.as_str() {
        RUNTIME_KIND_BUILTIN => {
            let kind_name = record
                .runtime
                .builtin_kind
                .as_deref()
                .ok_or_else(|| "builtin MCP is missing runtime.builtin_kind".to_string())?;
            let kind = builtin_kind_by_kind_name(kind_name)
                .ok_or_else(|| format!("unknown builtin MCP kind: {kind_name}"))?;
            Ok(Some(LiveMcpDescriptor {
                tools: chatos_builtin_tools::builtin_tool_catalog(kind)?,
                ..LiveMcpDescriptor::default()
            }))
        }
        RUNTIME_KIND_SYSTEM_ROUTED => {
            let server_name = record
                .runtime
                .server_name
                .as_deref()
                .unwrap_or(record.name.as_str());
            if server_name == TASK_RUNNER_SERVER_NAME {
                return fetch_task_runner_descriptor(config).await.map(Some);
            }
            Ok(
                system_routed_tool_catalog(server_name)?.map(|tools| LiveMcpDescriptor {
                    tools,
                    ..LiveMcpDescriptor::default()
                }),
            )
        }
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
    fn every_static_system_route_has_real_tools() {
        for server_name in [
            SANDBOX_IMAGES_SERVER_NAME,
            PROJECT_ENVIRONMENT_SERVER_NAME,
            PROJECT_RUNTIME_ENVIRONMENT_SERVER_NAME,
            LOCAL_COMMAND_APPROVAL_SERVER_NAME,
        ] {
            let tools = system_routed_tool_catalog(server_name)
                .expect("catalog")
                .expect("static system route");
            assert!(!tools.is_empty(), "{server_name}");
            assert!(tools.iter().all(|tool| tool.get("name").is_some()));
        }
    }
}
