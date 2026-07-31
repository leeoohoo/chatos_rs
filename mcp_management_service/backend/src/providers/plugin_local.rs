// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, WorkspaceProviderKind,
};
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::runtime::{PluginLocalProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[derive(Clone)]
pub(super) struct PluginLocalProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    response_limit_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct PluginPrepareResponse {
    run_id: String,
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    mcp: PreparedPluginMcpSnapshot,
    operations: Vec<String>,
    adapter_session_id: String,
    session_sha256: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct PreparedPluginMcpSnapshot {
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    oauth_connection_id: Option<String>,
    tools: Vec<Value>,
    tool_snapshot_sha256: String,
}

#[derive(Debug, Deserialize)]
struct PluginExecuteResponse {
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    invocation_id: String,
    tool_name: String,
    adapter_session_id: String,
    operation: String,
    result: Value,
}

#[derive(Debug, Deserialize)]
struct PluginCancelResponse {
    run_id: String,
    adapter_session_id: String,
    invocation_id: String,
    status: String,
}

impl PluginLocalProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Plugin Local Provider base URL is invalid: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Plugin Local Provider base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|error| format!("build Plugin Local Provider client failed: {error}"))?;
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
        self.internal_secret.is_some()
            && route.provider_kind == McpProviderKind::PluginLocal
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|value| value.starts_with("plugin-binding:"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_routes(
        &self,
        immutable_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, PluginLocalProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let mut bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::PluginLocal)
        {
            route.cancel_supported = false;
            let Some(immutable) = immutable_bindings.get(route.resource_id.as_str()) else {
                make_route_unavailable(route, "immutable Plugin MCP binding is missing");
                continue;
            };
            match self
                .prepare_route(
                    immutable,
                    route,
                    context,
                    runtime_session_id,
                    owner_user_id,
                    expires_at_unix,
                )
                .await
            {
                Ok(binding) => {
                    route.cancel_supported = true;
                    tool_snapshots.insert(route.resource_id.clone(), binding.tools.clone());
                    bindings.insert(route.resource_id.clone(), binding);
                }
                Err(error) => make_route_unavailable(route, error.message.as_str()),
            }
        }
        (bindings, tool_snapshots)
    }

    async fn prepare_route(
        &self,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> Result<PluginLocalProviderBinding, ProviderCallError> {
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.allow_writes != immutable.allow_writes
            || route.resource_id != immutable.resource_id
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Local route does not match its immutable binding",
            ));
        }
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Local route requires a Local Connector project workspace",
            ));
        }
        let workspace = context.workspace.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Local route is missing its project workspace snapshot",
            )
        })?;
        let device_id = workspace
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Local route is missing its device id",
                )
            })?;
        let workspace_id = workspace.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Local route is missing its workspace id",
            ));
        }
        if immutable.installation_device_id.as_deref() != Some(device_id) {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin installation is not pinned to the Project Context device",
            ));
        }
        let mut body = serde_json::Map::from_iter([
            ("run_id".to_string(), json!(runtime_session_id)),
            ("plugin_id".to_string(), json!(immutable.plugin_id)),
            ("release_id".to_string(), json!(immutable.release_id)),
            (
                "artifact_sha256".to_string(),
                json!(immutable.artifact_sha256),
            ),
            ("component_key".to_string(), json!(immutable.component_key)),
            (
                "permission_snapshot".to_string(),
                json!(immutable.permission_snapshot),
            ),
            (
                "tool_allowlist".to_string(),
                json!(immutable.tool_allowlist),
            ),
            (
                "tool_blocklist".to_string(),
                json!(immutable.tool_blocklist),
            ),
        ]);
        if let Some(server_key) = immutable.server_key.as_deref() {
            body.insert("server_key".to_string(), json!(server_key));
        }
        let response = self
            .request(
                owner_user_id,
                device_id,
                workspace_id,
                "prepare",
                Value::Object(body),
            )
            .await?;
        let prepared = serde_json::from_slice::<PluginPrepareResponse>(response.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local prepare returned invalid JSON: {error}"
                ))
            })?;
        validate_prepare_response(immutable, runtime_session_id, expires_at_unix, &prepared)?;
        let operation = prepared
            .operations
            .iter()
            .map(String::as_str)
            .find(|operation| *operation == MCP_TOOL_CALL_OPERATION)
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin Local prepare did not publish the MCP tool call operation",
                )
            })?;
        Ok(PluginLocalProviderBinding {
            runtime: immutable.clone(),
            run_id: runtime_session_id.to_string(),
            device_id: device_id.to_string(),
            workspace_id: workspace_id.to_string(),
            adapter_session_id: prepared.adapter_session_id,
            operation: operation.to_string(),
            session_sha256: prepared.session_sha256,
            tool_snapshot_sha256: prepared.mcp.tool_snapshot_sha256,
            tools: prepared.mcp.tools,
            oauth_connection_id: prepared.mcp.oauth_connection_id,
            expires_at_unix: prepared.expires_at,
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
        let binding = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        validate_bound_route(snapshot, route, binding)?;
        if !binding.publishes_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is not published by the immutable Plugin MCP snapshot".to_string(),
            });
        }
        let body = json!({
            "run_id": binding.run_id,
            "plugin_id": binding.runtime.plugin_id,
            "release_id": binding.runtime.release_id,
            "artifact_sha256": binding.runtime.artifact_sha256,
            "component_key": binding.runtime.component_key,
            "adapter_session_id": binding.adapter_session_id,
            "invocation_id": invocation_id,
            "operation": binding.operation,
            "tool_name": original_tool_name,
            "arguments": arguments,
        });
        let bytes = self
            .request(
                snapshot.owner_user_id.as_str(),
                binding.device_id.as_str(),
                binding.workspace_id.as_str(),
                "execute",
                body,
            )
            .await?;
        let response =
            serde_json::from_slice::<PluginExecuteResponse>(bytes.as_slice()).map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local execute returned invalid JSON: {error}"
                ))
            })?;
        if response.plugin_id != binding.runtime.plugin_id
            || response.release_id != binding.runtime.release_id
            || response.version != binding.runtime.version
            || response.artifact_sha256 != binding.runtime.artifact_sha256
            || response.component_key != binding.runtime.component_key
            || response.invocation_id != invocation_id
            || response.tool_name != original_tool_name
            || response.adapter_session_id != binding.adapter_session_id
            || response.operation != binding.operation
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin Local execute response does not match the immutable runtime binding",
            ));
        }
        Ok(ProviderCallOutcome {
            result: response.result,
            response_bytes: bytes.len(),
        })
    }

    pub(super) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let binding = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        validate_bound_route(snapshot, route, binding)?;
        let body = json!({
            "run_id": binding.run_id,
            "plugin_id": binding.runtime.plugin_id,
            "release_id": binding.runtime.release_id,
            "artifact_sha256": binding.runtime.artifact_sha256,
            "component_key": binding.runtime.component_key,
            "adapter_session_id": binding.adapter_session_id,
            "invocation_id": invocation_id,
        });
        let bytes = self
            .request(
                snapshot.owner_user_id.as_str(),
                binding.device_id.as_str(),
                binding.workspace_id.as_str(),
                "cancel",
                body,
            )
            .await?;
        let response =
            serde_json::from_slice::<PluginCancelResponse>(bytes.as_slice()).map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local cancel returned invalid JSON: {error}"
                ))
            })?;
        if response.run_id != binding.run_id
            || response.adapter_session_id != binding.adapter_session_id
            || response.invocation_id != invocation_id
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin Local cancel response does not match the immutable invocation binding",
            ));
        }
        match response.status.trim() {
            "cancelled" => Ok(ProviderCancelOutcome::Cancelled),
            "cancel_requested" | "invocation_not_found" | "already_completed" => {
                Ok(ProviderCancelOutcome::CancelRequested)
            }
            other => Err(ProviderCallError::invalid_response(format!(
                "Plugin Local cancel returned invalid status: {other}"
            ))),
        }
    }

    pub(super) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        self.close_bindings(
            snapshot.owner_user_id.as_str(),
            snapshot.session_id.as_str(),
            &snapshot.plugin_local_bindings,
        )
        .await;
    }

    pub(super) async fn close_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalProviderBinding>,
    ) {
        for binding in bindings.values() {
            let body = json!({
                "run_id": binding.run_id,
                "plugin_id": binding.runtime.plugin_id,
                "release_id": binding.runtime.release_id,
                "artifact_sha256": binding.runtime.artifact_sha256,
                "component_key": binding.runtime.component_key,
                "adapter_session_id": binding.adapter_session_id,
            });
            if let Err(error) = self
                .request(
                    owner_user_id,
                    binding.device_id.as_str(),
                    binding.workspace_id.as_str(),
                    "cancel",
                    body,
                )
                .await
            {
                tracing::warn!(
                    session_id = runtime_session_id,
                    resource_id = binding.runtime.resource_id.as_str(),
                    error = error.message,
                    "close Plugin Local runtime session failed"
                );
            }
        }
    }

    async fn request(
        &self,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: &str,
        action: &str,
        body: Value,
    ) -> Result<Vec<u8>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Local Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/plugins/{action}",
                self.base_url,
                urlencoding::encode(device_id)
            )
            .as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Plugin Local Provider URL failed: {error}"
            ))
        })?;
        url.query_pairs_mut()
            .append_pair("workspace_id", workspace_id);
        let response = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Plugin Local Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Plugin Local Provider rejected {action} with HTTP {}",
                status.as_u16()
            )));
        }
        Ok(bytes.to_vec())
    }
}

fn validate_prepare_response(
    immutable: &PluginMcpRuntimeBinding,
    runtime_session_id: &str,
    runtime_expires_at_unix: i64,
    prepared: &PluginPrepareResponse,
) -> Result<(), ProviderCallError> {
    if prepared.run_id != runtime_session_id
        || prepared.plugin_id != immutable.plugin_id
        || prepared.release_id != immutable.release_id
        || prepared.version != immutable.version
        || prepared.artifact_sha256 != immutable.artifact_sha256
        || prepared.component_key != immutable.component_key
        || prepared.mcp.plugin_id != immutable.plugin_id
        || prepared.mcp.release_id != immutable.release_id
        || prepared.mcp.version != immutable.version
        || prepared.mcp.artifact_sha256 != immutable.artifact_sha256
        || prepared.mcp.component_key != immutable.component_key
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare response does not match the immutable runtime binding",
        ));
    }
    if prepared.adapter_session_id.trim().is_empty()
        || !is_lower_sha256(prepared.session_sha256.as_str())
        || !is_lower_sha256(prepared.mcp.tool_snapshot_sha256.as_str())
        || prepared.expires_at <= chrono::Utc::now().timestamp()
        || prepared.expires_at < runtime_expires_at_unix
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare returned an invalid or prematurely expiring session snapshot",
        ));
    }
    validate_tool_snapshot(
        prepared.mcp.tools.as_slice(),
        prepared.mcp.tool_snapshot_sha256.as_str(),
    )?;
    if prepared.mcp.oauth_connection_id.as_ref().is_some_and(|id| {
        !immutable
            .auth_connection_ids
            .iter()
            .any(|allowed| allowed == id)
    }) {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare selected an OAuth connection outside the immutable snapshot",
        ));
    }
    Ok(())
}

fn validate_tool_snapshot(tools: &[Value], expected_sha256: &str) -> Result<(), ProviderCallError> {
    if tools.is_empty() || tools.len() > MAX_PLUGIN_TOOLS {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP tool snapshot must contain between 1 and 200 tools",
        ));
    }
    let encoded = serde_json::to_vec(tools).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin MCP tool snapshot failed: {error}"
        ))
    })?;
    if encoded.len() > MAX_PLUGIN_TOOL_SNAPSHOT_BYTES
        || hex::encode(Sha256::digest(encoded)) != expected_sha256
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP tool snapshot hash or size is invalid",
        ));
    }
    let mut names = HashSet::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin MCP tool snapshot contains an unnamed tool",
                )
            })?;
        if !names.insert(name) {
            return Err(ProviderCallError::invalid_response(
                "Plugin MCP tool snapshot contains duplicate tool names",
            ));
        }
    }
    Ok(())
}

fn validate_bound_route(
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    binding: &PluginLocalProviderBinding,
) -> Result<(), ProviderCallError> {
    let immutable = snapshot
        .plugin_mcp_bindings
        .get(route.resource_id.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "immutable Plugin MCP runtime binding is missing",
            )
        })?;
    let workspace = snapshot.project_context.workspace.as_ref();
    if !matches!(
        snapshot.project_context.workspace_provider,
        WorkspaceProviderKind::LocalConnector
    ) || !snapshot
        .expires_at_unix
        .min(binding.expires_at_unix)
        .gt(&chrono::Utc::now().timestamp())
        || immutable != &binding.runtime
        || route.provider_ref.as_deref() != Some(binding.runtime.provider_ref.as_str())
        || route.allow_writes != binding.runtime.allow_writes
        || workspace.and_then(|workspace| workspace.device_id.as_deref())
            != Some(binding.device_id.as_str())
        || workspace.map(|workspace| workspace.workspace_id.as_str())
            != Some(binding.workspace_id.as_str())
    {
        return Err(ProviderCallError::provider_unavailable(
            "Plugin Local route does not match its prepared runtime session",
        ));
    }
    Ok(())
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Plugin Local Provider unavailable: {reason}");
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::extract::{Path, Query, State};
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, SandboxProviderKind, WorkspaceExecutionTarget,
    };
    use chatos_plugin_management_sdk::{PluginExecutionHost, PluginMcpServer};

    use super::*;

    fn immutable_binding() -> PluginMcpRuntimeBinding {
        PluginMcpRuntimeBinding {
            provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
            resource_id: "plugin_mcp_workspace".to_string(),
            plugin_id: "plugin-workspace".to_string(),
            release_id: "release-workspace-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component_key: "workspace".to_string(),
            component_content_sha256: "c".repeat(64),
            declared_execution_host: PluginExecutionHost::Local,
            installation_device_id: Some("device-1".to_string()),
            permission_snapshot: vec!["workspace.read".to_string()],
            auth_connection_ids: vec!["oauth-workspace".to_string()],
            runtime: PluginMcpServer::Http {
                component_key: "workspace".to_string(),
                url: "http://127.0.0.1:4100/mcp".to_string(),
                headers: Default::default(),
                oauth_resource: None,
                connect_timeout_ms: None,
            },
            server_key: None,
            tool_allowlist: Vec::new(),
            tool_blocklist: Vec::new(),
            required: true,
            allow_writes: true,
        }
    }

    fn context() -> ProjectExecutionContext {
        ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Local,
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some("device-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: None,
            }),
            sandbox_provider: SandboxProviderKind::LocalConnector,
            sandbox_pairing_id: None,
            source_type: Some("local_connector".to_string()),
            revision: "project-revision".to_string(),
        }
    }

    fn route(binding: &PluginMcpRuntimeBinding) -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: binding.resource_id.clone(),
            server_name: "plugin_workspace_workspace".to_string(),
            provider_kind: McpProviderKind::PluginLocal,
            provider_ref: Some(binding.provider_ref.clone()),
            tool_namespace: "plugin_workspace_workspace".to_string(),
            allow_writes: true,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    async fn start_local_connector(
        secret: &'static str,
    ) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        #[derive(Clone)]
        struct TestState {
            secret: &'static str,
            actions: Arc<Mutex<Vec<String>>>,
        }

        async fn handler(
            State(state): State<TestState>,
            Path(action): Path<String>,
            Query(query): Query<HashMap<String, String>>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            assert_eq!(
                query.get("workspace_id").map(String::as_str),
                Some("workspace-1")
            );
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
            let token = headers
                .get("x-local-connector-internal-token")
                .and_then(|value| value.to_str().ok())
                .unwrap();
            chatos_service_runtime::verify_internal_service_token(
                token,
                state.secret,
                CALLER_SERVICE,
                TOKEN_AUDIENCE,
                PLUGIN_RELAY_SCOPE,
            )
            .unwrap();
            state.actions.lock().unwrap().push(action.clone());
            match action.as_str() {
                "prepare" => {
                    assert_eq!(
                        body.get("run_id").and_then(Value::as_str),
                        Some("session-1")
                    );
                    assert_eq!(
                        body.pointer("/permission_snapshot/0")
                            .and_then(Value::as_str),
                        Some("workspace.read")
                    );
                    let tools = vec![json!({
                        "name": "read_file",
                        "description": "Read a file",
                        "inputSchema": {"type": "object"}
                    })];
                    let tool_snapshot_sha256 =
                        hex::encode(Sha256::digest(serde_json::to_vec(&tools).unwrap()));
                    Json(json!({
                        "run_id": "session-1",
                        "plugin_id": "plugin-workspace",
                        "release_id": "release-workspace-1",
                        "version": "1.0.0",
                        "artifact_sha256": "a".repeat(64),
                        "component_key": "workspace",
                        "mcp": {
                            "plugin_id": "plugin-workspace",
                            "release_id": "release-workspace-1",
                            "version": "1.0.0",
                            "artifact_sha256": "a".repeat(64),
                            "component_key": "workspace",
                            "oauth_connection_id": "oauth-workspace",
                            "tools": tools,
                            "tool_snapshot_sha256": tool_snapshot_sha256
                        },
                        "operations": [MCP_TOOL_CALL_OPERATION, "mcp_health_check"],
                        "adapter_session_id": "adapter-1",
                        "session_sha256": "d".repeat(64),
                        "expires_at": chrono::Utc::now().timestamp() + 7200
                    }))
                }
                "execute" => {
                    assert_eq!(
                        body.get("adapter_session_id").and_then(Value::as_str),
                        Some("adapter-1")
                    );
                    assert_eq!(
                        body.get("tool_name").and_then(Value::as_str),
                        Some("read_file")
                    );
                    assert_eq!(
                        body.get("invocation_id").and_then(Value::as_str),
                        Some("invocation-1")
                    );
                    Json(json!({
                        "plugin_id": "plugin-workspace",
                        "release_id": "release-workspace-1",
                        "version": "1.0.0",
                        "artifact_sha256": "a".repeat(64),
                        "component_key": "workspace",
                        "invocation_id": "invocation-1",
                        "tool_name": "read_file",
                        "adapter_session_id": "adapter-1",
                        "operation": MCP_TOOL_CALL_OPERATION,
                        "result": {"content": [{"type": "text", "text": "hello"}]}
                    }))
                }
                "cancel" if body.get("invocation_id").is_some() => Json(json!({
                    "run_id": "session-1",
                    "adapter_session_id": "adapter-1",
                    "invocation_id": body["invocation_id"],
                    "status": "cancelled"
                })),
                "cancel" => Json(json!({"cancelled": true})),
                _ => panic!("unexpected Plugin relay action"),
            }
        }

        let actions = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/api/local-connectors/relay/device-1/plugins/{action}",
                post(handler),
            )
            .with_state(TestState {
                secret,
                actions: actions.clone(),
            });
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), actions, handle)
    }

    #[tokio::test]
    async fn prepare_call_and_close_use_the_exact_local_plugin_snapshot() {
        const SECRET: &str = "a-long-plugin-local-test-secret";
        let (base_url, actions, server) = start_local_connector(SECRET).await;
        let provider = PluginLocalProvider::new(
            base_url,
            Duration::from_secs(5),
            Some(SECRET.to_string()),
            1024 * 1024,
        )
        .unwrap();
        let immutable = immutable_binding();
        let mut routes = vec![route(&immutable)];
        let expires_at_unix = chrono::Utc::now().timestamp() + 600;
        let (local_bindings, tool_snapshots) = provider
            .prepare_routes(
                &HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
                routes.as_mut_slice(),
                &context(),
                "session-1",
                "user-1",
                expires_at_unix,
            )
            .await;
        assert_eq!(local_bindings.len(), 1);
        assert_eq!(
            tool_snapshots[&immutable.resource_id][0]["name"],
            "read_file"
        );
        assert!(routes[0].cancel_supported);
        let snapshot = RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_local_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: None,
            source_session_id: None,
            source_user_message_id: None,
            contact_agent_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            sandbox_target: None,
            project_context: context(),
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: routes.clone(),
            tools: Vec::new(),
            plugin_mcp_bindings: HashMap::from([(
                immutable.resource_id.clone(),
                immutable.clone(),
            )]),
            plugin_local_bindings: local_bindings,
            external_http_bindings: Default::default(),
            cloud_stdio_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix,
        };
        let outcome = provider
            .call_tool(
                &snapshot,
                &routes[0],
                "read_file",
                json!({"path": "README.md"}),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(
            outcome.result.pointer("/content/0/text"),
            Some(&json!("hello"))
        );
        assert_eq!(
            provider
                .cancel_invocation(&snapshot, &routes[0], "invocation-1")
                .await
                .unwrap(),
            ProviderCancelOutcome::Cancelled
        );
        provider.close_session(&snapshot).await;
        assert_eq!(
            actions.lock().unwrap().as_slice(),
            ["prepare", "execute", "cancel", "cancel"]
        );
        server.abort();
    }
}
