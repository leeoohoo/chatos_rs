// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, WorkspaceProviderKind};
use chatos_mcp_service::{
    builtin_kind_header_value, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER, METHOD_TOOLS_CALL,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::project_service::decode_jsonrpc_response;
use super::{ProviderCallError, ProviderCallOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const MCP_RELAY_SCOPE: &str = "relay.mcp";

#[derive(Clone)]
pub(super) struct LocalConnectorProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    response_limit_bytes: usize,
}

impl LocalConnectorProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("Local Connector Provider base URL is invalid: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Local Connector Provider base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|err| format!("build Local Connector Provider client failed: {err}"))?;
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
        if self.internal_secret.is_none() || route.provider_kind != McpProviderKind::LocalConnector
        {
            return false;
        }
        let Some(descriptor) = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
        else {
            return false;
        };
        matches!(
            descriptor.key,
            SystemMcpKey::CodeMaintainerRead
                | SystemMcpKey::CodeMaintainerWrite
                | SystemMcpKey::TerminalController
                | SystemMcpKey::BrowserTools
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
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector Provider internal secret is not configured",
            )
        })?;
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Local Connector Provider does not support this route",
            ));
        }
        if snapshot.project_context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "runtime session is not pinned to a Local Connector workspace",
            ));
        }
        let workspace = snapshot.project_context.workspace.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector route is missing its workspace snapshot",
            )
        })?;
        let device_id = workspace
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Local Connector route is missing its device id",
                )
            })?;
        let workspace_id = workspace.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Local Connector route is missing its workspace id",
            ));
        }
        let expected_provider_ref = format!("device:{device_id}/workspace:{workspace_id}");
        if route.provider_ref.as_deref() != Some(expected_provider_ref.as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "Local Connector route does not match the runtime workspace snapshot",
            ));
        }
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Local Connector route is not a registered System MCP",
                )
            })?;
        let enabled_builtin_kinds = builtin_kind_header_value([descriptor.key.as_str()]);
        if enabled_builtin_kinds.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Local Connector route has no supported builtin capability",
            ));
        }
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/mcp",
                self.base_url,
                urlencoding::encode(device_id)
            )
            .as_str(),
        )
        .map_err(|err| {
            ProviderCallError::provider_unavailable(format!(
                "build Local Connector Provider URL failed: {err}"
            ))
        })?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("workspace_id", workspace_id);
            if let Some(relative_root) = workspace
                .relative_root
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                validate_relative_root(relative_root)?;
                query.append_pair("cwd", relative_root);
            }
        }
        let response = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header(
                "x-local-connector-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header(
                LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
                enabled_builtin_kinds,
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
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Connector Provider request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Local Connector Provider response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Connector Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result =
            decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Local Connector Provider")?;
        Ok(ProviderCallOutcome {
            result,
            response_bytes: bytes.len(),
        })
    }
}

fn validate_relative_root(relative_root: &str) -> Result<(), ProviderCallError> {
    let looks_like_windows_absolute = relative_root.as_bytes().get(1) == Some(&b':');
    if relative_root.starts_with(['/', '\\'])
        || looks_like_windows_absolute
        || relative_root.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment
                    .chars()
                    .any(|value| value == '\\' || value.is_control())
        })
    {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector workspace relative root is invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::extract::{Query, State};
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
        WorkspaceExecutionTarget,
    };

    use super::*;

    #[derive(Clone, Copy)]
    enum ResponseMode {
        Valid,
        WrongId,
        Oversized,
    }

    fn snapshot() -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_local_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: None,
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                execution_plane: ExecutionPlane::Local,
                workspace_provider: WorkspaceProviderKind::LocalConnector,
                workspace: Some(WorkspaceExecutionTarget {
                    device_id: Some("device-1".to_string()),
                    workspace_id: "workspace-1".to_string(),
                    relative_root: Some("apps/backend".to_string()),
                }),
                sandbox_provider: SandboxProviderKind::LocalConnector,
                sandbox_pairing_id: None,
                source_type: Some("local_connector".to_string()),
                revision: "project-revision".to_string(),
            },
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: Vec::new(),
            tools: Vec::new(),
            external_http_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: i64::MAX,
        }
    }

    fn code_read_route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::LocalConnector,
            provider_ref: Some("device:device-1/workspace:workspace-1".to_string()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    async fn start_local_connector(
        secret: &'static str,
        mode: ResponseMode,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State((secret, mode)): State<(&'static str, ResponseMode)>,
            headers: HeaderMap,
            Query(query): Query<HashMap<String, String>>,
            Json(request): Json<Value>,
        ) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-local-connector-caller")
                    .and_then(|value| value.to_str().ok()),
                Some(CALLER_SERVICE)
            );
            assert_eq!(
                headers
                    .get("x-local-connector-owner-user-id")
                    .and_then(|value| value.to_str().ok()),
                Some("user-1")
            );
            assert_eq!(
                headers
                    .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
                    .and_then(|value| value.to_str().ok()),
                Some("CodeMaintainerRead")
            );
            let token = headers
                .get("x-local-connector-internal-token")
                .and_then(|value| value.to_str().ok())
                .expect("signed Local Connector token");
            chatos_service_runtime::verify_internal_service_token(
                token,
                secret,
                CALLER_SERVICE,
                TOKEN_AUDIENCE,
                MCP_RELAY_SCOPE,
            )
            .expect("valid Local Connector token");
            assert_eq!(
                query.get("workspace_id").map(String::as_str),
                Some("workspace-1")
            );
            assert_eq!(query.get("cwd").map(String::as_str), Some("apps/backend"));
            let id = match mode {
                ResponseMode::WrongId => json!("different-invocation"),
                ResponseMode::Valid | ResponseMode::Oversized => {
                    request.get("id").cloned().unwrap_or(Value::Null)
                }
            };
            let result = match mode {
                ResponseMode::Oversized => json!({"content": "x".repeat(2048)}),
                ResponseMode::Valid | ResponseMode::WrongId => json!({
                    "forwarded_name": request.pointer("/params/name"),
                    "forwarded_arguments": request.pointer("/params/arguments")
                }),
            };
            Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/api/local-connectors/relay/device-1/mcp", post(handler))
            .with_state((secret, mode));
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), handle)
    }

    #[tokio::test]
    async fn call_uses_signed_identity_workspace_snapshot_and_original_tool_name() {
        const SECRET: &str = "a-long-local-connector-secret";
        let (base_url, server) = start_local_connector(SECRET, ResponseMode::Valid).await;
        let provider = LocalConnectorProvider::new(
            base_url,
            Duration::from_secs(5),
            Some(SECRET.to_string()),
            1024 * 1024,
        )
        .unwrap();
        let mut route = code_read_route();
        route.server_name = "browser_tools".to_string();
        assert!(provider.supports(&route));
        let outcome = provider
            .call_tool(
                &snapshot(),
                &route,
                "read_file",
                json!({"path": "src/lib.rs"}),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(
            outcome.result,
            json!({
                "forwarded_name": "read_file",
                "forwarded_arguments": {"path": "src/lib.rs"}
            })
        );
        server.abort();
    }

    #[tokio::test]
    async fn mismatched_jsonrpc_id_is_rejected() {
        const SECRET: &str = "a-long-local-connector-secret";
        let (base_url, server) = start_local_connector(SECRET, ResponseMode::WrongId).await;
        let provider = LocalConnectorProvider::new(
            base_url,
            Duration::from_secs(5),
            Some(SECRET.to_string()),
            1024 * 1024,
        )
        .unwrap();
        assert!(provider
            .call_tool(
                &snapshot(),
                &code_read_route(),
                "read_file",
                json!({"path": "src/lib.rs"}),
                "invocation-1",
            )
            .await
            .is_err());
        server.abort();
    }

    #[tokio::test]
    async fn oversized_response_is_rejected() {
        const SECRET: &str = "a-long-local-connector-secret";
        let (base_url, server) = start_local_connector(SECRET, ResponseMode::Oversized).await;
        let provider = LocalConnectorProvider::new(
            base_url,
            Duration::from_secs(5),
            Some(SECRET.to_string()),
            256,
        )
        .unwrap();
        assert!(provider
            .call_tool(
                &snapshot(),
                &code_read_route(),
                "read_file",
                json!({"path": "src/lib.rs"}),
                "invocation-1",
            )
            .await
            .is_err());
        server.abort();
    }

    #[test]
    fn absolute_or_parent_relative_roots_are_rejected() {
        for value in [
            "/tmp/project",
            "../project",
            "apps/../project",
            "C:/project",
        ] {
            assert!(validate_relative_root(value).is_err(), "accepted {value}");
        }
        validate_relative_root("apps/backend").unwrap();
    }
}
