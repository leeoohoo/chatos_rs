// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::sandbox_images::{SANDBOX_IMAGE_PROJECT_ID_HEADER, SANDBOX_IMAGE_RUN_ID_HEADER};
use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxProviderKind};
use chatos_mcp_service::METHOD_TOOLS_CALL;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::project_service::decode_jsonrpc_response;
use super::{ProviderCallError, ProviderCallOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const SANDBOX_MANAGER_AUDIENCE: &str = "sandbox-manager";
const LOCAL_CONNECTOR_AUDIENCE: &str = "local-connector-service";
const SANDBOX_SERVICE_SCOPE: &str = "sandbox.service";
const CLOUD_PROVIDER_REF: &str = "sandbox-images:cloud";
const LOCAL_PROVIDER_REF_PREFIX: &str = "sandbox-images:local:";
const TOOL_CREATE_IMAGE: &str = "create_image";
const DEFAULT_CREATE_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
const MAX_CREATE_TIMEOUT_MS: u64 = 2 * 60 * 60 * 1_000;
const TRANSPORT_GRACE_MS: u64 = 30_000;

#[derive(Clone)]
pub(super) struct SandboxImagesProvider {
    cloud_http: reqwest::Client,
    cloud_base_url: String,
    cloud_internal_secret: Option<String>,
    local_http: reqwest::Client,
    local_base_url: String,
    local_internal_secret: Option<String>,
    request_timeout: Duration,
    image_request_timeout: Duration,
    response_limit_bytes: usize,
}

impl SandboxImagesProvider {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        cloud_base_url: impl Into<String>,
        cloud_internal_secret: Option<String>,
        local_base_url: impl Into<String>,
        local_internal_secret: Option<String>,
        request_timeout: Duration,
        image_request_timeout: Duration,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let cloud_base_url = normalized_base_url(cloud_base_url.into(), "Sandbox Manager")?;
        let local_base_url = normalized_base_url(local_base_url.into(), "Local Connector")?;
        Ok(Self {
            cloud_http: build_client("Sandbox Manager image")?,
            cloud_base_url,
            cloud_internal_secret: normalized_secret(cloud_internal_secret),
            local_http: build_client("Local Connector image")?,
            local_base_url,
            local_internal_secret: normalized_secret(local_internal_secret),
            request_timeout,
            image_request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if !is_sandbox_images_route(route) {
            return false;
        }
        match route.provider_kind {
            McpProviderKind::CloudSandbox => self.cloud_internal_secret.is_some(),
            McpProviderKind::LocalConnector => self.local_internal_secret.is_some(),
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
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Sandbox Images Provider does not support this route",
            ));
        }
        let timeout = call_timeout(
            original_tool_name,
            &arguments,
            self.request_timeout,
            self.image_request_timeout,
        );
        let request = match route.provider_kind {
            McpProviderKind::CloudSandbox => self.cloud_request(snapshot, route)?,
            McpProviderKind::LocalConnector => self.local_request(snapshot, route)?,
            _ => {
                return Err(ProviderCallError::provider_unavailable(
                    "Sandbox Images route has an invalid provider kind",
                ))
            }
        };
        let response = request
            .timeout(timeout)
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
                    "Sandbox Images Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Sandbox Images Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Sandbox Images Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result =
            decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Sandbox Images Provider")?;
        Ok(ProviderCallOutcome {
            result,
            response_bytes: bytes.len(),
        })
    }

    fn cloud_request(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        if snapshot.project_context.sandbox_provider != SandboxProviderKind::Cloud
            || route.provider_ref.as_deref() != Some(CLOUD_PROVIDER_REF)
        {
            return Err(ProviderCallError::provider_unavailable(
                "cloud Sandbox Images route does not match the immutable project context",
            ));
        }
        let secret = self.cloud_internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Sandbox Manager image internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            SANDBOX_MANAGER_AUDIENCE,
            SANDBOX_SERVICE_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        Ok(with_runtime_headers(
            self.cloud_http
                .post(format!("{}/api/sandbox-images/mcp", self.cloud_base_url))
                .header("x-sandbox-caller", CALLER_SERVICE)
                .header("x-sandbox-internal-token", token),
            snapshot,
        ))
    }

    fn local_request(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        if snapshot.project_context.sandbox_provider != SandboxProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "local Sandbox Images route does not match the immutable project context",
            ));
        }
        let pairing_id = snapshot
            .project_context
            .sandbox_pairing_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "local Sandbox Images route has no sandbox pairing",
                )
            })?;
        let expected_provider_ref = local_provider_ref(pairing_id);
        if route.provider_ref.as_deref() != Some(expected_provider_ref.as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "local Sandbox Images route does not match the immutable sandbox pairing",
            ));
        }
        let secret = self.local_internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector image internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            LOCAL_CONNECTOR_AUDIENCE,
            SANDBOX_SERVICE_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let pairing_id = urlencoding::encode(pairing_id);
        Ok(with_runtime_headers(
            self.local_http
                .post(format!(
                    "{}/api/local-connectors/sandbox-facade/{pairing_id}/api/local/sandbox/images/mcp",
                    self.local_base_url
                ))
                .header("x-local-connector-caller", CALLER_SERVICE)
                .header("x-local-connector-internal-token", token)
                .header(
                    "x-local-connector-owner-user-id",
                    snapshot.owner_user_id.as_str(),
                ),
            snapshot,
        ))
    }
}

pub(crate) const fn cloud_provider_ref() -> &'static str {
    CLOUD_PROVIDER_REF
}

pub(crate) fn local_provider_ref(pairing_id: &str) -> String {
    format!("{LOCAL_PROVIDER_REF_PREFIX}{}", pairing_id.trim())
}

fn is_sandbox_images_route(route: &ResolvedMcpRoute) -> bool {
    system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
        .is_some_and(|descriptor| descriptor.key == SystemMcpKey::SandboxImages)
}

fn normalized_base_url(value: String, provider: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(value.as_str())
        .map_err(|error| format!("{provider} image base URL is invalid: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{provider} image base URL must use http or https"));
    }
    Ok(value.trim().trim_end_matches('/').to_string())
}

fn normalized_secret(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_client(provider: &str) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(Policy::none())
        .build()
        .map_err(|error| format!("build {provider} Provider client failed: {error}"))
}

fn with_runtime_headers(
    mut request: reqwest::RequestBuilder,
    snapshot: &RuntimeSessionSnapshot,
) -> reqwest::RequestBuilder {
    request = request.header(
        SANDBOX_IMAGE_PROJECT_ID_HEADER,
        snapshot.project_id.as_str(),
    );
    if let Some(run_id) = snapshot
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.header(SANDBOX_IMAGE_RUN_ID_HEADER, run_id);
    }
    request
}

fn call_timeout(
    tool_name: &str,
    arguments: &Value,
    request_timeout: Duration,
    image_request_timeout: Duration,
) -> Duration {
    if tool_name != TOOL_CREATE_IMAGE {
        return request_timeout.min(image_request_timeout);
    }
    let requested = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CREATE_TIMEOUT_MS)
        .clamp(1_000, MAX_CREATE_TIMEOUT_MS);
    Duration::from_millis(requested.saturating_add(TRANSPORT_GRACE_MS)).min(image_request_timeout)
}

#[cfg(test)]
mod tests {
    use axum::extract::{Path, State};
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, WorkspaceProviderKind,
    };

    use super::*;

    const CLOUD_SECRET: &str = "a-long-sandbox-manager-secret";
    const LOCAL_SECRET: &str = "a-long-local-connector-secret";

    fn route(kind: McpProviderKind, provider_ref: String) -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: chatos_mcp::system_mcp_descriptor(SystemMcpKey::SandboxImages)
                .resource_id
                .to_string(),
            server_name: "sandbox_images".to_string(),
            provider_kind: kind,
            provider_ref: Some(provider_ref),
            tool_namespace: "sandbox_images".to_string(),
            allow_writes: true,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: false,
            reason: "test".to_string(),
        }
    }

    fn snapshot(provider: SandboxProviderKind, pairing_id: Option<&str>) -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
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
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: None,
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                execution_plane: ExecutionPlane::Cloud,
                workspace_provider: WorkspaceProviderKind::None,
                workspace: None,
                sandbox_provider: provider,
                sandbox_pairing_id: pairing_id.map(str::to_string),
                source_type: None,
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

    async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State((cloud_secret, local_secret)): State<(&'static str, &'static str)>,
            Path(path): Path<String>,
            headers: HeaderMap,
            Json(request): Json<Value>,
        ) -> Json<Value> {
            let (caller_header, token_header, secret, audience) = if path
                == "cloud/api/sandbox-images/mcp"
            {
                (
                    "x-sandbox-caller",
                    "x-sandbox-internal-token",
                    cloud_secret,
                    SANDBOX_MANAGER_AUDIENCE,
                )
            } else {
                assert_eq!(
                    path,
                    "local/api/local-connectors/sandbox-facade/pairing-1/api/local/sandbox/images/mcp"
                );
                assert_eq!(
                    headers
                        .get("x-local-connector-owner-user-id")
                        .and_then(|value| value.to_str().ok()),
                    Some("user-1")
                );
                (
                    "x-local-connector-caller",
                    "x-local-connector-internal-token",
                    local_secret,
                    LOCAL_CONNECTOR_AUDIENCE,
                )
            };
            assert_eq!(
                headers
                    .get(caller_header)
                    .and_then(|value| value.to_str().ok()),
                Some(CALLER_SERVICE)
            );
            let token = headers
                .get(token_header)
                .and_then(|value| value.to_str().ok())
                .expect("signed internal token");
            chatos_service_runtime::verify_internal_service_token(
                token,
                secret,
                CALLER_SERVICE,
                audience,
                SANDBOX_SERVICE_SCOPE,
            )
            .expect("valid internal token");
            assert_eq!(
                headers
                    .get(SANDBOX_IMAGE_PROJECT_ID_HEADER)
                    .and_then(|value| value.to_str().ok()),
                Some("project-1")
            );
            assert_eq!(
                headers
                    .get(SANDBOX_IMAGE_RUN_ID_HEADER)
                    .and_then(|value| value.to_str().ok()),
                Some("run-1")
            );
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": {
                    "forwarded_tool": request.pointer("/params/name"),
                    "forwarded_arguments": request.pointer("/params/arguments"),
                }
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/{*path}", post(handler))
            .with_state((CLOUD_SECRET, LOCAL_SECRET));
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), handle)
    }

    fn provider(base_url: &str) -> SandboxImagesProvider {
        SandboxImagesProvider::new(
            format!("{base_url}/cloud"),
            Some(CLOUD_SECRET.to_string()),
            format!("{base_url}/local"),
            Some(LOCAL_SECRET.to_string()),
            Duration::from_secs(5),
            Duration::from_secs(2 * 60 * 60 + 30),
            1024 * 1024,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn cloud_and_local_routes_use_their_pinned_management_planes() {
        let (base_url, server) = start_server().await;
        let provider = provider(base_url.as_str());
        let cloud = provider
            .call_tool(
                &snapshot(SandboxProviderKind::Cloud, None),
                &route(
                    McpProviderKind::CloudSandbox,
                    cloud_provider_ref().to_string(),
                ),
                "get_image_catalog",
                json!({}),
                "invocation-cloud",
            )
            .await
            .unwrap();
        assert_eq!(cloud.result["forwarded_tool"], "get_image_catalog");

        let local = provider
            .call_tool(
                &snapshot(SandboxProviderKind::LocalConnector, Some("pairing-1")),
                &route(
                    McpProviderKind::LocalConnector,
                    local_provider_ref("pairing-1"),
                ),
                "search_images",
                json!({"features": ["node@24"]}),
                "invocation-local",
            )
            .await
            .unwrap();
        assert_eq!(local.result["forwarded_tool"], "search_images");
        assert_eq!(
            local.result["forwarded_arguments"]["features"][0],
            "node@24"
        );
        server.abort();
    }

    #[test]
    fn create_image_timeout_tracks_the_tool_wait_with_transport_grace() {
        assert_eq!(
            call_timeout(
                TOOL_CREATE_IMAGE,
                &json!({"timeout_ms": 90_000}),
                Duration::from_secs(5),
                Duration::from_secs(2 * 60 * 60 + 30),
            ),
            Duration::from_secs(120)
        );
        assert_eq!(
            call_timeout(
                "get_image_catalog",
                &json!({}),
                Duration::from_secs(5),
                Duration::from_secs(60),
            ),
            Duration::from_secs(5)
        );
    }
}
