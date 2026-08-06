// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, SandboxExecutionTarget,
    SandboxProviderKind, WorkspaceProviderKind,
};
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
const TOKEN_AUDIENCE: &str = "local-connector-service";
const SANDBOX_ROUTING_SCOPE: &str = "sandbox-routing.read";
const SANDBOX_SERVICE_SCOPE: &str = "sandbox.service";

#[derive(Clone)]
pub(super) struct LocalSandboxProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    response_limit_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct SandboxPairingRecord {
    id: String,
    device_id: String,
    workspace_id: String,
    enabled: bool,
    #[serde(default)]
    sandbox_readiness: String,
}

#[derive(Debug, Deserialize)]
struct LocalSandboxLeaseBinding {
    id: String,
    sandbox_id: String,
    tenant_id: String,
    project_id: String,
    run_id: String,
    status: String,
}

impl LocalSandboxProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Local Sandbox Provider base URL is invalid: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Local Sandbox Provider base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|error| format!("build Local Sandbox Provider client failed: {error}"))?;
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
        if self.internal_secret.is_none()
            || route.provider_kind != McpProviderKind::LocalConnector
            || !route
                .provider_ref
                .as_deref()
                .is_some_and(|value| value.starts_with("sandbox-pairing:"))
        {
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

    pub(super) async fn resolve_active_pairing(
        &self,
        context: &ProjectExecutionContext,
    ) -> Result<Option<String>, ProviderCallError> {
        if context.sandbox_provider != SandboxProviderKind::LocalConnector {
            return Ok(None);
        }
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox requires a Local Connector workspace",
            ));
        }
        let workspace = context.workspace.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Sandbox Project Context is missing its workspace target",
            )
        })?;
        let device_id = workspace
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Local Sandbox Project Context is missing its device id",
                )
            })?;
        let workspace_id = workspace.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox Project Context is missing its workspace id",
            ));
        }
        let mut url = reqwest::Url::parse(
            format!("{}/api/local-connectors/sandbox-pairings", self.base_url).as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Local Sandbox pairing URL failed: {error}"
            ))
        })?;
        url.query_pairs_mut()
            .append_pair("active_only", "true")
            .append_pair("device_id", device_id)
            .append_pair("workspace_id", workspace_id);
        let response = self
            .authenticated(
                self.http.get(url),
                SANDBOX_ROUTING_SCOPE,
                context.owner_user_id.as_str(),
            )?
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox pairing request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox pairing response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox pairing request returned HTTP {}",
                status.as_u16()
            )));
        }
        let pairings = serde_json::from_slice::<Vec<SandboxPairingRecord>>(bytes.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox pairing response is invalid: {error}"
                ))
            })?;
        Ok(pairings
            .into_iter()
            .find(|pairing| {
                pairing.enabled
                    && pairing.device_id == device_id
                    && pairing.workspace_id == workspace_id
                    && pairing
                        .sandbox_readiness
                        .trim()
                        .eq_ignore_ascii_case("ready")
            })
            .map(|pairing| pairing.id))
    }

    pub(super) async fn validate_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        if target.provider != SandboxProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox target has the wrong provider",
            ));
        }
        let pairing_id = required_pairing_id(target)?;
        let run_id = run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Local Sandbox route requires a concrete run_id",
                )
            })?;
        let response = self
            .authenticated(
                self.http
                    .get(self.sandbox_url(pairing_id, target.sandbox_id.as_str(), None)),
                SANDBOX_SERVICE_SCOPE,
                owner_user_id,
            )?
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox lease validation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox lease response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox rejected lease validation with HTTP {}",
                status.as_u16()
            )));
        }
        let binding = serde_json::from_slice::<LocalSandboxLeaseBinding>(bytes.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox returned an invalid lease record: {error}"
                ))
            })?;
        if binding.id != target.lease_id
            || binding.sandbox_id != target.sandbox_id
            || binding.tenant_id != owner_user_id.trim()
            || binding.project_id != project_id.trim()
            || binding.run_id != run_id
        {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox lease identity does not match the runtime session",
            ));
        }
        if !matches!(binding.status.as_str(), "ready" | "running") {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox lease is not runnable: {}",
                binding.status
            )));
        }
        Ok(())
    }

    pub(super) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let target = self.validated_snapshot_target(snapshot, route).await?;
        let pairing_id = required_pairing_id(target)?;
        let response = self
            .authenticated(
                self.http.post(self.sandbox_url(
                    pairing_id,
                    target.sandbox_id.as_str(),
                    Some("mcp"),
                )),
                SANDBOX_SERVICE_SCOPE,
                snapshot.owner_user_id.as_str(),
            )?
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
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": {
                    "name": original_tool_name,
                    "arguments": arguments,
                }
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox MCP request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox MCP response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox MCP rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Local Sandbox")?;
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
        let target = self.validated_snapshot_target(snapshot, route).await?;
        let pairing_id = required_pairing_id(target)?;
        let response = self
            .authenticated(
                self.http.post(self.sandbox_url(
                    pairing_id,
                    target.sandbox_id.as_str(),
                    Some("mcp"),
                )),
                SANDBOX_SERVICE_SCOPE,
                snapshot.owner_user_id.as_str(),
            )?
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
                    "Local Sandbox cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Local Sandbox")
    }

    async fn validated_snapshot_target<'a>(
        &self,
        snapshot: &'a RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<&'a SandboxExecutionTarget, ProviderCallError> {
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox Provider does not support this route",
            ));
        }
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "runtime session does not contain a Local Sandbox target",
            )
        })?;
        if target.provider != SandboxProviderKind::LocalConnector
            || route.provider_ref.as_deref() != Some(target.provider_ref().as_str())
            || snapshot.project_context.sandbox_pairing_id.as_deref()
                != target.pairing_id.as_deref()
        {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox route does not match the immutable runtime target",
            ));
        }
        self.validate_target(
            target,
            snapshot.owner_user_id.as_str(),
            snapshot.project_id.as_str(),
            snapshot.run_id.as_deref(),
        )
        .await?;
        Ok(target)
    }

    fn authenticated(
        &self,
        request: reqwest::RequestBuilder,
        scope: &str,
        owner_user_id: &str,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Sandbox Provider internal secret is not configured",
            )
        })?;
        let owner_user_id = owner_user_id.trim();
        if owner_user_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox Provider owner identity is empty",
            ));
        }
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        Ok(request
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id))
    }

    fn sandbox_url(&self, pairing_id: &str, sandbox_id: &str, suffix: Option<&str>) -> String {
        let mut url = format!(
            "{}/api/local-connectors/sandbox-facade/{}/api/sandboxes/{}",
            self.base_url,
            urlencoding::encode(pairing_id.trim()),
            urlencoding::encode(sandbox_id.trim())
        );
        if let Some(suffix) = suffix {
            url.push('/');
            url.push_str(suffix);
        }
        url
    }
}

fn required_pairing_id(target: &SandboxExecutionTarget) -> Result<&str, ProviderCallError> {
    target
        .pairing_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Sandbox target is missing its pairing id",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{ExecutionPlane, McpRetryClass, WorkspaceExecutionTarget};

    fn target() -> SandboxExecutionTarget {
        SandboxExecutionTarget {
            provider: SandboxProviderKind::LocalConnector,
            pairing_id: Some("pairing-1".to_string()),
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        }
    }

    fn project_context() -> ProjectExecutionContext {
        ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some("device-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: None,
            }),
            sandbox_provider: SandboxProviderKind::LocalConnector,
            sandbox_pairing_id: Some("pairing-1".to_string()),
            source_type: Some("local_connector".to_string()),
            revision: "revision-1".to_string(),
        }
    }

    fn snapshot(target: SandboxExecutionTarget) -> RuntimeSessionSnapshot {
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
            turn_id: None,
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            contact_agent_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: Some(target),
            project_context: project_context(),
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

    fn route(target: &SandboxExecutionTarget) -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::LocalConnector,
            provider_ref: Some(target.provider_ref()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn local_sandbox_call_is_pinned_to_pairing_lease_and_runtime_identity() {
        async fn lease(headers: HeaderMap) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-local-connector-caller")
                    .and_then(|value| value.to_str().ok()),
                Some("mcp-management-service")
            );
            assert!(headers.get("x-local-connector-internal-token").is_some());
            Json(json!({
                "id": "lease-1",
                "sandbox_id": "sandbox-1",
                "tenant_id": "user-1",
                "project_id": "project-1",
                "run_id": "run-1",
                "status": "ready"
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
                    .get("x-mcp-management-project-id")
                    .and_then(|value| value.to_str().ok()),
                Some("project-1")
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
                    .route(
                        "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1",
                        get(lease),
                    )
                    .route(
                        "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1/mcp",
                        post(mcp),
                    ),
            )
            .await
            .unwrap();
        });
        let provider = LocalSandboxProvider::new(
            format!("http://{address}"),
            Duration::from_secs(5),
            Some("a-long-local-connector-secret".to_string()),
            1024 * 1024,
        )
        .unwrap();
        let target = target();
        let outcome = provider
            .call_tool(
                &snapshot(target.clone()),
                &route(&target),
                "read_file_raw",
                json!({"path": "README.md"}),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(outcome.result["called"], "read_file_raw");

        assert!(provider
            .validate_target(&target, "user-2", "project-1", Some("run-1"))
            .await
            .is_err());
        server.abort();
    }
}
