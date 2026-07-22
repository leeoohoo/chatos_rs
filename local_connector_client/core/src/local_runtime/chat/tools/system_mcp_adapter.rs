// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp::{
    system_mcp_descriptor, ResolvedSystemMcpBackend, SystemMcpHost, SystemMcpHostAdapter,
    SystemMcpResolveContext,
};
use chatos_mcp_runtime::{BuiltinMcpServerOptions, BuiltinToolProvider, McpBuiltinServer};
use chatos_plugin_management_sdk::SystemMcpKey;

use crate::local_runtime::ask_user::LocalAskUserProvider;
use crate::local_runtime::project_management::LocalProjectManagementProvider;
use crate::local_runtime::task_board::LocalTaskManagerProvider;
use crate::local_runtime::task_runner::LocalTaskRunnerServiceProvider;

use super::builtins::LocalChatBuiltinProvider;
use super::context::LocalChatToolContext;

pub(super) struct LocalConnectorSystemMcpAdapter {
    context: LocalChatToolContext,
    options: BuiltinMcpServerOptions,
}

impl LocalConnectorSystemMcpAdapter {
    pub(super) fn new(context: LocalChatToolContext) -> Self {
        let options = BuiltinMcpServerOptions::new(context.project_root.display().to_string())
            .with_user_id(owner_user_id(&context))
            .with_project_id(project_id(&context))
            .with_auto_create_task(context.auto_create_task);
        Self { context, options }
    }
}

#[async_trait]
impl SystemMcpHostAdapter for LocalConnectorSystemMcpAdapter {
    fn host(&self) -> SystemMcpHost {
        SystemMcpHost::LocalConnector
    }

    async fn resolve(
        &self,
        key: SystemMcpKey,
        _context: &SystemMcpResolveContext,
    ) -> Result<ResolvedSystemMcpBackend, String> {
        let descriptor = system_mcp_descriptor(key);
        if !descriptor.supports_host(self.host()) {
            return Ok(ResolvedSystemMcpBackend::Unavailable(format!(
                "system MCP {} is not supported by Local Connector",
                descriptor.server_name
            )));
        }
        if key == SystemMcpKey::TaskRunnerService {
            let provider: Arc<dyn BuiltinToolProvider> = Arc::new(
                LocalTaskRunnerServiceProvider::new(
                    self.context.database.clone(),
                    owner_user_id(&self.context),
                    project_id(&self.context),
                    self.context.session_id.clone(),
                    self.context.source_turn_id.clone(),
                    self.context.default_model_config_id.clone(),
                    &self.context.state,
                )
                .await?,
            );
            return Ok(ResolvedSystemMcpBackend::Embedded {
                server: McpBuiltinServer {
                    name: descriptor.server_name.to_string(),
                    kind: descriptor.key.as_str().to_string(),
                    workspace_dir: self.context.project_root.display().to_string(),
                    user_id: Some(owner_user_id(&self.context)),
                    project_id: Some(project_id(&self.context)),
                    remote_connection_id: None,
                    contact_agent_id: None,
                    auto_create_task: false,
                    allow_writes: descriptor.allow_writes,
                    max_file_bytes: 0,
                    max_write_bytes: 0,
                    search_limit: 0,
                },
                provider: Some(provider),
            });
        }
        let Some(kind) = descriptor.embedded_kind else {
            return Ok(ResolvedSystemMcpBackend::Unavailable(format!(
                "Local Connector has no embedded provider for system MCP {}",
                descriptor.server_name
            )));
        };
        let provider: Arc<dyn BuiltinToolProvider> = match key {
            SystemMcpKey::ProjectManagement => Arc::new(LocalProjectManagementProvider::new(
                self.context.database.clone(),
                owner_user_id(&self.context),
                project_id(&self.context),
            )),
            SystemMcpKey::TaskManager => Arc::new(LocalTaskManagerProvider::new(
                self.context.database.clone(),
                owner_user_id(&self.context),
                self.context.auto_create_task,
                self.context.ask_user_prompts.clone(),
            )),
            SystemMcpKey::AskUser => Arc::new(LocalAskUserProvider::new(
                self.context.database.clone(),
                owner_user_id(&self.context),
                self.context.ask_user_prompts.clone(),
            )),
            _ => Arc::new(LocalChatBuiltinProvider::new(
                kind,
                self.context.request.clone(),
                self.context.state.clone(),
                self.context.history_recorder.clone(),
            )),
        };
        Ok(ResolvedSystemMcpBackend::Embedded {
            server: kind.server_with_options(&self.options),
            provider: Some(provider),
        })
    }
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
