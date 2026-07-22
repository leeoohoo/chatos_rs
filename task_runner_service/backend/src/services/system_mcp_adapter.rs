// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp::{
    system_mcp_descriptor, ResolvedSystemMcpBackend, SystemMcpHost, SystemMcpHostAdapter,
    SystemMcpResolveContext,
};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::SystemMcpKey;

use crate::config::AppConfig;
use crate::models::PUBLIC_PROJECT_ID;

pub(super) struct TaskRunnerSystemMcpAdapter<'a> {
    config: &'a AppConfig,
}

impl<'a> TaskRunnerSystemMcpAdapter<'a> {
    pub(super) const fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SystemMcpHostAdapter for TaskRunnerSystemMcpAdapter<'_> {
    fn host(&self) -> SystemMcpHost {
        SystemMcpHost::TaskRunner
    }

    async fn resolve(
        &self,
        key: SystemMcpKey,
        context: &SystemMcpResolveContext,
    ) -> Result<ResolvedSystemMcpBackend, String> {
        let descriptor = system_mcp_descriptor(key);
        if !descriptor.supports_host(self.host()) {
            return Ok(ResolvedSystemMcpBackend::Unavailable(format!(
                "system MCP {} is not supported by Task Runner",
                descriptor.server_name
            )));
        }
        match key {
            SystemMcpKey::ProjectRuntimeEnvironment => {
                self.resolve_project_runtime_environment(context)
            }
            _ if descriptor.embedded_kind.is_some() => {
                Ok(ResolvedSystemMcpBackend::Unavailable(format!(
                    "embedded system MCP {} is resolved by the Task Runner builtin registry",
                    descriptor.server_name
                )))
            }
            _ => Ok(ResolvedSystemMcpBackend::Unavailable(format!(
                "Task Runner has no backend adapter for system MCP {}",
                descriptor.server_name
            ))),
        }
    }
}

impl TaskRunnerSystemMcpAdapter<'_> {
    fn resolve_project_runtime_environment(
        &self,
        context: &SystemMcpResolveContext,
    ) -> Result<ResolvedSystemMcpBackend, String> {
        let project_id = context
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "project runtime environment MCP requires a project-scoped task".to_string()
            })?;
        if project_id == PUBLIC_PROJECT_ID {
            return Err(
                "project runtime environment MCP requires a project-scoped task".to_string(),
            );
        }
        let base_url = self
            .config
            .project_service_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "TASK_RUNNER_PROJECT_SERVICE_BASE_URL is required for the project runtime environment MCP"
                    .to_string()
            })?
            .trim_end_matches('/');
        let secret = self
            .config
            .project_service_sync_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET is required for the project runtime environment MCP"
                    .to_string()
            })?;
        let descriptor = system_mcp_descriptor(SystemMcpKey::ProjectRuntimeEnvironment);
        let url = format!(
            "{base_url}/api/chatos-sync/projects/{}/runtime-environment/mcp",
            urlencoding::encode(project_id)
        );
        let mut headers = context.headers.clone();
        super::project_management_api_client::insert_project_service_mcp_signing_headers(
            &mut headers,
            secret,
            super::project_management_api_client::PROJECT_READ_SCOPE,
        )?;
        if let Some(task_id) = context
            .task_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            headers.insert("x-task-runner-task-id".to_string(), task_id.to_string());
        }
        headers.insert(
            "x-task-runner-project-id".to_string(),
            project_id.to_string(),
        );
        if let Some(owner_user_id) = context
            .owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            headers.insert(
                "x-task-runner-owner-user-id".to_string(),
                owner_user_id.to_string(),
            );
        }
        Ok(ResolvedSystemMcpBackend::Http(
            McpHttpServer::new(descriptor.server_name, url)
                .with_headers(headers.into_iter().collect()),
        ))
    }
}
