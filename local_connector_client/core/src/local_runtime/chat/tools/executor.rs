// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chatos_ai_runtime::{McpRuntimeToolExecutor, ToolExecutor};
use chatos_mcp_runtime::{BuiltinMcpServerOptions, McpExecutor, McpHttpServer, McpStdioServer};
use serde_json::Value;

use crate::local_runtime::ask_user::LocalAskUserProvider;
use crate::local_runtime::project_management::LocalProjectManagementProvider;
use crate::local_runtime::storage::{LocalProjectRecord, LocalRuntimeSettingsRecord};
use crate::local_runtime::task_board::LocalTaskManagerProvider;
use crate::mcp::configs::{stdio_server_for_manifest, validate_loopback_http_url};
use crate::mcp::manifest::{LocalMcpManifestRecord, LocalMcpTransport};
use crate::LocalRuntime;

use super::builtins::LocalChatBuiltinProvider;
use super::context::{resolve_local_chat_tool_context, LocalChatToolContext};

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
) -> Result<PreparedLocalChatTools, String> {
    let context =
        resolve_local_chat_tool_context(runtime, owner_user_id, request_id, project, settings)
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
    let options = BuiltinMcpServerOptions::new(context.project_root.display().to_string())
        .with_user_id(
            context
                .request
                .owner_user_id
                .clone()
                .unwrap_or_else(|| "local_runtime".to_string()),
        )
        .with_project_id(
            context
                .request
                .headers
                .get("x-task-runner-task-id")
                .cloned()
                .unwrap_or_else(|| context.request.workspace_id.clone()),
        )
        .with_auto_create_task(context.auto_create_task);
    let mut builder = McpExecutor::builder();
    for kind in context.builtin_kinds.iter().copied() {
        builder = builder.with_builtin_server(kind.server_with_options(&options));
        builder = match kind {
            chatos_mcp_runtime::BuiltinMcpKind::ProjectManagement => {
                builder.with_builtin_provider(LocalProjectManagementProvider::new(
                    context.database.clone(),
                    owner_user_id(&context),
                    project_id(&context),
                ))
            }
            chatos_mcp_runtime::BuiltinMcpKind::TaskManager => {
                builder.with_builtin_provider(LocalTaskManagerProvider::new(
                    context.database.clone(),
                    owner_user_id(&context),
                    context.auto_create_task,
                    context.ask_user_prompts.clone(),
                ))
            }
            chatos_mcp_runtime::BuiltinMcpKind::AskUser => {
                builder.with_builtin_provider(LocalAskUserProvider::new(
                    context.database.clone(),
                    owner_user_id(&context),
                    context.ask_user_prompts.clone(),
                ))
            }
            _ => builder.with_builtin_provider(LocalChatBuiltinProvider::new(
                kind,
                context.request.clone(),
                context.state.clone(),
                context.history_recorder.clone(),
            )),
        };
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

fn owner_user_id(context: &LocalChatToolContext) -> String {
    context
        .request
        .owner_user_id
        .clone()
        .unwrap_or_else(|| "local_runtime".to_string())
}

fn project_id(context: &LocalChatToolContext) -> String {
    context
        .request
        .headers
        .get("x-task-runner-task-id")
        .cloned()
        .unwrap_or_else(|| context.request.workspace_id.clone())
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
