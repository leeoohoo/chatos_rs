// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::METHOD_TOOLS_CALL;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::project_service::decode_jsonrpc_response;
use super::{ProviderCallError, ProviderCallOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "chatos";
const CHATOS_MCP_SCOPE: &str = "mcp.tools.call";
const CHATOS_ASK_USER_PROVIDER_REF: &str = "chatos";

#[derive(Clone)]
pub(super) struct ChatosProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    ask_user_request_timeout: Duration,
    response_limit_bytes: usize,
}

impl ChatosProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        ask_user_request_timeout: Duration,
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
            .timeout(ask_user_request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|err| format!("build ChatOS Provider client failed: {err}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            ask_user_request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        self.internal_secret.is_some()
            && route.provider_kind == McpProviderKind::InternalService
            && route.provider_ref.as_deref() == Some(CHATOS_ASK_USER_PROVIDER_REF)
            && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                .is_some_and(|descriptor| descriptor.key == SystemMcpKey::AskUser)
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
            .filter(|descriptor| descriptor.key == SystemMcpKey::AskUser && self.supports(route))
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "ChatOS route is not a supported System MCP",
                )
            })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            CHATOS_MCP_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let mut request = self
            .http
            .post(endpoint)
            .header("x-chatos-caller", CALLER_SERVICE)
            .header("x-chatos-internal-token", token)
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-agent-key", snapshot.agent_key.as_str())
            .header("x-mcp-management-session-id", snapshot.session_id.as_str())
            .header(
                "x-mcp-management-session-expires-at-unix",
                snapshot.expires_at_unix.to_string(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .timeout(self.ask_user_request_timeout);
        for (header, value) in [
            ("x-mcp-management-run-id", snapshot.run_id.as_deref()),
            ("x-mcp-management-turn-id", snapshot.turn_id.as_deref()),
            ("x-mcp-management-task-id", snapshot.task_id.as_deref()),
            (
                "x-mcp-management-source-session-id",
                snapshot.source_session_id.as_deref(),
            ),
            (
                "x-mcp-management-source-user-message-id",
                snapshot.source_user_message_id.as_deref(),
            ),
            (
                "x-mcp-management-default-model-config-id",
                snapshot.default_model_config_id.as_deref(),
            ),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        if !snapshot.expected_project_task_ids.is_empty() {
            request = request.header(
                "x-mcp-management-expected-project-task-ids",
                snapshot.expected_project_task_ids.join(","),
            );
        }
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
            Duration::from_secs(60),
            Some("chatos-provider-secret".to_string()),
            1024 * 1024,
        )
        .expect("provider");
        let outcome = provider
            .call_tool(
                &snapshot(),
                &route(),
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
    }

    #[test]
    fn provider_only_supports_chatos_owned_ask_user_route() {
        let provider = ChatosProvider::new(
            "http://127.0.0.1:3997",
            Duration::from_secs(60),
            Some("secret".to_string()),
            1024,
        )
        .expect("provider");
        assert!(provider.supports(&route()));
        let mut wrong_owner = route();
        wrong_owner.provider_ref = Some("task-runner".to_string());
        assert!(!provider.supports(&wrong_owner));
        let mut wrong_kind = route();
        wrong_kind.provider_kind = McpProviderKind::Harness;
        assert!(!provider.supports(&wrong_kind));
    }

    fn route() -> ResolvedMcpRoute {
        let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser);
        ResolvedMcpRoute {
            resource_id: descriptor.resource_id.to_string(),
            server_name: descriptor.server_name.to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some(CHATOS_ASK_USER_PROVIDER_REF.to_string()),
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
            external_http_bindings: Default::default(),
            cloud_stdio_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: i64::MAX,
        }
    }
}
