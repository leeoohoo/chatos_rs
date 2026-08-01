// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chatos_ai_runtime::{McpRuntimeToolExecutor, ToolExecutor};
use chatos_mcp::system_mcp_descriptor_by_embedded_kind;
use chatos_mcp_runtime::{McpExecutor, McpHttpServer, McpStdioServer};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::local_runtime::storage::{LocalProjectRecord, LocalRuntimeSettingsRecord};
use crate::mcp::configs::{stdio_server_for_manifest, validate_loopback_http_url};
use crate::mcp::manifest::{LocalMcpManifestRecord, LocalMcpTransport};
use crate::LocalRuntime;

use super::context::{resolve_local_chat_tool_context, LocalChatToolContext};
use super::legacy_system_mcp::build_legacy_local_system_mcp;

pub(crate) struct PreparedLocalChatTools {
    pub(crate) executor: Option<Arc<dyn ToolExecutor>>,
    pub(crate) available_tools: Vec<Value>,
    pub(crate) unavailable_tools: Vec<Value>,
    pub(crate) project_root: PathBuf,
    pub(crate) capability_prompt: Option<String>,
}

pub(crate) async fn prepare_local_chat_tools(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    request_id: &str,
    project: &LocalProjectRecord,
    settings: &LocalRuntimeSettingsRecord,
    agent_key: SystemAgentKey,
    include_all_configured: bool,
    expected_project_task_ids: &[String],
) -> Result<PreparedLocalChatTools, String> {
    let context = resolve_local_chat_tool_context(
        runtime,
        owner_user_id,
        request_id,
        project,
        settings,
        agent_key,
        include_all_configured,
        expected_project_task_ids,
    )
    .await?;
    if !context.enabled {
        return Ok(PreparedLocalChatTools {
            executor: None,
            available_tools: Vec::new(),
            unavailable_tools: Vec::new(),
            project_root: context.project_root,
            capability_prompt: context.capability_prompt,
        });
    }

    let project_root = context.project_root.clone();
    let capability_prompt = context.capability_prompt.clone();
    let executor = build_mcp_executor(context).await?;
    let available_tools = executor.available_tools();
    let unavailable_tools = executor.unavailable_tools();
    let tool_executor = (!available_tools.is_empty())
        .then(|| Arc::new(McpRuntimeToolExecutor::new(executor)) as Arc<dyn ToolExecutor>);
    Ok(PreparedLocalChatTools {
        executor: tool_executor,
        available_tools,
        unavailable_tools,
        project_root,
        capability_prompt,
    })
}

async fn build_mcp_executor(context: LocalChatToolContext) -> Result<McpExecutor, String> {
    let mut builder = McpExecutor::builder();
    for kind in context.builtin_kinds.iter().copied() {
        if kind == chatos_mcp_runtime::BuiltinMcpKind::TaskManager {
            continue;
        }
        let descriptor = system_mcp_descriptor_by_embedded_kind(kind)
            .ok_or_else(|| format!("missing system MCP descriptor for {}", kind.kind_name()))?;
        let resolved = build_legacy_local_system_mcp(&context, descriptor.key).await?;
        builder = builder.with_builtin_server(resolved.server);
        builder = builder.with_builtin_provider_arc(resolved.provider);
    }
    for key in context.host_system_mcps.iter().copied() {
        let resolved = build_legacy_local_system_mcp(&context, key).await?;
        builder = builder.with_builtin_server(resolved.server);
        builder = builder.with_builtin_provider_arc(resolved.provider);
    }
    for skill in &context.skills {
        if let (Some(server), Some(provider)) = (&skill.server, &skill.provider) {
            builder = builder.with_builtin_server(server.clone());
            builder = builder.with_builtin_provider(provider.clone());
        }
    }
    for manifest in &context.user_manifests {
        match manifest.transport {
            LocalMcpTransport::Stdio => {
                builder = builder.with_stdio_server(local_stdio_server(manifest)?);
            }
            LocalMcpTransport::Http => {
                builder = builder.with_http_server(local_http_server(manifest)?);
            }
        }
    }
    builder.build_initialized().await
}

fn local_stdio_server(manifest: &LocalMcpManifestRecord) -> Result<McpStdioServer, String> {
    stdio_server_for_manifest(manifest).map_err(|error| error.to_string())
}

fn local_http_server(manifest: &LocalMcpManifestRecord) -> Result<McpHttpServer, String> {
    let config = manifest
        .http
        .as_ref()
        .ok_or_else(|| "local HTTP MCP config is missing".to_string())?;
    validate_loopback_http_url(config.url.as_str()).map_err(|error| error.to_string())?;
    Ok(
        McpHttpServer::new(manifest.internal_name.clone(), config.url.clone())
            .with_headers(
                config
                    .headers
                    .clone()
                    .into_iter()
                    .collect::<HashMap<_, _>>(),
            )
            .with_timeout(Duration::from_millis(config.timeout_ms.clamp(300, 120_000))),
    )
}
