// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
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
const TOKEN_AUDIENCE: &str = "task-runner";
const TASK_RUNNER_MCP_SCOPE: &str = "mcp.tools.call";
const TASK_RUNNER_OWNER_SERVICE: &str = "task_runner_service";
const TASK_RUNNER_ASK_USER_PROVIDER_REF: &str = "task-runner";

#[derive(Clone)]
pub(super) struct TaskRunnerProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    ask_user_request_timeout: Duration,
    response_limit_bytes: usize,
}

impl TaskRunnerProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        request_timeout: Duration,
        ask_user_request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("Task Runner Provider base URL is invalid: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Task Runner Provider base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|err| format!("build Task Runner Provider client failed: {err}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            ask_user_request_timeout,
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
                SystemMcpKey::TaskRunnerService | SystemMcpKey::TaskProcessLog => {
                    route.provider_ref.as_deref() == Some(TASK_RUNNER_OWNER_SERVICE)
                }
                SystemMcpKey::AskUser => {
                    route.provider_ref.as_deref() == Some(TASK_RUNNER_ASK_USER_PROVIDER_REF)
                }
                _ => false,
            }
        })
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
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|descriptor| {
                matches!(
                    descriptor.key,
                    SystemMcpKey::TaskRunnerService
                        | SystemMcpKey::TaskProcessLog
                        | SystemMcpKey::AskUser
                ) && self.supports(route)
            })
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Task Runner route is not a supported System MCP",
                )
            })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            TASK_RUNNER_MCP_SCOPE,
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
            .header("x-task-runner-caller", CALLER_SERVICE)
            .header("x-task-runner-internal-token", token)
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
            .header("x-chatos-project-id", snapshot.project_id.as_str())
            .timeout(if descriptor.key == SystemMcpKey::AskUser {
                self.ask_user_request_timeout
            } else {
                self.request_timeout
            });
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
                    "Task Runner Provider request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Task Runner Provider response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Task Runner Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result =
            decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Task Runner Provider")?;
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
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|descriptor| {
                matches!(
                    descriptor.key,
                    SystemMcpKey::TaskRunnerService
                        | SystemMcpKey::TaskProcessLog
                        | SystemMcpKey::AskUser
                ) && self.supports(route)
            })
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Task Runner route is not a supported System MCP",
                )
            })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            TASK_RUNNER_MCP_SCOPE,
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
            .header("x-task-runner-caller", CALLER_SERVICE)
            .header("x-task-runner-internal-token", token)
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
            .header("x-chatos-project-id", snapshot.project_id.as_str())
            .timeout(Duration::from_secs(5));
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
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        let response = request
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
                    "Task Runner Provider cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Task Runner Provider cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Task Runner Provider")
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
    async fn provider_uses_signed_service_identity_and_forwards_immutable_session_binding() {
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
            .expect("bind mock Task Runner");
        let address = listener.local_addr().expect("mock address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve mock Task Runner");
        });
        let provider = TaskRunnerProvider::new(
            format!("http://{address}"),
            Duration::from_secs(2),
            Duration::from_secs(60),
            Some("task-runner-provider-secret".to_string()),
            1024 * 1024,
        )
        .expect("provider");
        let task_process_route = route(SystemMcpKey::TaskProcessLog);
        let outcome = provider
            .call_tool(
                &snapshot(),
                &task_process_route,
                "record_process",
                json!({"operation": "append", "content": "verified", "heading": null}),
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
        assert_eq!(system_key, SystemMcpKey::TaskProcessLog.as_str());
        assert_eq!(headers["x-task-runner-caller"], CALLER_SERVICE);
        assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
        assert_eq!(
            headers["x-mcp-management-agent-key"],
            "task_runner_run_phase"
        );
        assert_eq!(headers["x-mcp-management-session-id"], "session-1");
        assert_eq!(
            headers["x-mcp-management-session-expires-at-unix"]
                .to_str()
                .expect("session expiry header"),
            i64::MAX.to_string().as_str()
        );
        assert_eq!(headers["x-mcp-management-project-id"], "project-1");
        assert_eq!(headers["x-mcp-management-run-id"], "run-1");
        assert_eq!(headers["x-mcp-management-task-id"], "task-1");
        assert_eq!(
            headers["x-mcp-management-source-session-id"],
            "source-session-1"
        );
        assert_eq!(
            headers["x-mcp-management-source-user-message-id"],
            "message-1"
        );
        assert_eq!(
            headers["x-mcp-management-expected-project-task-ids"],
            "project-task-1"
        );
        let token = headers["x-task-runner-internal-token"]
            .to_str()
            .expect("signed token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "task-runner-provider-secret",
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            TASK_RUNNER_MCP_SCOPE,
        )
        .expect("valid signed token");
        assert_eq!(body["params"]["name"], "record_process");
        assert!(body["params"]["arguments"].get("task_id").is_none());
        assert!(body["params"]["arguments"].get("run_id").is_none());

        provider
            .call_tool(
                &snapshot(),
                &route(SystemMcpKey::AskUser),
                "prompt_choices",
                json!({
                    "title": "Continue?",
                    "options": [{"label": "Yes", "value": "yes"}]
                }),
                "invocation-ask-user",
            )
            .await
            .expect("Ask User provider call");
        let (system_key, _, body) = captured
            .0
            .lock()
            .expect("captured Ask User request")
            .clone()
            .expect("Ask User request was captured");
        assert_eq!(system_key, SystemMcpKey::AskUser.as_str());
        assert_eq!(body["params"]["name"], "prompt_choices");
        assert!(body["params"]["arguments"].get("task_id").is_none());
        assert!(body["params"]["arguments"].get("run_id").is_none());
    }

    #[test]
    fn provider_supports_task_runner_owned_and_callback_system_mcps() {
        let provider = TaskRunnerProvider::new(
            "http://127.0.0.1:39090",
            Duration::from_secs(2),
            Duration::from_secs(60),
            Some("secret".to_string()),
            1024,
        )
        .expect("provider");
        assert!(provider.supports(&route(SystemMcpKey::TaskRunnerService)));
        assert!(provider.supports(&route(SystemMcpKey::TaskProcessLog)));
        assert!(provider.supports(&route(SystemMcpKey::AskUser)));
        assert!(!provider.supports(&route(SystemMcpKey::ProjectManagement)));
    }

    fn route(key: SystemMcpKey) -> ResolvedMcpRoute {
        let descriptor = chatos_mcp::system_mcp_descriptor(key);
        ResolvedMcpRoute {
            resource_id: descriptor.resource_id.to_string(),
            server_name: descriptor.server_name.to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some(if key == SystemMcpKey::AskUser {
                TASK_RUNNER_ASK_USER_PROVIDER_REF.to_string()
            } else {
                descriptor.owner_service.to_string()
            }),
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
            caller_service: "task-runner".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            task_id: Some("task-1".to_string()),
            source_session_id: Some("source-session-1".to_string()),
            source_user_message_id: Some("message-1".to_string()),
            contact_agent_id: None,
            default_model_config_id: Some("model-1".to_string()),
            expected_project_task_ids: vec!["project-task-1".to_string()],
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
            external_http_bindings: Default::default(),
            cloud_stdio_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: i64::MAX,
        }
    }
}
