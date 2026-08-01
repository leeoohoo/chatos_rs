// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_mcp::{build_shared_builtin_provider, system_mcp_descriptor};
use chatos_mcp_runtime::{
    BuiltinMcpKind, BuiltinMcpServerOptions, BuiltinToolProvider, McpBuiltinServer,
};
use chatos_plugin_management_sdk::SystemMcpKey;

use crate::local_runtime::ask_user::LocalAskUserProvider;
use crate::local_runtime::project_management::LocalProjectManagementProvider;
use crate::local_runtime::task_runner::LocalTaskRunnerServiceProvider;

use super::builtins::LocalChatBuiltinProvider;
use super::context::LocalChatToolContext;
use super::task_process_log::LocalTaskProcessLogProvider;

pub(super) struct LegacyLocalSystemMcp {
    pub(super) server: McpBuiltinServer,
    pub(super) provider: Arc<dyn BuiltinToolProvider>,
}

pub(super) async fn build_legacy_local_system_mcp(
    context: &LocalChatToolContext,
    key: SystemMcpKey,
) -> Result<LegacyLocalSystemMcp, String> {
    if key == SystemMcpKey::TaskManager {
        return Err("Task Manager builtin MCP has been removed".to_string());
    }
    let descriptor = system_mcp_descriptor(key);
    let options = BuiltinMcpServerOptions::new(context.project_root.display().to_string())
        .with_user_id(owner_user_id(context))
        .with_project_id(project_id(context))
        .with_auto_create_task(context.auto_create_task);
    if key == SystemMcpKey::TaskRunnerService {
        let provider: Arc<dyn BuiltinToolProvider> = Arc::new(
            LocalTaskRunnerServiceProvider::new(
                context.database.clone(),
                owner_user_id(context),
                project_id(context),
                context.session_id.clone(),
                context.source_turn_id.clone(),
                context.default_model_config_id.clone(),
                context.agent_key,
                context.expected_project_task_ids.clone(),
                &context.state,
            )
            .await?,
        );
        return Ok(LegacyLocalSystemMcp {
            server: bare_server(context, descriptor),
            provider,
        });
    }
    if key == SystemMcpKey::TaskProcessLog {
        let provider: Arc<dyn BuiltinToolProvider> = Arc::new(LocalTaskProcessLogProvider::new(
            context.database.clone(),
            owner_user_id(context),
            context.session_id.clone(),
            project_id(context),
            run_id(context),
        ));
        return Ok(LegacyLocalSystemMcp {
            server: bare_server(context, descriptor),
            provider,
        });
    }
    let kind = descriptor.embedded_kind.ok_or_else(|| {
        format!(
            "Local Connector legacy runtime has no embedded provider for system MCP {}",
            descriptor.server_name
        )
    })?;
    let provider: Arc<dyn BuiltinToolProvider> = match key {
        SystemMcpKey::ProjectManagement => Arc::new(LocalProjectManagementProvider::new(
            context.database.clone(),
            owner_user_id(context),
            project_id(context),
        )),
        SystemMcpKey::AskUser => Arc::new(LocalAskUserProvider::new(
            context.database.clone(),
            owner_user_id(context),
            context.ask_user_prompts.clone(),
        )),
        _ if shared_local_builtin_kind(kind) => {
            let provider = build_shared_builtin_provider(&kind.server_with_options(&options))?
                .ok_or_else(|| {
                    format!(
                        "Local Connector builtin provider is not implemented for {}",
                        kind.kind_name()
                    )
                })?;
            Arc::new(provider)
        }
        _ => Arc::new(LocalChatBuiltinProvider::new(
            kind,
            context.request.clone(),
            context.state.clone(),
            context.history_recorder.clone(),
        )),
    };
    Ok(LegacyLocalSystemMcp {
        server: kind.server_with_options(&options),
        provider,
    })
}

fn bare_server(
    context: &LocalChatToolContext,
    descriptor: &chatos_mcp::SystemMcpDescriptor,
) -> McpBuiltinServer {
    McpBuiltinServer {
        name: descriptor.server_name.to_string(),
        kind: descriptor.key.as_str().to_string(),
        workspace_dir: context.project_root.display().to_string(),
        user_id: Some(owner_user_id(context)),
        project_id: Some(project_id(context)),
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes: descriptor.allow_writes,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    }
}

fn shared_local_builtin_kind(kind: BuiltinMcpKind) -> bool {
    matches!(kind, BuiltinMcpKind::WebTools)
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

fn run_id(context: &LocalChatToolContext) -> String {
    context
        .request
        .headers
        .get("x-task-runner-run-id")
        .cloned()
        .unwrap_or_else(|| context.request.request_id.clone())
}
