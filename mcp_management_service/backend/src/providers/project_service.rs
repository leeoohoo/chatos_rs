// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::{
    builtin_kind_header_value, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER, MCP_ERROR_AUTH_REQUIRED,
    MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS, METHOD_NOTIFICATIONS_CANCELLED,
    METHOD_TOOLS_CALL,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::{decode_cancel_notification_response, ProviderCancelOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "project-service";
const PROJECT_MCP_SCOPE: &str = "project.mcp";
const PROJECT_READ_SCOPE: &str = "project.read";
const PROJECT_HARNESS_SCOPE: &str = "project.harness";
const PROJECT_ENVIRONMENT_SCOPE: &str = "project.environment";
const PROJECT_MANAGEMENT_OWNER_SERVICE: &str = "project_management_service";

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderCallOutcome {
    pub result: Value,
    pub response_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCallError {
    pub code: i32,
    pub message: String,
}

impl ProviderCallError {
    pub fn provider_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: MCP_ERROR_INTERNAL,
            message: message.into(),
        }
    }

    pub(super) fn invalid_response(message: impl Into<String>) -> Self {
        Self {
            code: MCP_ERROR_INTERNAL,
            message: message.into(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ProjectServiceProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    response_limit_bytes: usize,
}

impl ProjectServiceProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("project service Provider base URL is invalid: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("project service Provider base URL must use http or https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if self.internal_secret.is_none() {
            return false;
        }
        let Some(descriptor) = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
        else {
            return false;
        };
        match route.provider_kind {
            McpProviderKind::Harness => matches!(
                descriptor.key,
                SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite
            ),
            McpProviderKind::InternalService
                if route.provider_ref.as_deref() == Some(PROJECT_MANAGEMENT_OWNER_SERVICE) =>
            {
                matches!(
                    descriptor.key,
                    SystemMcpKey::ProjectManagement
                        | SystemMcpKey::ProjectEnvironment
                        | SystemMcpKey::ProjectRuntimeEnvironment
                )
            }
            _ => false,
        }
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
                "project service Provider internal secret is not configured",
            )
        })?;
        let (url, scope) = self.endpoint(snapshot, route)?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut request = self
            .http
            .post(url)
            .header("x-project-service-caller", CALLER_SERVICE)
            .header("x-project-service-internal-token", token)
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-agent-key", snapshot.agent_key.as_str())
            .header("x-mcp-management-session-id", snapshot.session_id.as_str())
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header("x-chatos-project-id", snapshot.project_id.as_str())
            .header("x-task-runner-project-id", snapshot.project_id.as_str());
        if route.provider_kind == McpProviderKind::Harness {
            let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "Harness route is not a registered System MCP",
                    )
                })?;
            request = request.header(
                HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
                builtin_kind_header_value([descriptor.key.as_str()]),
            );
        }
        for (header, value) in [
            ("x-mcp-management-run-id", snapshot.run_id.as_deref()),
            ("x-mcp-management-turn-id", snapshot.turn_id.as_deref()),
            ("x-mcp-management-task-id", snapshot.task_id.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        let response = request
            .with_internal_trace_context()
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
                    "project service Provider request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "project service Provider response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "project service Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result =
            decode_jsonrpc_response(bytes.as_slice(), invocation_id, "project service Provider")?;
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
                "project service Provider internal secret is not configured",
            )
        })?;
        let (url, scope) = self.endpoint(snapshot, route)?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut request = self
            .http
            .post(url)
            .header("x-project-service-caller", CALLER_SERVICE)
            .header("x-project-service-internal-token", token)
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-agent-key", snapshot.agent_key.as_str())
            .header("x-mcp-management-session-id", snapshot.session_id.as_str())
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header("x-chatos-project-id", snapshot.project_id.as_str())
            .header("x-task-runner-project-id", snapshot.project_id.as_str());
        for (header, value) in [
            ("x-mcp-management-run-id", snapshot.run_id.as_deref()),
            ("x-mcp-management-turn-id", snapshot.turn_id.as_deref()),
            ("x-mcp-management-task-id", snapshot.task_id.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        let response = request
            .with_internal_trace_context()
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
                    "project service Provider cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "project service Provider cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "project service Provider")
    }

    fn endpoint(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<(String, &'static str), ProviderCallError> {
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "project service route is not a registered System MCP",
                )
            })?;
        let project_id = urlencoding::encode(snapshot.project_id.trim());
        match (route.provider_kind, descriptor.key) {
            (McpProviderKind::InternalService, SystemMcpKey::ProjectManagement) => {
                Ok((format!("{}/mcp", self.base_url), PROJECT_MCP_SCOPE))
            }
            (McpProviderKind::InternalService, SystemMcpKey::ProjectEnvironment) => Ok((
                format!(
                    "{}/api/internal/projects/{project_id}/environment-agent/mcp",
                    self.base_url
                ),
                PROJECT_ENVIRONMENT_SCOPE,
            )),
            (McpProviderKind::InternalService, SystemMcpKey::ProjectRuntimeEnvironment) => Ok((
                format!(
                    "{}/api/chatos-sync/projects/{project_id}/runtime-environment/mcp",
                    self.base_url
                ),
                PROJECT_READ_SCOPE,
            )),
            (
                McpProviderKind::Harness,
                SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite,
            ) => Ok((
                format!(
                    "{}/api/chatos-sync/projects/{project_id}/harness/mcp",
                    self.base_url
                ),
                PROJECT_HARNESS_SCOPE,
            )),
            _ => Err(ProviderCallError::provider_unavailable(
                "project service Provider does not support this route",
            )),
        }
    }
}

pub(super) fn decode_jsonrpc_response(
    bytes: &[u8],
    invocation_id: &str,
    provider_label: &str,
) -> Result<Value, ProviderCallError> {
    let envelope = serde_json::from_slice::<Value>(bytes).map_err(|err| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} returned invalid JSON: {err}"
        ))
    })?;
    let object = envelope.as_object().ok_or_else(|| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} returned a non-object JSON-RPC response"
        ))
    })?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(ProviderCallError::invalid_response(format!(
            "{provider_label} returned an invalid JSON-RPC version"
        )));
    }
    if object.get("id").and_then(Value::as_str) != Some(invocation_id) {
        return Err(ProviderCallError::invalid_response(format!(
            "{provider_label} response id does not match the invocation"
        )));
    }
    if let Some(error) = object.get("error").filter(|value| !value.is_null()) {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(MCP_ERROR_INTERNAL);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Provider returned an MCP error");
        return Err(ProviderCallError {
            code: match code {
                MCP_ERROR_AUTH_REQUIRED | MCP_ERROR_INVALID_PARAMS | MCP_ERROR_INTERNAL => code,
                _ => MCP_ERROR_INTERNAL,
            },
            message: message.to_string(),
        });
    }
    object.get("result").cloned().ok_or_else(|| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} response is missing result and error"
        ))
    })
}

#[cfg(test)]
mod tests {
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
        WorkspaceProviderKind,
    };

    use super::*;

    fn snapshot() -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            task_profile: Some("default".to_string()),
            project_id: "project-1".to_string(),
            device_id: None,
            run_id: Some("run-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            contact_agent_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: None,
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                execution_plane: ExecutionPlane::Cloud,
                workspace_provider: WorkspaceProviderKind::Harness,
                workspace: None,
                sandbox_provider: SandboxProviderKind::Cloud,
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

    fn project_management_route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "builtin_project_management".to_string(),
            server_name: "project_management_service".to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some(PROJECT_MANAGEMENT_OWNER_SERVICE.to_string()),
            tool_namespace: "project_management_service".to_string(),
            allow_writes: true,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    fn project_environment_route() -> ResolvedMcpRoute {
        let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::ProjectEnvironment);
        ResolvedMcpRoute {
            resource_id: descriptor.resource_id.to_string(),
            server_name: descriptor.server_name.to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some(PROJECT_MANAGEMENT_OWNER_SERVICE.to_string()),
            tool_namespace: descriptor.server_name.to_string(),
            allow_writes: true,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    async fn start_project_service(secret: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State(secret): State<&'static str>,
            headers: HeaderMap,
            Json(request): Json<Value>,
        ) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-project-service-caller")
                    .and_then(|value| value.to_str().ok()),
                Some(CALLER_SERVICE)
            );
            assert_eq!(
                headers
                    .get("x-mcp-management-owner-user-id")
                    .and_then(|value| value.to_str().ok()),
                Some("user-1")
            );
            let token = headers
                .get("x-project-service-internal-token")
                .and_then(|value| value.to_str().ok())
                .expect("signed project service token");
            chatos_service_runtime::verify_internal_service_token(
                token,
                secret,
                CALLER_SERVICE,
                TOKEN_AUDIENCE,
                PROJECT_MCP_SCOPE,
            )
            .expect("valid project service token");
            assert!(headers.get("x-project-service-sync-secret").is_none());
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": {
                    "forwarded_name": request.pointer("/params/name"),
                    "forwarded_arguments": request.pointer("/params/arguments"),
                    "identity_headers": {
                        "owner_user_id": headers.get("x-mcp-management-owner-user-id").and_then(|value| value.to_str().ok()),
                        "agent_key": headers.get("x-mcp-management-agent-key").and_then(|value| value.to_str().ok()),
                        "session_id": headers.get("x-mcp-management-session-id").and_then(|value| value.to_str().ok()),
                        "project_id": headers.get("x-mcp-management-project-id").and_then(|value| value.to_str().ok()),
                        "run_id": headers.get("x-mcp-management-run-id").and_then(|value| value.to_str().ok()),
                        "turn_id": headers.get("x-mcp-management-turn-id").and_then(|value| value.to_str().ok()),
                        "task_id": headers.get("x-mcp-management-task-id").and_then(|value| value.to_str().ok())
                    }
                }
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/mcp", post(handler))
            .with_state(secret);
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), handle)
    }

    #[test]
    fn downstream_jsonrpc_response_is_bound_to_invocation_id() {
        let valid = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": "invocation-1",
            "result": {"ok": true}
        }))
        .unwrap();
        assert_eq!(
            decode_jsonrpc_response(valid.as_slice(), "invocation-1", "test Provider").unwrap(),
            json!({"ok": true})
        );
        assert!(
            decode_jsonrpc_response(valid.as_slice(), "invocation-2", "test Provider").is_err()
        );
    }

    #[test]
    fn downstream_jsonrpc_errors_are_normalized() {
        let response = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": "invocation-1",
            "error": {"code": -32001, "message": "denied"}
        }))
        .unwrap();
        assert_eq!(
            decode_jsonrpc_response(response.as_slice(), "invocation-1", "test Provider")
                .unwrap_err(),
            ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "denied".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn project_management_call_uses_frozen_snapshot_identity_and_original_tool_name() {
        const SECRET: &str = "a-long-project-service-secret";
        let (base_url, server) = start_project_service(SECRET).await;
        let provider = ProjectServiceProvider::new(
            reqwest::Client::new(),
            base_url,
            Some(SECRET.to_string()),
            1024 * 1024,
        )
        .unwrap();
        let route = project_management_route();
        assert!(provider.supports(&route));
        let outcome = provider
            .call_tool(
                &snapshot(),
                &route,
                "list_requirements",
                json!({
                    "status": "draft",
                    "owner_user_id": "forged-owner",
                    "agent_key": "forged-agent",
                    "session_id": "forged-session",
                    "project_id": "forged-project",
                    "run_id": "forged-run",
                    "turn_id": "forged-turn",
                    "task_id": "forged-task"
                }),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(
            outcome.result,
            json!({
                "forwarded_name": "list_requirements",
                "forwarded_arguments": {
                    "status": "draft",
                    "owner_user_id": "forged-owner",
                    "agent_key": "forged-agent",
                    "session_id": "forged-session",
                    "project_id": "forged-project",
                    "run_id": "forged-run",
                    "turn_id": "forged-turn",
                    "task_id": "forged-task"
                },
                "identity_headers": {
                    "owner_user_id": "user-1",
                    "agent_key": "task_runner_run_phase",
                    "session_id": "session-1",
                    "project_id": "project-1",
                    "run_id": "run-1",
                    "turn_id": "turn-1",
                    "task_id": "task-1"
                }
            })
        );
        assert!(outcome.response_bytes > 0);
        server.abort();
    }

    #[test]
    fn harness_route_uses_the_project_scoped_harness_endpoint() {
        let provider = ProjectServiceProvider::new(
            reqwest::Client::new(),
            "http://127.0.0.1:39210",
            Some("a-long-project-service-secret".to_string()),
            1024 * 1024,
        )
        .unwrap();
        let route = ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::Harness,
            provider_ref: Some("project:project-1@revision".to_string()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        };
        let (url, scope) = provider.endpoint(&snapshot(), &route).unwrap();
        assert_eq!(scope, PROJECT_HARNESS_SCOPE);
        assert!(url.ends_with("/api/chatos-sync/projects/project-1/harness/mcp"));
    }

    #[test]
    fn project_environment_route_uses_the_run_bound_internal_endpoint() {
        let provider = ProjectServiceProvider::new(
            reqwest::Client::new(),
            "http://127.0.0.1:39210",
            Some("a-long-project-service-secret".to_string()),
            1024 * 1024,
        )
        .unwrap();
        let route = project_environment_route();
        assert!(provider.supports(&route));
        let (url, scope) = provider.endpoint(&snapshot(), &route).unwrap();
        assert_eq!(scope, PROJECT_ENVIRONMENT_SCOPE);
        assert!(url.ends_with("/api/internal/projects/project-1/environment-agent/mcp"));
    }
}
