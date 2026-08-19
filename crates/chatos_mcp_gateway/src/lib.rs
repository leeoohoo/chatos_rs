// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementClient, McpManagementClientConfig,
    McpManagementRuntimeSessionHandle, RuntimeSessionResponse, RuntimeSessionRoutesResponse,
    RuntimeWorkspaceRouteTarget,
};
use chatos_mcp_runtime::{McpAsyncResultTransport, McpHttpServer};

pub struct McpManagementGatewayBuilder {
    caller_service: String,
    request: CreateRuntimeSessionRequest,
    default_timeout: Duration,
    tool_timeouts: HashMap<String, Duration>,
    async_result_transport: McpAsyncResultTransport,
}

impl McpManagementGatewayBuilder {
    pub fn new(
        caller_service: impl Into<String>,
        request: CreateRuntimeSessionRequest,
        default_timeout: Duration,
    ) -> Self {
        Self {
            caller_service: caller_service.into(),
            request,
            default_timeout,
            tool_timeouts: HashMap::new(),
            async_result_transport: McpAsyncResultTransport::Disabled,
        }
    }

    pub fn with_tool_timeout(mut self, tool_name: impl Into<String>, timeout: Duration) -> Self {
        let tool_name = tool_name.into().trim().to_string();
        if !tool_name.is_empty() {
            self.tool_timeouts.insert(tool_name, timeout);
        }
        self
    }

    pub fn with_async_result_transport(mut self, transport: McpAsyncResultTransport) -> Self {
        self.async_result_transport = transport;
        self
    }

    pub async fn resolve(self) -> Result<ResolvedMcpGateway, String> {
        let config = McpManagementClientConfig::from_env(self.caller_service.clone())
            .await
            .map_err(|error| format!("load MCP Management config failed: {error}"))?;
        let client = McpManagementClient::new(config)
            .map_err(|error| format!("initialize MCP Management client failed: {error}"))?;
        let session = client
            .resolve_runtime_session(&self.request)
            .await
            .map_err(|error| format!("resolve MCP Management runtime session failed: {error}"))?;
        build_resolved_gateway(
            client,
            session,
            self.default_timeout,
            self.tool_timeouts,
            self.async_result_transport,
            true,
        )
        .await
    }

    pub async fn resolve_existing(self, session_id: &str) -> Result<ResolvedMcpGateway, String> {
        self.resolve_existing_with_identity(session_id, true).await
    }

    /// Reconnects to an existing runtime session when the caller cannot provide
    /// the original request identity. Run-scoped callers must use resolve_existing.
    pub async fn resolve_existing_unchecked(
        self,
        session_id: &str,
    ) -> Result<ResolvedMcpGateway, String> {
        self.resolve_existing_with_identity(session_id, false).await
    }

    async fn resolve_existing_with_identity(
        self,
        session_id: &str,
        validate_identity: bool,
    ) -> Result<ResolvedMcpGateway, String> {
        let config = McpManagementClientConfig::from_env(self.caller_service.clone())
            .await
            .map_err(|error| format!("load MCP Management config failed: {error}"))?;
        let client = McpManagementClient::new(config)
            .map_err(|error| format!("initialize MCP Management client failed: {error}"))?;
        let session = client
            .runtime_session_routes(session_id)
            .await
            .map_err(|error| format!("resolve existing MCP runtime session failed: {error}"))?;
        if validate_identity {
            validate_existing_session_identity(&self.request, &session)?;
        }
        let response = RuntimeSessionResponse {
            session_id: session.session_id,
            policy_revision: session.policy_revision,
            route_revision: session.route_revision,
            expires_at: session.expires_at,
            mcp_server_url: session.mcp_server_url,
            mcp_command_queue: session.mcp_command_queue,
            runtime_token: session.runtime_token,
            configured_mcp_count: session.routes.len(),
            exposed_tool_count: session.tools.len(),
            effective_mcp_ids: Vec::new(),
            provider_skills_prompt: None,
            plugin_instruction_items: Vec::new(),
            unavailable_required_mcps: Vec::new(),
        };
        build_resolved_gateway(
            client,
            response,
            self.default_timeout,
            self.tool_timeouts,
            self.async_result_transport,
            false,
        )
        .await
    }
}

pub struct ResolvedMcpGateway {
    pub server: McpHttpServer,
    pub session_id: String,
    pub route_revision: String,
    pub configured_mcp_count: usize,
    pub exposed_tool_count: usize,
    pub effective_mcp_ids: Vec<String>,
    pub provider_skills_prompt: Option<String>,
    pub plugin_instruction_items: Vec<serde_json::Value>,
    pub mcp_command_queue: String,
    pub runtime_token: String,
    pub runtime_session: McpManagementRuntimeSessionHandle,
}

fn build_gateway_server(
    session: &RuntimeSessionResponse,
    default_timeout: Duration,
    tool_timeouts: HashMap<String, Duration>,
    async_result_transport: McpAsyncResultTransport,
) -> Result<McpHttpServer, String> {
    if session.mcp_server_url.trim().is_empty() {
        return Err("MCP Management runtime session returned an empty MCP server URL".to_string());
    }
    if session.runtime_token.trim().is_empty() {
        return Err("MCP Management runtime session returned an empty runtime token".to_string());
    }
    if session.mcp_command_queue.trim().is_empty() {
        return Err("MCP Management runtime session returned an empty command queue".to_string());
    }
    let mut server = McpHttpServer::new("mcp_management", session.mcp_server_url.clone())
        .with_headers(HashMap::from([
            (
                "authorization".to_string(),
                format!("Bearer {}", session.runtime_token),
            ),
            (
                "x-chatos-mcp-command-queue".to_string(),
                session.mcp_command_queue.clone(),
            ),
        ]))
        .with_timeout(default_timeout)
        .with_async_result_transport(async_result_transport)
        .with_preserved_tool_names()
        .with_fail_on_unavailable();
    for (tool_name, timeout) in tool_timeouts {
        server = server.with_tool_timeout(tool_name, timeout);
    }
    Ok(server)
}

async fn build_resolved_gateway(
    client: McpManagementClient,
    session: RuntimeSessionResponse,
    default_timeout: Duration,
    tool_timeouts: HashMap<String, Duration>,
    async_result_transport: McpAsyncResultTransport,
    close_invalid_session: bool,
) -> Result<ResolvedMcpGateway, String> {
    let runtime_session =
        McpManagementRuntimeSessionHandle::new(client, session.session_id.clone());
    let server = match build_gateway_server(
        &session,
        default_timeout,
        tool_timeouts,
        async_result_transport,
    ) {
        Ok(server) => server,
        Err(error) if close_invalid_session => {
            let close_error = runtime_session.close().await.err();
            return Err(match close_error {
                Some(close_error) => {
                    format!("{error}; close invalid Runtime Session failed: {close_error}")
                }
                None => error,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ResolvedMcpGateway {
        server,
        session_id: session.session_id,
        route_revision: session.route_revision,
        configured_mcp_count: session.configured_mcp_count,
        exposed_tool_count: session.exposed_tool_count,
        effective_mcp_ids: session.effective_mcp_ids,
        provider_skills_prompt: session.provider_skills_prompt,
        plugin_instruction_items: session.plugin_instruction_items,
        mcp_command_queue: session.mcp_command_queue,
        runtime_token: session.runtime_token,
        runtime_session,
    })
}

fn validate_existing_session_identity(
    request: &CreateRuntimeSessionRequest,
    session: &RuntimeSessionRoutesResponse,
) -> Result<(), String> {
    let expected = [
        (
            "tenant_id",
            normalized_text(request.tenant_id.as_str()),
            normalized_text(session.tenant_id.as_str()),
        ),
        (
            "owner_user_id",
            normalized_text(request.owner_user_id.as_str()),
            normalized_text(session.owner_user_id.as_str()),
        ),
        (
            "agent_key",
            normalized_text(request.agent_key.as_str()),
            normalized_text(session.agent_key.as_str()),
        ),
        (
            "project_id",
            normalized_text(request.project_id.as_str()),
            normalized_text(session.project_id.as_str()),
        ),
    ];
    for (field, expected, actual) in expected {
        if expected != actual {
            return Err(format!(
                "existing MCP runtime session identity mismatch for {field}: expected {}, got {}",
                identity_label(expected.as_deref()),
                identity_label(actual.as_deref())
            ));
        }
    }

    for (field, expected, actual) in [
        (
            "run_id",
            request.run_id.as_deref(),
            session.run_id.as_deref(),
        ),
        (
            "execution_group_id",
            request.execution_group_id.as_deref(),
            session.execution_group_id.as_deref(),
        ),
        (
            "task_id",
            request.task_id.as_deref(),
            session.task_id.as_deref(),
        ),
        (
            "task_profile",
            request.task_profile.as_deref(),
            session.task_profile.as_deref(),
        ),
    ] {
        validate_optional_identity_field(field, expected, actual)?;
    }

    if request.workspace_route.as_ref() != session.workspace_route.as_ref() {
        return Err(format!(
            "existing MCP runtime session identity mismatch for workspace_route: expected {}, got {}",
            workspace_route_label(request.workspace_route.as_ref()),
            workspace_route_label(session.workspace_route.as_ref())
        ));
    }

    Ok(())
}

fn validate_optional_identity_field(
    field: &str,
    expected: Option<&str>,
    actual: Option<&str>,
) -> Result<(), String> {
    let expected = normalized_option(expected);
    let actual = normalized_option(actual);
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "existing MCP runtime session identity mismatch for {field}: expected {}, got {}",
        identity_label(expected.as_deref()),
        identity_label(actual.as_deref())
    ))
}

fn normalized_option(value: Option<&str>) -> Option<String> {
    value.and_then(normalized_text)
}

fn normalized_text(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn identity_label(value: Option<&str>) -> &str {
    value.unwrap_or("<none>")
}

fn workspace_route_label(value: Option<&RuntimeWorkspaceRouteTarget>) -> String {
    value
        .and_then(|route| serde_json::to_string(route).ok())
        .unwrap_or_else(|| "<none>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_request(run_id: Option<&str>) -> CreateRuntimeSessionRequest {
        CreateRuntimeSessionRequest {
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_role: None,
            agent_key: "task_runner_agent".to_string(),
            project_id: "project-1".to_string(),
            run_id: run_id.map(ToOwned::to_owned),
            execution_group_id: Some("group-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            task_profile: Some("task".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            contact_agent_id: None,
            default_model_config_id: None,
            tool_result_max_chars: None,
            expected_project_task_ids: Vec::new(),
            requested_mcp_ids: None,
            selected_plugins: Vec::new(),
            plugin_command_invocations: Vec::new(),
            locale: None,
            workspace_route: None,
        }
    }

    fn routes_response(run_id: Option<&str>) -> RuntimeSessionRoutesResponse {
        RuntimeSessionRoutesResponse {
            session_id: "session-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_agent".to_string(),
            project_id: "project-1".to_string(),
            device_id: Some("device-1".to_string()),
            run_id: run_id.map(ToOwned::to_owned),
            execution_group_id: Some("group-1".to_string()),
            task_id: Some("task-1".to_string()),
            task_profile: Some("task".to_string()),
            workspace_route: None,
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            routes: Vec::new(),
            tools: Vec::new(),
            mcp_command_queue: "mcp_management.async.dispatch".to_string(),
            mcp_server_url: "http://127.0.0.1:39280/mcp".to_string(),
            runtime_token: "runtime-token".to_string(),
        }
    }

    fn session() -> RuntimeSessionResponse {
        RuntimeSessionResponse {
            session_id: "session-1".to_string(),
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            mcp_server_url: "http://127.0.0.1:39280/mcp".to_string(),
            mcp_command_queue: "mcp_management.async.dispatch".to_string(),
            runtime_token: "runtime-token".to_string(),
            configured_mcp_count: 1,
            exposed_tool_count: 1,
            effective_mcp_ids: Vec::new(),
            provider_skills_prompt: None,
            plugin_instruction_items: Vec::new(),
            unavailable_required_mcps: Vec::new(),
        }
    }

    #[test]
    fn gateway_server_uses_only_runtime_bearer_and_explicit_async_transport() {
        let server = build_gateway_server(
            &session(),
            Duration::from_secs(30),
            HashMap::from([("wait".to_string(), Duration::from_secs(60))]),
            McpAsyncResultTransport::RabbitMq,
        )
        .expect("gateway server");

        assert_eq!(server.headers.as_ref().map(HashMap::len), Some(2));
        assert_eq!(
            server
                .headers
                .as_ref()
                .and_then(|headers| headers.get("authorization"))
                .map(String::as_str),
            Some("Bearer runtime-token")
        );
        assert_eq!(
            server.async_result_transport,
            McpAsyncResultTransport::RabbitMq
        );
        assert_eq!(
            server.tool_timeout_duration("wait"),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn gateway_server_rejects_empty_runtime_credentials() {
        let mut session = session();
        session.runtime_token.clear();
        assert!(build_gateway_server(
            &session,
            Duration::from_secs(30),
            HashMap::new(),
            McpAsyncResultTransport::RabbitMq,
        )
        .is_err());
    }

    #[test]
    fn existing_session_identity_accepts_the_same_run() {
        let request = create_request(Some("run-1"));
        let session = routes_response(Some("run-1"));

        validate_existing_session_identity(&request, &session).expect("same run should match");
    }

    #[test]
    fn existing_session_identity_rejects_a_different_run() {
        let request = create_request(Some("backend-run"));
        let session = routes_response(Some("frontend-run"));

        let error = validate_existing_session_identity(&request, &session)
            .expect_err("different run must not be reused");

        assert!(error.contains("run_id"));
        assert!(error.contains("backend-run"));
        assert!(error.contains("frontend-run"));
    }

    #[test]
    fn existing_session_identity_rejects_a_different_project() {
        let request = create_request(Some("run-1"));
        let mut session = routes_response(Some("run-1"));
        session.project_id = "other-project".to_string();

        let error = validate_existing_session_identity(&request, &session)
            .expect_err("different project must not be reused");

        assert!(error.contains("project_id"));
        assert!(error.contains("project-1"));
        assert!(error.contains("other-project"));
    }

    #[test]
    fn existing_session_identity_rejects_missing_expected_identity() {
        let mut request = create_request(Some("run-1"));
        request.project_id.clear();
        let session = routes_response(Some("run-1"));

        let error = validate_existing_session_identity(&request, &session)
            .expect_err("blank expected project identity must not match a populated session");

        assert!(error.contains("project_id"));
        assert!(error.contains("<none>"));
        assert!(error.contains("project-1"));
    }

    #[test]
    fn existing_session_identity_rejects_a_different_execution_group() {
        let request = create_request(Some("run-1"));
        let mut session = routes_response(Some("run-1"));
        session.execution_group_id = Some("other-group".to_string());

        let error = validate_existing_session_identity(&request, &session)
            .expect_err("different execution group must not be reused");

        assert!(error.contains("execution_group_id"));
        assert!(error.contains("group-1"));
        assert!(error.contains("other-group"));
    }

    #[test]
    fn existing_session_identity_rejects_a_different_workspace_route() {
        let mut request = create_request(Some("run-1"));
        request.workspace_route = Some(RuntimeWorkspaceRouteTarget::LocalConnector {
            default_tool_root: None,
        });
        let session = routes_response(Some("run-1"));

        let error = validate_existing_session_identity(&request, &session)
            .expect_err("different workspace route must not be reused");

        assert!(error.contains("workspace_route"));
        assert!(error.contains("local_connector"));
        assert!(error.contains("<none>"));
    }
}
