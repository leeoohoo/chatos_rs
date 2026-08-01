// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{SystemMcpDescriptor, SystemMcpHost};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::{McpRecord, SystemMcpKey};

use crate::config::AppConfig;
use crate::models::{TaskRecord, PUBLIC_PROJECT_ID};

pub(super) fn load_legacy_system_mcp_server(
    config: &AppConfig,
    task: &TaskRecord,
    resource: &McpRecord,
    descriptor: &SystemMcpDescriptor,
    owner_user_id: Option<String>,
) -> Result<McpHttpServer, String> {
    if !descriptor.legacy_supports_host(SystemMcpHost::TaskRunner) {
        return Err(format!(
            "system MCP {} is not supported by the Task Runner legacy runtime",
            descriptor.server_name
        ));
    }
    match descriptor.key {
        SystemMcpKey::ProjectRuntimeEnvironment => load_project_runtime_environment_server(
            config,
            task,
            resource,
            descriptor,
            owner_user_id.as_deref(),
        ),
        _ if descriptor.embedded_kind.is_some() => Err(format!(
            "embedded system MCP {} is resolved by the Task Runner legacy builtin registry",
            descriptor.server_name
        )),
        _ => Err(format!(
            "Task Runner legacy runtime has no direct backend for system MCP {}",
            descriptor.server_name
        )),
    }
}

fn load_project_runtime_environment_server(
    config: &AppConfig,
    task: &TaskRecord,
    resource: &McpRecord,
    descriptor: &SystemMcpDescriptor,
    owner_user_id: Option<&str>,
) -> Result<McpHttpServer, String> {
    let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
    if project_id == PUBLIC_PROJECT_ID {
        return Err("project runtime environment MCP requires a project-scoped task".to_string());
    }
    let base_url = config
        .project_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_PROJECT_SERVICE_BASE_URL is required for the project runtime environment MCP"
                .to_string()
        })?
        .trim_end_matches('/');
    let secret = config
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET is required for the project runtime environment MCP"
                .to_string()
        })?;
    let url = format!(
        "{base_url}/api/chatos-sync/projects/{}/runtime-environment/mcp",
        urlencoding::encode(project_id.as_str())
    );
    let mut headers = resource.runtime.headers.clone();
    crate::services::project_management_api_client::insert_project_service_mcp_signing_headers(
        &mut headers,
        secret,
        crate::services::project_management_api_client::PROJECT_READ_SCOPE,
    )?;
    headers.insert("x-task-runner-task-id".to_string(), task.id.clone());
    headers.insert("x-task-runner-project-id".to_string(), project_id);
    if let Some(owner_user_id) = owner_user_id {
        headers.insert(
            "x-task-runner-owner-user-id".to_string(),
            owner_user_id.to_string(),
        );
    }
    Ok(McpHttpServer::new(descriptor.server_name, url).with_headers(headers.into_iter().collect()))
}
