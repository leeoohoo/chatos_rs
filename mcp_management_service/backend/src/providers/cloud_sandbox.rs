// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::project_service::decode_jsonrpc_response;
use super::{
    decode_cancel_notification_response, ProviderCallError, ProviderCallOutcome,
    ProviderCancelOutcome,
};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";

#[derive(Clone)]
pub(super) struct CloudSandboxProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    response_limit_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct SandboxLeaseBinding {
    id: String,
    sandbox_id: String,
    tenant_id: String,
    project_id: String,
    run_id: String,
    status: String,
    #[serde(default = "default_lease_kind")]
    lease_kind: String,
    #[serde(default)]
    environment_services: Vec<SandboxEnvironmentServiceBinding>,
}

#[derive(Debug, Deserialize)]
struct SandboxEnvironmentServiceBinding {
    service_id: String,
}

fn default_lease_kind() -> String {
    "sandbox".to_string()
}

impl CloudSandboxProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("Sandbox Manager Provider base URL is invalid: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Sandbox Manager Provider base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|err| format!("build Sandbox Manager Provider client failed: {err}"))?;
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
        if self.internal_secret.is_none() || route.provider_kind != McpProviderKind::CloudSandbox {
            return false;
        }
        system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_some_and(|descriptor| {
            matches!(
                descriptor.key,
                SystemMcpKey::CodeMaintainerRead
                    | SystemMcpKey::CodeMaintainerWrite
                    | SystemMcpKey::TerminalController
            )
        })
    }

    pub(super) async fn validate_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        let run_id = run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Cloud Sandbox route requires a concrete run_id",
                )
            })?;
        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let response = self
            .authenticated(
                self.http
                    .get(format!("{}/api/sandboxes/{sandbox_id}", self.base_url)),
            )?
            .send()
            .await
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "Sandbox Manager lease validation request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Sandbox Manager lease response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Sandbox Manager rejected lease validation with HTTP {}",
                status.as_u16()
            )));
        }
        let record =
            serde_json::from_slice::<SandboxLeaseBinding>(bytes.as_slice()).map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Sandbox Manager returned an invalid lease record: {err}"
                ))
            })?;
        validate_lease_binding(
            &record,
            target,
            owner_user_id.trim(),
            project_id.trim(),
            run_id,
        )
    }

    pub(super) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud Sandbox Provider does not support this route",
            ));
        }
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "runtime session does not contain a Cloud Sandbox target",
            )
        })?;
        if route.provider_ref.as_deref() != Some(target.provider_ref().as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud Sandbox route does not match the immutable runtime target",
            ));
        }
        self.validate_target(
            target,
            snapshot.owner_user_id.as_str(),
            snapshot.project_id.as_str(),
            snapshot.run_id.as_deref(),
        )
        .await?;

        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let path = if target.is_environment {
            format!("/api/sandbox-environments/{sandbox_id}/mcp")
        } else {
            format!("/api/sandboxes/{sandbox_id}/mcp")
        };
        let mut request = self.authenticated(self.http.post(format!("{}{path}", self.base_url)))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        request = request
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header(
                "x-mcp-management-run-id",
                snapshot.run_id.as_deref().unwrap_or_default(),
            );
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
                    "Cloud Sandbox Provider request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Cloud Sandbox Provider response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud Sandbox Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Cloud Sandbox")?;
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
        if !self.supports(route) {
            return Ok(ProviderCancelOutcome::NotSupported);
        }
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "runtime session does not contain a Cloud Sandbox target",
            )
        })?;
        if route.provider_ref.as_deref() != Some(target.provider_ref().as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud Sandbox route does not match the immutable runtime target",
            ));
        }
        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let path = if target.is_environment {
            format!("/api/sandbox-environments/{sandbox_id}/mcp")
        } else {
            format!("/api/sandboxes/{sandbox_id}/mcp")
        };
        let mut request = self.authenticated(self.http.post(format!("{}{path}", self.base_url)))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        let response = request
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header(
                "x-mcp-management-run-id",
                snapshot.run_id.as_deref().unwrap_or_default(),
            )
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
                    "Cloud Sandbox cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud Sandbox cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Cloud Sandbox")
    }

    fn authenticated(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Sandbox Manager Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            INTERNAL_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        Ok(request
            .header("x-sandbox-caller", CALLER_SERVICE)
            .header("x-sandbox-internal-token", token))
    }
}

fn validate_lease_binding(
    record: &SandboxLeaseBinding,
    target: &SandboxExecutionTarget,
    owner_user_id: &str,
    project_id: &str,
    run_id: &str,
) -> Result<(), ProviderCallError> {
    let expected_kind = if target.is_environment {
        "environment"
    } else {
        "sandbox"
    };
    if record.id != target.lease_id
        || record.sandbox_id != target.sandbox_id
        || record.tenant_id != owner_user_id
        || record.project_id != project_id
        || record.run_id != run_id
        || record.lease_kind != expected_kind
    {
        return Err(ProviderCallError::provider_unavailable(
            "Sandbox Manager lease identity does not match the runtime session",
        ));
    }
    if !matches!(record.status.as_str(), "ready" | "running") {
        return Err(ProviderCallError::provider_unavailable(format!(
            "Sandbox Manager lease is not runnable: {}",
            record.status
        )));
    }
    if let Some(service_id) = target.service_id.as_deref() {
        if !record
            .environment_services
            .iter()
            .any(|service| service.service_id == service_id)
        {
            return Err(ProviderCallError::provider_unavailable(
                "Sandbox environment service does not match the runtime session",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
        WorkspaceProviderKind,
    };

    fn target() -> SandboxExecutionTarget {
        SandboxExecutionTarget {
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        }
    }

    fn record() -> SandboxLeaseBinding {
        SandboxLeaseBinding {
            id: "lease-1".to_string(),
            sandbox_id: "sandbox-1".to_string(),
            tenant_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            status: "ready".to_string(),
            lease_kind: "sandbox".to_string(),
            environment_services: Vec::new(),
        }
    }

    #[test]
    fn lease_binding_requires_exact_owner_project_run_and_lease() {
        validate_lease_binding(&record(), &target(), "user-1", "project-1", "run-1").unwrap();
        assert!(
            validate_lease_binding(&record(), &target(), "another-user", "project-1", "run-1")
                .is_err()
        );
    }

    #[tokio::test]
    async fn cloud_sandbox_call_uses_signed_manager_proxy_and_bound_headers() {
        async fn lease(headers: HeaderMap) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-sandbox-caller")
                    .and_then(|value| value.to_str().ok()),
                Some("mcp-management-service")
            );
            assert!(headers.get("x-sandbox-internal-token").is_some());
            Json(json!({
                "id": "lease-1",
                "sandbox_id": "sandbox-1",
                "tenant_id": "user-1",
                "project_id": "project-1",
                "run_id": "run-1",
                "status": "ready",
                "lease_kind": "sandbox",
                "environment_services": []
            }))
        }

        async fn mcp(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-chatos-sandbox-lease-id")
                    .and_then(|value| value.to_str().ok()),
                Some("lease-1")
            );
            assert_eq!(
                headers
                    .get("x-mcp-management-run-id")
                    .and_then(|value| value.to_str().ok()),
                Some("run-1")
            );
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": {"called": request.pointer("/params/name")}
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/api/sandboxes/sandbox-1", get(lease))
                    .route("/api/sandboxes/sandbox-1/mcp", post(mcp)),
            )
            .await
            .unwrap();
        });
        let provider = CloudSandboxProvider::new(
            format!("http://{address}"),
            Duration::from_secs(5),
            Some("a-long-sandbox-secret".to_string()),
            1024 * 1024,
        )
        .unwrap();
        let target = target();
        let snapshot = RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            contact_agent_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: Some(target.clone()),
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                execution_plane: ExecutionPlane::Cloud,
                workspace_provider: WorkspaceProviderKind::CloudSandbox,
                workspace: None,
                sandbox_provider: SandboxProviderKind::Cloud,
                sandbox_pairing_id: None,
                source_type: Some("cloud".to_string()),
                revision: "revision-1".to_string(),
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
        };
        let route = ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::CloudSandbox,
            provider_ref: Some(target.provider_ref()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        };
        let outcome = provider
            .call_tool(
                &snapshot,
                &route,
                "read_file_raw",
                json!({"path": "README.md"}),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(outcome.result["called"], "read_file_raw");
        server.abort();
    }
}
