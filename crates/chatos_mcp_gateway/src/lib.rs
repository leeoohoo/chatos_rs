// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementClient, McpManagementClientConfig,
    McpManagementRuntimeSessionHandle, RuntimeSessionResponse,
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
        let runtime_session =
            McpManagementRuntimeSessionHandle::new(client, session.session_id.clone());
        let server = match build_gateway_server(
            &session,
            self.default_timeout,
            self.tool_timeouts,
            self.async_result_transport,
        ) {
            Ok(server) => server,
            Err(error) => {
                let close_error = runtime_session.close().await.err();
                return Err(match close_error {
                    Some(close_error) => {
                        format!("{error}; close invalid Runtime Session failed: {close_error}")
                    }
                    None => error,
                });
            }
        };
        Ok(ResolvedMcpGateway {
            server,
            session_id: session.session_id,
            route_revision: session.route_revision,
            configured_mcp_count: session.configured_mcp_count,
            exposed_tool_count: session.exposed_tool_count,
            effective_mcp_ids: session.effective_mcp_ids,
            provider_skills_prompt: session.provider_skills_prompt,
            runtime_session,
        })
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
    let mut server = McpHttpServer::new("mcp_management", session.mcp_server_url.clone())
        .with_headers(HashMap::from([(
            "authorization".to_string(),
            format!("Bearer {}", session.runtime_token),
        )]))
        .with_timeout(default_timeout)
        .with_async_result_transport(async_result_transport)
        .with_preserved_tool_names()
        .with_fail_on_unavailable();
    for (tool_name, timeout) in tool_timeouts {
        server = server.with_tool_timeout(tool_name, timeout);
    }
    Ok(server)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> RuntimeSessionResponse {
        RuntimeSessionResponse {
            session_id: "session-1".to_string(),
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            mcp_server_url: "http://127.0.0.1:39280/mcp".to_string(),
            runtime_token: "runtime-token".to_string(),
            configured_mcp_count: 1,
            exposed_tool_count: 1,
            effective_mcp_ids: Vec::new(),
            provider_skills_prompt: None,
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

        assert_eq!(server.headers.as_ref().map(HashMap::len), Some(1));
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
}
