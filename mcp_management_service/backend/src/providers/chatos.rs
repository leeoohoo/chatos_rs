// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST};
use chatos_plugin_management_sdk::SystemAgentKey;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::project_service::decode_jsonrpc_response;
use super::{
    decode_cancel_notification_response, ProviderCallError, ProviderCallOutcome,
    ProviderCancelOutcome,
};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "chatos";
const CHATOS_MCP_SCOPE: &str = "mcp.tools.call";
const CHATOS_PROVIDER_REF: &str = "chatos";
const CHATOS_MEMORY_PROVIDER_REF_PREFIX: &str = "chatos:memory:";
const CLOUD_BROWSER_SESSION_CLOSE_METHOD: &str = "browser/session/close";

struct ChatosRequestBinding<'a> {
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
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("ChatOS Provider base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .build()
            .map_err(|err| format!("build ChatOS Provider client failed: {err}"))?;
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

    pub(super) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|_| self.supports(route))
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "ChatOS route is not a supported System MCP",
                )
            })?;
        if is_memory_reader(descriptor.key) {
            let contact_agent_id = snapshot
                .contact_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "ChatOS Memory Reader has no bound contact agent",
                    )
                })?;
            let expected_provider_ref = memory_provider_ref(contact_agent_id);
            if snapshot
                .source_session_id
                .as_deref()
                .map(str::trim)
                .is_none_or(|value| value.is_empty())
                || route.provider_ref.as_deref() != Some(expected_provider_ref.as_str())
            {
                return Err(ProviderCallError::provider_unavailable(
                    "ChatOS Memory Reader route does not match the immutable runtime binding",
                ));
            }
        }
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let timeout = match descriptor.key {
            SystemMcpKey::AskUser => self.ask_user_request_timeout,
            SystemMcpKey::BrowserTools => self.browser_request_timeout,
            _ => self.request_timeout,
        };
        let binding = ChatosRequestBinding::from(snapshot);
        let request = self.bound_request(&binding, endpoint, timeout, secret)?;
        let response = request
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": {
                    "name": original_tool_name,
                    "arguments": arguments,
                }
            }))
            .send()
            .await
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Provider request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Provider response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "ChatOS Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, "ChatOS Provider")?;
        Ok(ProviderCallOutcome {
            result,
            response_bytes: bytes.len(),
        })
    }

    pub(super) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|_| self.supports(route))
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "ChatOS route is not a supported System MCP",
                )
            })?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let binding = ChatosRequestBinding::from(snapshot);
        let response = self
            .bound_request(&binding, endpoint, Duration::from_secs(5), secret)?
            .json(&json!({
                "jsonrpc": "2.0",
                "method": METHOD_NOTIFICATIONS_CANCELLED,
                "params": {
                    "requestId": invocation_id,
                    "reason": "MCP Management runtime cancelled the invocation"
                }
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Provider cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Provider cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "ChatOS Provider")
    }

    pub(super) async fn close_session(
        &self,
        snapshot: &RuntimeSessionSnapshot,
    ) -> Result<(), ProviderCallError> {
        let has_cloud_browser = snapshot.routes.iter().any(|route| {
            self.supports(route)
                && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                    .is_some_and(|descriptor| descriptor.key == SystemMcpKey::BrowserTools)
        });
        if !has_cloud_browser {
            return Ok(());
        }
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let invocation_id = format!("close-{}", snapshot.session_id);
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/browser_tools/sessions/{}/close",
            self.base_url,
            urlencoding::encode(snapshot.session_id.as_str())
        );
        let binding = ChatosRequestBinding::from(snapshot);
        let request =
            self.bound_request(&binding, endpoint, self.browser_request_timeout, secret)?;
        let response = request
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": CLOUD_BROWSER_SESSION_CLOSE_METHOD,
                "params": {}
            }))
            .send()
            .await
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Browser Runtime close request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Browser Runtime close response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "ChatOS Browser Runtime close was rejected with HTTP {}",
                status.as_u16()
            )));
        }
        decode_jsonrpc_response(
            bytes.as_slice(),
            invocation_id.as_str(),
            "ChatOS Browser Runtime close",
        )?;
        Ok(())
    }

    fn bound_request(
        &self,
        binding: &ChatosRequestBinding<'_>,
        endpoint: String,
        timeout: Duration,
        secret: &str,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            CHATOS_MCP_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut request = self
            .http
            .post(endpoint)
            .header("x-chatos-caller", CALLER_SERVICE)
            .header("x-chatos-internal-token", token)
            .header("x-mcp-management-owner-user-id", binding.owner_user_id)
            .header("x-mcp-management-agent-key", binding.agent_key)
            .header("x-mcp-management-session-id", binding.session_id)
            .header(
                "x-mcp-management-session-expires-at-unix",
                binding.expires_at_unix.to_string(),
            )
            .header("x-mcp-management-project-id", binding.project_id)
            .timeout(timeout);
        for (header, value) in [
            ("x-mcp-management-run-id", binding.run_id),
            ("x-mcp-management-turn-id", binding.turn_id),
            ("x-mcp-management-task-id", binding.task_id),
            (
                "x-mcp-management-source-session-id",
                binding.source_session_id,
            ),
            (
                "x-mcp-management-source-user-message-id",
                binding.source_user_message_id,
            ),
            (
                "x-mcp-management-default-model-config-id",
                binding.default_model_config_id,
            ),
            (
                "x-mcp-management-contact-agent-id",
                binding.contact_agent_id,
            ),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        if !binding.expected_project_task_ids.is_empty() {
            request = request.header(
                "x-mcp-management-expected-project-task-ids",
                binding.expected_project_task_ids.join(","),
            );
        }
        Ok(request)
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
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::extract::{Path, State};
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
        WorkspaceProviderKind,
    };

    use super::*;

    #[derive(Clone, Default)]
    struct CapturedRequest(Arc<Mutex<Option<(String, HeaderMap, Value)>>>);

    #[tokio::test]
    async fn provider_signs_request_and_forwards_chatos_session_binding() {
        let captured = CapturedRequest::default();
        let app = Router::new()
            .route(
                "/internal/mcp-management/mcp/{system_key}",
                post(
                    |State(captured): State<CapturedRequest>,
                     Path(system_key): Path<String>,
                     headers: HeaderMap,
                     Json(body): Json<Value>| async move {
                        *captured.0.lock().expect("capture request") =
                            Some((system_key, headers.clone(), body.clone()));
                        Json(json!({
                            "jsonrpc": "2.0",
                            "id": body["id"],
                            "result": {"content": [{"type": "text", "text": "ok"}]}
                        }))
                    },
                ),
            )
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock ChatOS");
        let address = listener.local_addr().expect("mock address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock ChatOS");
        });
        let provider = ChatosProvider::new(
            format!("http://{address}"),
            Duration::from_secs(5),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Some("chatos-provider-secret".to_string()),
            1024 * 1024,
        )
        .expect("provider");
        let outcome = provider
            .call_tool(
                &snapshot(),
                &route(SystemMcpKey::AskUser, CHATOS_PROVIDER_REF.to_string()),
                "prompt_choices",
                json!({
                    "title": "Continue?",
                    "options": [{"label": "Yes", "value": "yes"}]
                }),
                "invocation-1",
            )
            .await
            .expect("provider call");
        assert_eq!(outcome.result["content"][0]["text"], "ok");

        let (system_key, headers, body) = captured
            .0
            .lock()
            .expect("captured request")
            .clone()
            .expect("request was captured");
        assert_eq!(system_key, SystemMcpKey::AskUser.as_str());
        assert_eq!(headers["x-chatos-caller"], CALLER_SERVICE);
        assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
        assert_eq!(
            headers["x-mcp-management-agent-key"],
            "chatos_conversation_agent"
        );
        assert_eq!(headers["x-mcp-management-session-id"], "session-1");
        assert_eq!(headers["x-mcp-management-project-id"], "project-1");
        assert_eq!(headers["x-mcp-management-turn-id"], "turn-1");
        assert_eq!(
            headers["x-mcp-management-source-session-id"],
            "conversation-1"
        );
        assert_eq!(
            headers["x-mcp-management-source-user-message-id"],
            "message-1"
        );
        let token = headers["x-chatos-internal-token"]
            .to_str()
            .expect("signed token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "chatos-provider-secret",
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            CHATOS_MCP_SCOPE,
        )
        .expect("valid signed token");
        assert_eq!(body["params"]["name"], "prompt_choices");
        assert!(body["params"]["arguments"].get("conversation_id").is_none());
        assert!(body["params"]["arguments"]
            .get("conversation_turn_id")
            .is_none());

        let outcome = provider
            .call_tool(
                &snapshot(),
                &route(SystemMcpKey::Notepad, CHATOS_PROVIDER_REF.to_string()),
                "create_note",
                json!({"title": "Gateway note", "content": "bound to owner"}),
                "invocation-notepad",
            )
            .await
            .expect("notepad provider call");
        assert_eq!(outcome.result["content"][0]["text"], "ok");
        let (system_key, headers, body) = captured
            .0
            .lock()
            .expect("captured notepad request")
            .clone()
            .expect("notepad request was captured");
        assert_eq!(system_key, SystemMcpKey::Notepad.as_str());
        assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
        assert_eq!(body["params"]["name"], "create_note");
        assert!(body["params"]["arguments"].get("user_id").is_none());
        assert!(body["params"]["arguments"].get("owner_user_id").is_none());

        let outcome = provider
            .call_tool(
                &snapshot(),
                &route(SystemMcpKey::AgentBuilder, CHATOS_PROVIDER_REF.to_string()),
                "create_memory_agent",
                json!({"name": "Owner agent", "role_definition": "Owner scoped"}),
                "invocation-agent-builder",
            )
            .await
            .expect("agent builder provider call");
        assert_eq!(outcome.result["content"][0]["text"], "ok");
        let (system_key, headers, body) = captured
            .0
            .lock()
            .expect("captured agent builder request")
            .clone()
            .expect("agent builder request was captured");
        assert_eq!(system_key, SystemMcpKey::AgentBuilder.as_str());
        assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
        assert_eq!(body["params"]["name"], "create_memory_agent");
        assert!(body["params"]["arguments"].get("user_id").is_none());

        let outcome = provider
            .call_tool(
                &snapshot(),
                &route(SystemMcpKey::BrowserTools, CHATOS_PROVIDER_REF.to_string()),
                "browser_navigate",
                json!({"url": "https://example.com"}),
                "invocation-browser",
            )
            .await
            .expect("browser provider call");
        assert_eq!(outcome.result["content"][0]["text"], "ok");
        let (system_key, headers, body) = captured
            .0
            .lock()
            .expect("captured browser request")
            .clone()
            .expect("browser request was captured");
        assert_eq!(system_key, SystemMcpKey::BrowserTools.as_str());
        assert_eq!(headers["x-mcp-management-session-id"], "session-1");
        assert_eq!(
            headers["x-mcp-management-source-session-id"],
            "conversation-1"
        );
        assert_eq!(body["params"]["name"], "browser_navigate");
        assert!(body["params"]["arguments"].get("session_id").is_none());

        let outcome = provider
            .call_tool(
                &snapshot(),
                &route(
                    SystemMcpKey::MemorySkillReader,
                    memory_provider_ref("contact-agent-1"),
                ),
                "get_skill_detail",
                json!({"skill_ref": "SK1"}),
                "invocation-2",
            )
            .await
            .expect("memory provider call");
        assert_eq!(outcome.result["content"][0]["text"], "ok");
        let (system_key, headers, body) = captured
            .0
            .lock()
            .expect("captured memory request")
            .clone()
            .expect("memory request was captured");
        assert_eq!(system_key, SystemMcpKey::MemorySkillReader.as_str());
        assert_eq!(
            headers["x-mcp-management-contact-agent-id"],
            "contact-agent-1"
        );
        assert_eq!(body["params"]["name"], "get_skill_detail");
        assert!(body["params"]["arguments"].get("agent_id").is_none());
    }

    #[tokio::test]
    async fn prepare_routes_materializes_the_live_chatos_browser_catalog() {
        let captured = CapturedRequest::default();
        let app = Router::new()
            .route(
                "/internal/mcp-management/mcp/{system_key}",
                post(
                    |State(captured): State<CapturedRequest>,
                     Path(system_key): Path<String>,
                     headers: HeaderMap,
                     Json(body): Json<Value>| async move {
                        *captured.0.lock().expect("capture tools/list request") =
                            Some((system_key, headers, body.clone()));
                        Json(json!({
                            "jsonrpc": "2.0",
                            "id": body["id"],
                            "result": {
                                "tools": [
                                    {
                                        "name": "browser_navigate",
                                        "description": "Navigate",
                                        "inputSchema": {"type": "object"}
                                    },
                                    {
                                        "name": "browser_snapshot",
                                        "description": "Snapshot",
                                        "inputSchema": {"type": "object"}
                                    }
                                ]
                            }
                        }))
                    },
                ),
            )
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock ChatOS tools/list endpoint");
        let address = listener.local_addr().expect("mock tools/list address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve mock ChatOS tools/list endpoint");
        });
        let provider = ChatosProvider::new(
            format!("http://{address}"),
            Duration::from_secs(5),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Some("chatos-provider-secret".to_string()),
            1024 * 1024,
        )
        .expect("provider");
        let mut routes = vec![route(
            SystemMcpKey::BrowserTools,
            CHATOS_PROVIDER_REF.to_string(),
        )];

        let snapshots = provider
            .prepare_routes(
                routes.as_mut_slice(),
                "session-1",
                "user-1",
                SystemAgentKey::ChatosConversationAgent,
                "project-1",
                Some("conversation-1"),
                i64::MAX,
            )
            .await;

        let tools = snapshots
            .get("builtin_browser_tools")
            .expect("live browser snapshot");
        assert_eq!(tools.len(), 2);
        assert_eq!(routes[0].provider_kind, McpProviderKind::InternalService);
        let (system_key, headers, body) = captured
            .0
            .lock()
            .expect("captured tools/list")
            .clone()
            .expect("tools/list request was captured");
        assert_eq!(system_key, SystemMcpKey::BrowserTools.as_str());
        assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
        assert_eq!(
            headers["x-mcp-management-source-session-id"],
            "conversation-1"
        );
        assert_eq!(body["method"], METHOD_TOOLS_LIST);
    }

    #[tokio::test]
    async fn prepare_routes_marks_cloud_browser_unavailable_without_source_session() {
        let provider = ChatosProvider::new(
            "http://127.0.0.1:3997",
            Duration::from_secs(5),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Some("secret".to_string()),
            1024,
        )
        .expect("provider");
        let mut routes = vec![route(
            SystemMcpKey::BrowserTools,
            CHATOS_PROVIDER_REF.to_string(),
        )];

        let snapshots = provider
            .prepare_routes(
                routes.as_mut_slice(),
                "session-1",
                "user-1",
                SystemAgentKey::ChatosConversationAgent,
                "project-1",
                None,
                i64::MAX,
            )
            .await;

        assert!(snapshots.is_empty());
        assert_eq!(routes[0].provider_kind, McpProviderKind::Unavailable);
        assert!(routes[0].reason.contains("source_session_id"));
    }

    #[tokio::test]
    async fn close_session_releases_the_bound_chatos_browser_runtime() {
        let captured = CapturedRequest::default();
        let app = Router::new()
            .route(
                "/internal/mcp-management/mcp/browser_tools/sessions/{session_id}/close",
                post(
                    |State(captured): State<CapturedRequest>,
                     Path(session_id): Path<String>,
                     headers: HeaderMap,
                     Json(body): Json<Value>| async move {
                        *captured.0.lock().expect("capture close request") =
                            Some((session_id, headers, body.clone()));
                        Json(json!({
                            "jsonrpc": "2.0",
                            "id": body["id"],
                            "result": {"closed": true}
                        }))
                    },
                ),
            )
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock ChatOS close endpoint");
        let address = listener.local_addr().expect("mock close address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve mock ChatOS close endpoint");
        });
        let provider = ChatosProvider::new(
            format!("http://{address}"),
            Duration::from_secs(5),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Some("chatos-provider-secret".to_string()),
            1024 * 1024,
        )
        .expect("provider");
        let mut snapshot = snapshot();
        snapshot.routes = vec![route(
            SystemMcpKey::BrowserTools,
            CHATOS_PROVIDER_REF.to_string(),
        )];

        provider
            .close_session(&snapshot)
            .await
            .expect("close browser runtime");

        let (session_id, headers, body) = captured
            .0
            .lock()
            .expect("captured close request")
            .clone()
            .expect("close request was captured");
        assert_eq!(session_id, "session-1");
        assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
        assert_eq!(
            headers["x-mcp-management-source-session-id"],
            "conversation-1"
        );
        assert_eq!(body["method"], CLOUD_BROWSER_SESSION_CLOSE_METHOD);
    }

    #[test]
    fn provider_only_supports_chatos_owned_routes() {
        let provider = ChatosProvider::new(
            "http://127.0.0.1:3997",
            Duration::from_secs(5),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Some("secret".to_string()),
            1024,
        )
        .expect("provider");
        let ask_user = route(SystemMcpKey::AskUser, CHATOS_PROVIDER_REF.to_string());
        assert!(provider.supports(&ask_user));
        assert!(provider.supports(&route(
            SystemMcpKey::AgentBuilder,
            CHATOS_PROVIDER_REF.to_string(),
        )));
        assert!(provider.supports(&route(
            SystemMcpKey::BrowserTools,
            CHATOS_PROVIDER_REF.to_string(),
        )));
        assert!(provider.supports(&route(
            SystemMcpKey::Notepad,
            CHATOS_PROVIDER_REF.to_string(),
        )));
        assert!(provider.supports(&route(
            SystemMcpKey::MemoryPluginReader,
            memory_provider_ref("contact-agent-1"),
        )));
        let mut wrong_owner = ask_user.clone();
        wrong_owner.provider_ref = Some("task-runner".to_string());
        assert!(!provider.supports(&wrong_owner));
        let mut wrong_kind = ask_user;
        wrong_kind.provider_kind = McpProviderKind::Harness;
        assert!(!provider.supports(&wrong_kind));
    }

    fn route(key: SystemMcpKey, provider_ref: String) -> ResolvedMcpRoute {
        let descriptor = chatos_mcp::system_mcp_descriptor(key);
        ResolvedMcpRoute {
            resource_id: descriptor.resource_id.to_string(),
            server_name: descriptor.server_name.to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some(provider_ref),
            tool_namespace: descriptor.server_name.to_string(),
            allow_writes: descriptor.allow_writes,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: false,
            reason: "test".to_string(),
        }
    }

    fn snapshot() -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "chatos".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "chatos_conversation_agent".to_string(),
            project_id: "project-1".to_string(),
            run_id: None,
            turn_id: Some("turn-1".to_string()),
            task_id: None,
            source_session_id: Some("conversation-1".to_string()),
            source_user_message_id: Some("message-1".to_string()),
            contact_agent_id: Some("contact-agent-1".to_string()),
            default_model_config_id: Some("model-1".to_string()),
            expected_project_task_ids: Vec::new(),
            sandbox_target: None,
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                execution_plane: ExecutionPlane::Cloud,
                workspace_provider: WorkspaceProviderKind::Harness,
                workspace: None,
                sandbox_provider: SandboxProviderKind::None,
                sandbox_pairing_id: None,
                source_type: Some("cloud".to_string()),
                revision: "project-revision".to_string(),
            },
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: Vec::new(),
            tools: Vec::new(),
            plugin_mcp_bindings: Default::default(),
            plugin_local_bindings: Default::default(),
            plugin_tool_component_bindings: Default::default(),
            plugin_local_tool_component_bindings: Default::default(),
            plugin_cloud_tool_component_bindings: Default::default(),
            external_http_bindings: Default::default(),
            cloud_stdio_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: i64::MAX,
        }
    }
}
