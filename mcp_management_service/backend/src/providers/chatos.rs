// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::METHOD_TOOLS_LIST;
use chatos_plugin_management_sdk::SystemAgentKey;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::project_service::decode_jsonrpc_response;
use super::ProviderCallError;

mod request_builder;
mod runtime_calls;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "chatos";
const CHATOS_MCP_SCOPE: &str = "mcp.tools.call";
const CHATOS_PROVIDER_REF: &str = "chatos";
const CHATOS_MEMORY_PROVIDER_REF_PREFIX: &str = "chatos:memory:";
const CLOUD_BROWSER_SESSION_CLOSE_METHOD: &str = "browser/session/close";

pub(super) struct ChatosRequestBinding<'a> {
    owner_user_id: &'a str,
    agent_key: &'a str,
    session_id: &'a str,
    expires_at_unix: i64,
    project_id: &'a str,
    run_id: Option<&'a str>,
    turn_id: Option<&'a str>,
    task_id: Option<&'a str>,
    source_session_id: Option<&'a str>,
    source_user_message_id: Option<&'a str>,
    default_model_config_id: Option<&'a str>,
    contact_agent_id: Option<&'a str>,
    expected_project_task_ids: &'a [String],
}

impl<'a> From<&'a RuntimeSessionSnapshot> for ChatosRequestBinding<'a> {
    fn from(snapshot: &'a RuntimeSessionSnapshot) -> Self {
        Self {
            owner_user_id: snapshot.owner_user_id.as_str(),
            agent_key: snapshot.agent_key.as_str(),
            session_id: snapshot.session_id.as_str(),
            expires_at_unix: snapshot.expires_at_unix,
            project_id: snapshot.project_id.as_str(),
            run_id: snapshot.run_id.as_deref(),
            turn_id: snapshot.turn_id.as_deref(),
            task_id: snapshot.task_id.as_deref(),
            source_session_id: snapshot.source_session_id.as_deref(),
            source_user_message_id: snapshot.source_user_message_id.as_deref(),
            default_model_config_id: snapshot.default_model_config_id.as_deref(),
            contact_agent_id: snapshot.contact_agent_id.as_deref(),
            expected_project_task_ids: snapshot.expected_project_task_ids.as_slice(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ChatosProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    ask_user_request_timeout: Duration,
    browser_request_timeout: Duration,
    response_limit_bytes: usize,
}

impl ChatosProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: Duration,
        ask_user_request_timeout: Duration,
        browser_request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("ChatOS Provider base URL is invalid: {err}"))?;
        if parsed.scheme() != "https" && !cfg!(test) {
            return Err("ChatOS Provider base URL must use https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            ask_user_request_timeout,
            browser_request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if self.internal_secret.is_none() || route.provider_kind != McpProviderKind::InternalService
        {
            return false;
        }
        system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_some_and(|descriptor| {
            match descriptor.key {
                SystemMcpKey::AgentBuilder
                | SystemMcpKey::AskUser
                | SystemMcpKey::BrowserTools
                | SystemMcpKey::Notepad => {
                    route.provider_ref.as_deref() == Some(CHATOS_PROVIDER_REF)
                }
                SystemMcpKey::MemorySkillReader
                | SystemMcpKey::MemoryCommandReader
                | SystemMcpKey::MemoryPluginReader => route
                    .provider_ref
                    .as_deref()
                    .and_then(|value| value.strip_prefix(CHATOS_MEMORY_PROVIDER_REF_PREFIX))
                    .is_some_and(|value| !value.trim().is_empty()),
                _ => false,
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: &str,
        source_session_id: Option<&str>,
        expires_at_unix: i64,
    ) -> HashMap<String, Vec<Value>> {
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| is_chatos_browser_route(route))
        {
            let source_session_id = source_session_id
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(source_session_id) = source_session_id else {
                make_browser_route_unavailable(
                    route,
                    "bound source_session_id is required for cloud Browser Tools",
                );
                continue;
            };
            let binding = ChatosRequestBinding {
                owner_user_id,
                agent_key: agent_key.as_str(),
                session_id: runtime_session_id,
                expires_at_unix,
                project_id,
                run_id: None,
                turn_id: None,
                task_id: None,
                source_session_id: Some(source_session_id),
                source_user_message_id: None,
                default_model_config_id: None,
                contact_agent_id: None,
                expected_project_task_ids: &[],
            };
            match self.list_browser_tools(&binding).await {
                Ok(tools) if !tools.is_empty() => {
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                }
                Ok(_) => make_browser_route_unavailable(
                    route,
                    "ChatOS Browser Runtime reported no available tools",
                ),
                Err(error) => make_browser_route_unavailable(route, error.message.as_str()),
            }
        }
        tool_snapshots
    }

    async fn list_browser_tools(
        &self,
        binding: &ChatosRequestBinding<'_>,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let invocation_id = format!("list-browser-{}", binding.session_id);
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            SystemMcpKey::BrowserTools.as_str()
        );
        let response = self
            .bound_request(binding, endpoint, self.browser_request_timeout, secret)?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_LIST,
                "params": {}
            }))
            .send()
            .await
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Browser Runtime tools/list request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Browser Runtime tools/list response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "ChatOS Browser Runtime tools/list returned HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(
            bytes.as_slice(),
            invocation_id.as_str(),
            "ChatOS Browser Runtime tools/list",
        )?;
        extract_browser_tool_snapshot(result).map_err(ProviderCallError::invalid_response)
    }
}

fn is_chatos_browser_route(route: &ResolvedMcpRoute) -> bool {
    route.provider_kind == McpProviderKind::InternalService
        && route.provider_ref.as_deref() == Some(CHATOS_PROVIDER_REF)
        && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .is_some_and(|descriptor| descriptor.key == SystemMcpKey::BrowserTools)
}

fn make_browser_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("ChatOS Browser Runtime is unavailable: {reason}");
}

fn extract_browser_tool_snapshot(result: Value) -> Result<Vec<Value>, String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            "ChatOS Browser Runtime tools/list response has no tools array".to_string()
        })?;
    if tools.iter().any(|tool| {
        !tool.is_object()
            || tool
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
    }) {
        return Err(
            "ChatOS Browser Runtime tools/list response contains an invalid tool definition"
                .to_string(),
        );
    }
    Ok(tools)
}

pub(crate) fn memory_provider_ref(contact_agent_id: &str) -> String {
    format!(
        "{CHATOS_MEMORY_PROVIDER_REF_PREFIX}{}",
        contact_agent_id.trim()
    )
}

fn is_memory_reader(key: SystemMcpKey) -> bool {
    matches!(
        key,
        SystemMcpKey::MemorySkillReader
            | SystemMcpKey::MemoryCommandReader
            | SystemMcpKey::MemoryPluginReader
    )
}

#[cfg(test)]
mod tests;
