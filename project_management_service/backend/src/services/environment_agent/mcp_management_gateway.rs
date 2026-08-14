// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_gateway::McpManagementGatewayBuilder;
use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, HarnessBranchTarget, McpManagementRuntimeSessionHandle,
    RuntimeWorkspaceRouteTarget,
};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::SystemAgentKey;
use tracing::info;

use crate::models::ProjectRecord;

const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;

pub(super) struct ProjectEnvironmentMcpGateway {
    runtime_session: McpManagementRuntimeSessionHandle,
    server: McpHttpServer,
    provider_skills_prompt: Option<String>,
}

impl ProjectEnvironmentMcpGateway {
    pub(super) fn server(&self) -> &McpHttpServer {
        &self.server
    }

    pub(super) fn provider_skills_prompt(&self) -> Option<String> {
        self.provider_skills_prompt.clone()
    }

    pub(super) fn session_id(&self) -> &str {
        self.runtime_session.session_id()
    }

    pub(super) fn command_queue(&self) -> &str {
        self.server
            .headers
            .as_ref()
            .and_then(|headers| headers.get("x-chatos-mcp-command-queue"))
            .map(String::as_str)
            .unwrap_or_default()
    }

    pub(super) async fn close(self) -> Result<(), String> {
        self.runtime_session
            .close()
            .await
            .map(|_| ())
            .map_err(|error| {
                format!("close Project Environment MCP runtime session failed: {error}")
            })
    }
}

pub(super) async fn resolve_project_environment_mcp(
    project: &ProjectRecord,
    owner_user_id: &str,
    run_id: &str,
    model_config_id: &str,
) -> Result<ProjectEnvironmentMcpGateway, String> {
    let request = runtime_session_request(project, owner_user_id, run_id, model_config_id);
    let resolved = McpManagementGatewayBuilder::new("project-service", request, tool_timeout())
        .with_async_result_transport(chatos_mcp_runtime::McpAsyncResultTransport::RabbitMq)
        .resolve()
        .await
        .map_err(|error| format!("resolve Project Environment MCP gateway failed: {error}"))?;
    info!(
        project_id = project.id.as_str(),
        run_id,
        session_id = resolved.session_id.as_str(),
        route_revision = resolved.route_revision.as_str(),
        configured_mcp_count = resolved.configured_mcp_count,
        exposed_tool_count = resolved.exposed_tool_count,
        "Project Environment Agent resolved MCP Management runtime session"
    );
    Ok(ProjectEnvironmentMcpGateway {
        runtime_session: resolved.runtime_session,
        server: resolved.server,
        provider_skills_prompt: resolved.provider_skills_prompt,
    })
}

pub(super) async fn resolve_existing_project_environment_mcp(
    project: &ProjectRecord,
    owner_user_id: &str,
    run_id: &str,
    model_config_id: &str,
    session_id: &str,
) -> Result<ProjectEnvironmentMcpGateway, String> {
    let request = runtime_session_request(project, owner_user_id, run_id, model_config_id);
    let resolved = McpManagementGatewayBuilder::new("project-service", request, tool_timeout())
        .with_async_result_transport(chatos_mcp_runtime::McpAsyncResultTransport::RabbitMq)
        .resolve_existing(session_id)
        .await
        .map_err(|error| {
            format!("resolve existing Project Environment MCP gateway failed: {error}")
        })?;
    Ok(ProjectEnvironmentMcpGateway {
        runtime_session: resolved.runtime_session,
        server: resolved.server,
        provider_skills_prompt: resolved.provider_skills_prompt,
    })
}

fn runtime_session_request(
    project: &ProjectRecord,
    owner_user_id: &str,
    run_id: &str,
    model_config_id: &str,
) -> CreateRuntimeSessionRequest {
    let agent_key = if matches!(
        project.source_type,
        crate::models::ProjectSourceType::Local | crate::models::ProjectSourceType::LocalConnector
    ) {
        SystemAgentKey::ProjectManagementLocalAgent
    } else {
        SystemAgentKey::ProjectManagementAgent
    };
    CreateRuntimeSessionRequest {
        tenant_id: owner_user_id.trim().to_string(),
        owner_user_id: owner_user_id.trim().to_string(),
        owner_role: None,
        agent_key: agent_key.as_str().to_string(),
        project_id: project.id.trim().to_string(),
        run_id: Some(run_id.trim().to_string()),
        turn_id: None,
        task_id: None,
        task_profile: None,
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: Some(model_config_id.trim().to_string()),
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        requested_mcp_ids: None,
        selected_plugins: Vec::new(),
        plugin_command_invocations: Vec::new(),
        locale: Some("zh-CN".to_string()),
        workspace_route: Some(match project.source_type {
            crate::models::ProjectSourceType::Local
            | crate::models::ProjectSourceType::LocalConnector => {
                RuntimeWorkspaceRouteTarget::LocalConnector
            }
            crate::models::ProjectSourceType::Cloud => RuntimeWorkspaceRouteTarget::Harness {
                branch: HarnessBranchTarget::Default {
                    branch_ref: project
                        .harness_default_branch
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("main")
                        .to_string(),
                },
            },
        }),
    }
}

fn tool_timeout() -> Duration {
    Duration::from_millis(
        std::env::var("PROJECT_SERVICE_MCP_MANAGEMENT_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_MS)
            .clamp(1_000, 2 * 60 * 60 * 1_000),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project(source_type: crate::models::ProjectSourceType) -> ProjectRecord {
        ProjectRecord {
            id: "project-1".to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("user".to_string()),
            owner_display_name: Some("User".to_string()),
            name: "Project".to_string(),
            root_path: None,
            git_url: None,
            source_type,
            execution_plane: crate::models::ProjectExecutionPlane::Cloud,
            cloud_import_source: crate::models::CloudImportSource::None,
            import_status: crate::models::ProjectImportStatus::None,
            source_git_url: None,
            harness_space_identifier: None,
            harness_repo_identifier: None,
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            harness_default_branch: None,
            harness_provision_status: None,
            harness_provision_error: None,
            harness_provisioned_at: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: None,
            status: crate::models::ProjectStatus::Active,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn runtime_session_is_bound_to_project_agent_run_and_model() {
        let request = runtime_session_request(
            &sample_project(crate::models::ProjectSourceType::Cloud),
            "user-1",
            "run-1",
            "model-1",
        );
        assert_eq!(request.owner_user_id, "user-1");
        assert_eq!(request.project_id, "project-1");
        assert_eq!(
            request.agent_key,
            SystemAgentKey::ProjectManagementAgent.as_str()
        );
        assert_eq!(request.run_id.as_deref(), Some("run-1"));
        assert_eq!(request.default_model_config_id.as_deref(), Some("model-1"));
        assert_eq!(request.locale.as_deref(), Some("zh-CN"));
        assert!(matches!(
            request.workspace_route,
            Some(RuntimeWorkspaceRouteTarget::Harness { .. })
        ));
    }

    #[test]
    fn local_projects_bind_runtime_sessions_to_local_environment_agent() {
        let request = runtime_session_request(
            &sample_project(crate::models::ProjectSourceType::LocalConnector),
            "user-1",
            "run-1",
            "model-1",
        );

        assert_eq!(
            request.agent_key,
            SystemAgentKey::ProjectManagementLocalAgent.as_str()
        );
        assert_eq!(
            request.workspace_route,
            Some(RuntimeWorkspaceRouteTarget::LocalConnector)
        );
    }
}
