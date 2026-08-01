// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementClient, McpManagementClientConfig,
    RuntimeSessionResponse,
};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::SystemAgentKey;
use tracing::{info, warn};

use crate::models::ProjectRecord;

const GATEWAY_SERVER_NAME: &str = "mcp_management";
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpManagementExecutionMode {
    Off,
    Shadow,
    Gateway,
}

impl McpManagementExecutionMode {
    fn from_value(value: Option<&str>) -> Self {
        match value
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "shadow" | "observe" => Self::Shadow,
            "gateway" | "enabled" | "on" | "true" | "1" => Self::Gateway,
            _ => Self::Off,
        }
    }

    fn from_env() -> Self {
        Self::from_value(
            std::env::var("PROJECT_SERVICE_MCP_MANAGEMENT_MODE")
                .ok()
                .as_deref(),
        )
    }
}

pub(super) enum ProjectEnvironmentMcpResolution {
    Legacy,
    Gateway(Box<ProjectEnvironmentMcpGateway>),
}

pub(super) struct ProjectEnvironmentMcpGateway {
    client: McpManagementClient,
    session_id: String,
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

    pub(super) async fn close(self, project_id: &str, run_id: &str) {
        if let Err(error) = self
            .client
            .close_runtime_session(self.session_id.as_str())
            .await
        {
            warn!(
                project_id,
                run_id,
                session_id = self.session_id.as_str(),
                error = %error,
                "close Project Environment MCP Management runtime session failed"
            );
        }
    }
}

pub(super) async fn resolve_project_environment_mcp(
    project: &ProjectRecord,
    owner_user_id: &str,
    run_id: &str,
    model_config_id: &str,
) -> Result<ProjectEnvironmentMcpResolution, String> {
    let mode = McpManagementExecutionMode::from_env();
    if mode == McpManagementExecutionMode::Off {
        return Ok(ProjectEnvironmentMcpResolution::Legacy);
    }
    let config = McpManagementClientConfig::from_env("project-service").await;
    let client = McpManagementClient::new(config)
        .map_err(|error| format!("initialize MCP Management client failed: {error}"))?;
    let request =
        runtime_session_request(owner_user_id, project.id.as_str(), run_id, model_config_id);
    let session = match client.resolve_runtime_session(&request).await {
        Ok(session) => session,
        Err(error) if mode == McpManagementExecutionMode::Shadow => {
            warn!(
                project_id = project.id.as_str(),
                run_id,
                error = %error,
                "MCP Management shadow session resolution failed; legacy Project Environment execution remains active"
            );
            return Ok(ProjectEnvironmentMcpResolution::Legacy);
        }
        Err(error) => {
            return Err(format!(
                "resolve MCP Management runtime session failed: {error}"
            ));
        }
    };
    info!(
        project_id = project.id.as_str(),
        run_id,
        session_id = session.session_id.as_str(),
        route_revision = session.route_revision.as_str(),
        configured_mcp_count = session.configured_mcp_count,
        exposed_tool_count = session.exposed_tool_count,
        execution_mode = ?mode,
        "Project Environment Agent resolved MCP Management runtime session"
    );
    if mode == McpManagementExecutionMode::Shadow {
        if let Err(error) = client
            .close_runtime_session(session.session_id.as_str())
            .await
        {
            warn!(
                project_id = project.id.as_str(),
                run_id,
                session_id = session.session_id.as_str(),
                error = %error,
                "close Project Environment MCP Management shadow session failed"
            );
        }
        return Ok(ProjectEnvironmentMcpResolution::Legacy);
    }
    let provider_skills_prompt = session.provider_skills_prompt.clone();
    let server = gateway_server(session.clone(), tool_timeout())?;
    Ok(ProjectEnvironmentMcpResolution::Gateway(Box::new(
        ProjectEnvironmentMcpGateway {
            client,
            session_id: session.session_id,
            server,
            provider_skills_prompt,
        },
    )))
}

fn runtime_session_request(
    owner_user_id: &str,
    project_id: &str,
    run_id: &str,
    model_config_id: &str,
) -> CreateRuntimeSessionRequest {
    CreateRuntimeSessionRequest {
        owner_user_id: owner_user_id.trim().to_string(),
        agent_key: SystemAgentKey::ProjectManagementAgent.as_str().to_string(),
        project_id: project_id.trim().to_string(),
        run_id: Some(run_id.trim().to_string()),
        turn_id: None,
        task_id: None,
        task_profile: None,
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: Some(model_config_id.trim().to_string()),
        expected_project_task_ids: Vec::new(),
        locale: Some("zh-CN".to_string()),
        requested_device_id: None,
        requested_sandbox_provider: None,
        sandbox_target: None,
    }
}

fn gateway_server(
    session: RuntimeSessionResponse,
    timeout: Duration,
) -> Result<McpHttpServer, String> {
    if session.runtime_token.trim().is_empty() {
        return Err("MCP Management runtime session returned an empty runtime token".to_string());
    }
    Ok(
        McpHttpServer::new(GATEWAY_SERVER_NAME, session.mcp_server_url)
            .with_headers(HashMap::from([(
                "authorization".to_string(),
                format!("Bearer {}", session.runtime_token),
            )]))
            .with_timeout(timeout)
            .with_preserved_tool_names()
            .with_fail_on_unavailable(),
    )
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

    #[test]
    fn execution_mode_is_explicit_and_fail_closed() {
        assert_eq!(
            McpManagementExecutionMode::from_value(Some("shadow")),
            McpManagementExecutionMode::Shadow
        );
        assert_eq!(
            McpManagementExecutionMode::from_value(Some("gateway")),
            McpManagementExecutionMode::Gateway
        );
        assert_eq!(
            McpManagementExecutionMode::from_value(Some("unexpected")),
            McpManagementExecutionMode::Off
        );
        assert_eq!(
            McpManagementExecutionMode::from_value(None),
            McpManagementExecutionMode::Off
        );
    }

    #[test]
    fn runtime_session_is_bound_to_project_agent_run_and_model() {
        let request = runtime_session_request("user-1", "project-1", "run-1", "model-1");
        assert_eq!(request.owner_user_id, "user-1");
        assert_eq!(request.project_id, "project-1");
        assert_eq!(
            request.agent_key,
            SystemAgentKey::ProjectManagementAgent.as_str()
        );
        assert_eq!(request.run_id.as_deref(), Some("run-1"));
        assert_eq!(request.default_model_config_id.as_deref(), Some("model-1"));
        assert_eq!(request.locale.as_deref(), Some("zh-CN"));
        assert!(request.sandbox_target.is_none());
        assert!(request.requested_sandbox_provider.is_none());
    }

    #[test]
    fn gateway_server_uses_runtime_grant_and_preserves_aggregated_names() {
        let server = gateway_server(
            RuntimeSessionResponse {
                session_id: "session-1".to_string(),
                policy_revision: "policy-1".to_string(),
                route_revision: "route-1".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                mcp_server_url: "http://127.0.0.1:39280/mcp".to_string(),
                runtime_token: "runtime-token".to_string(),
                configured_mcp_count: 3,
                exposed_tool_count: 8,
                effective_mcp_ids: Vec::new(),
                provider_skills_prompt: None,
                unavailable_required_mcps: Vec::new(),
            },
            Duration::from_secs(30),
        )
        .expect("gateway server");
        assert!(server.preserve_tool_names);
        assert!(server.fail_on_unavailable);
        assert_eq!(server.timeout_duration(), Some(Duration::from_secs(30)));
        assert_eq!(
            server
                .headers
                .as_ref()
                .and_then(|headers| headers.get("authorization"))
                .map(String::as_str),
            Some("Bearer runtime-token")
        );
    }
}
