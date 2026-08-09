// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde::Deserialize;

use super::project_service::decode_jsonrpc_response;
use super::ProviderCallError;

mod manager_client;
mod runtime_calls;
mod validation;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";

#[derive(Clone)]
pub(super) struct CloudSandboxProvider {
    http: reqwest::Client,
    base_url: String,
    request_timeout: Duration,
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
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("Sandbox Manager Provider base URL is invalid: {err}"))?;
        if !cfg!(test) && parsed.scheme() != "https" {
            return Err("Sandbox Manager Provider base URL must use https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            request_timeout,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxExecutionTarget,
        SandboxProviderKind, WorkspaceProviderKind,
    };
    use serde_json::{json, Value};

    use super::validation::{cloud_sandbox_call_timeout, validate_lease_binding};
    use crate::runtime::RuntimeSessionSnapshot;

    fn target() -> SandboxExecutionTarget {
        SandboxExecutionTarget {
            provider: SandboxProviderKind::Cloud,
            pairing_id: None,
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

    #[test]
    fn terminal_wait_call_timeout_follows_requested_wait_budget() {
        let default_timeout = Duration::from_secs(180);
        assert_eq!(
            cloud_sandbox_call_timeout(
                "process_wait",
                &json!({"timeout_ms": 600_000}),
                default_timeout,
            ),
            Duration::from_millis(615_000)
        );
        assert_eq!(
            cloud_sandbox_call_timeout(
                "process",
                &json!({"action": "wait", "timeout": 600}),
                default_timeout,
            ),
            Duration::from_millis(615_000)
        );
        assert_eq!(
            cloud_sandbox_call_timeout("process_poll", &json!({}), default_timeout),
            default_timeout
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
                    .route("/api/internal/sandboxes/sandbox-1", get(lease))
                    .route("/api/internal/sandboxes/sandbox-1/mcp", post(mcp)),
            )
            .await
            .unwrap();
        });
        let provider = CloudSandboxProvider::new(
            reqwest::Client::new(),
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
            trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
                .as_str()
                .to_string(),
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
