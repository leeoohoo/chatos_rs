// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};
use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_mcp_service::{MCP_ERROR_AUTH_REQUIRED, METHOD_TOOLS_CALL};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, PluginMcpCloudRuntimeBundle, PluginMcpServer,
    ResolvedAgentCapabilities, ResolvedMcp,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::runtime::{CloudStdioProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

mod validation;
use validation::*;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";
const MAX_COMMAND_BYTES: usize = 256;
const MAX_TOOL_POLICY_ITEMS: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 256;

#[derive(Clone)]
pub(super) struct CloudStdioProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[derive(Debug, Serialize)]
struct CloudStdioCallRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation_id: Option<&'a str>,
    command: &'a str,
    args: &'a [String],
    env: &'a BTreeMap<String, String>,
    cwd: Option<&'a str>,
    plugin_artifact: Option<&'a PluginMcpCloudRuntimeBundle>,
    plugin_workspace_write: bool,
    method: &'a str,
    params: Value,
    expires_at_unix: i64,
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
struct CloudStdioCloseRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
}

#[derive(Debug, Serialize)]
struct CloudStdioCancelRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
    invocation_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct CloudStdioCallResponse {
    result: Value,
}

#[derive(Debug, Deserialize)]
struct CloudStdioCancelResponse {
    status: String,
}

impl CloudStdioProvider {
    pub(super) fn new(
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Sandbox Manager cloud stdio base URL is invalid: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Sandbox Manager cloud stdio base URL must use http or https".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|error| format!("build cloud stdio Provider client failed: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        self.internal_secret.is_some()
            && route.provider_kind == McpProviderKind::CloudStdio
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|provider_ref| provider_ref.starts_with("sandbox:"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, CloudStdioProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let resources = capabilities
            .mcps
            .iter()
            .map(|resolved| (resolved.resource.id.as_str(), resolved))
            .collect::<HashMap<_, _>>();
        let mut bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::CloudStdio)
        {
            let binding = resources
                .get(route.resource_id.as_str())
                .ok_or_else(|| "capability resource is missing".to_string())
                .and_then(|resolved| prepare_binding(resolved, route));
            let binding = match binding {
                Ok(binding) => {
                    route.cancel_supported = true;
                    binding
                }
                Err(reason) => {
                    make_route_unavailable(route, reason.as_str());
                    continue;
                }
            };
            let Some(target) = target else {
                make_route_unavailable(route, "runtime sandbox target is missing");
                continue;
            };
            let context = CloudStdioRequestContext {
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
            };
            match self
                .list_tools(target, &context, route.resource_id.as_str(), &binding)
                .await
            {
                Ok(tools) => {
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                    bindings.insert(route.resource_id.clone(), binding);
                }
                Err(error) => {
                    make_route_unavailable(route, error.message.as_str());
                }
            }
        }
        (bindings, tool_snapshots)
    }

    async fn list_tools(
        &self,
        target: &SandboxExecutionTarget,
        context: &CloudStdioRequestContext<'_>,
        resource_id: &str,
        binding: &CloudStdioProviderBinding,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let body = CloudStdioCallRequest {
            runtime_session_id: context.runtime_session_id,
            resource_id,
            invocation_id: None,
            command: binding.command.as_str(),
            args: binding.args.as_slice(),
            env: &binding.env,
            cwd: binding.cwd.as_deref(),
            plugin_artifact: binding.plugin_artifact.as_ref(),
            plugin_workspace_write: binding.plugin_artifact.is_some() && binding.allow_writes,
            method: "tools/list",
            params: json!({}),
            expires_at_unix: context.expires_at_unix,
            timeout_ms: self.request_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
        };
        let response = self.request(target, context, "call", &body).await?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP tools/list response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud stdio MCP tools/list returned HTTP {}",
                status.as_u16()
            )));
        }
        let response = serde_json::from_slice::<CloudStdioCallResponse>(bytes.as_slice()).map_err(
            |error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP tools/list returned an invalid response: {error}"
                ))
            },
        )?;
        extract_tool_snapshot(response.result).map_err(ProviderCallError::invalid_response)
    }

    pub(super) fn prepare_plugin_binding(
        &self,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        resolved_environment: &BTreeMap<String, String>,
        runtime_bundle: &PluginMcpCloudRuntimeBundle,
    ) -> Result<CloudStdioProviderBinding, String> {
        if route.provider_kind != McpProviderKind::PluginCloud
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.resource_id != immutable.resource_id
            || route.allow_writes != immutable.allow_writes
        {
            return Err("Plugin stdio route does not match its immutable binding".to_string());
        }
        let PluginMcpServer::Stdio {
            command,
            args,
            env,
            cwd,
            ..
        } = runtime_bundle.effective_runtime()
        else {
            return Err("Plugin MCP runtime is not stdio".to_string());
        };
        if !immutable
            .permission_snapshot
            .iter()
            .any(|permission| permission == "process.spawn")
        {
            return Err(
                "Plugin stdio MCP requires process.spawn in its immutable permission snapshot"
                    .to_string(),
            );
        }
        let configured_names = env.keys().collect::<std::collections::BTreeSet<_>>();
        let resolved_names = resolved_environment
            .keys()
            .collect::<std::collections::BTreeSet<_>>();
        if configured_names != resolved_names {
            return Err(
                "Plugin stdio resolved environment does not match the immutable templates"
                    .to_string(),
            );
        }
        if !env.is_empty()
            && !immutable.permission_snapshot.iter().any(|permission| {
                permission == "credential.use" || permission.starts_with("credential.use:")
            })
        {
            return Err(
                "Plugin stdio credentials require credential.use in the immutable permission snapshot"
                    .to_string(),
            );
        }
        if runtime_bundle.bundle_sha256 != immutable.component_content_sha256
            || runtime_bundle.plugin_id != immutable.plugin_id
            || runtime_bundle.release_id != immutable.release_id
            || runtime_bundle.component.component_key != immutable.component_key
            || runtime_bundle.runtime != immutable.runtime
        {
            return Err("Plugin artifact Bundle does not match the immutable binding".to_string());
        }
        let package_relative_command = command.contains('/');
        let (command, cwd, plugin_artifact) = if package_relative_command {
            validate_plugin_artifact_ref(runtime_bundle.artifact_ref.as_str())?;
            let command = normalize_plugin_relative_path(command)
                .map_err(|error| format!("Plugin package-relative command is invalid: {error}"))?;
            let cwd = cwd
                .as_ref()
                .map(|value| normalize_plugin_relative_path(value.path.as_str()))
                .transpose()
                .map_err(|error| format!("Plugin package-relative cwd is invalid: {error}"))?;
            (command, cwd, Some(runtime_bundle.clone()))
        } else {
            if cwd.is_some() {
                return Err(
                    "Plugin package-relative cwd requires a package-relative executable"
                        .to_string(),
                );
            }
            validate_command(command, args.as_slice())?;
            (command.trim().to_string(), None, None)
        };
        validate_arguments(args.as_slice())?;
        validate_environment(resolved_environment)?;
        let allowed_tool_names =
            configured_tool_names(immutable.tool_allowlist.as_slice(), "tool_allowlist")?;
        let blocked_tool_names =
            configured_tool_names(immutable.tool_blocklist.as_slice(), "tool_blocklist")?;
        if !route.allow_writes && allowed_tool_names.is_empty() {
            return Err("read-only Plugin stdio MCP requires tool_allowlist".to_string());
        }
        Ok(CloudStdioProviderBinding {
            provider_ref: immutable.provider_ref.clone(),
            command,
            args: args.clone(),
            env: resolved_environment.clone(),
            cwd,
            plugin_artifact,
            allow_writes: route.allow_writes,
            allowed_tool_names,
            blocked_tool_names,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn list_plugin_tools(
        &self,
        target: &SandboxExecutionTarget,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
        resource_id: &str,
        binding: &CloudStdioProviderBinding,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let context = CloudStdioRequestContext {
            runtime_session_id,
            owner_user_id,
            project_id,
            run_id,
            expires_at_unix,
        };
        self.list_tools(target, &context, resource_id, binding)
            .await
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
            .cloud_stdio_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Cloud stdio MCP runtime binding is missing",
                )
            })?;
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
            || binding.provider_ref != target.provider_ref()
            || route.allow_writes != binding.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud stdio MCP route does not match its immutable runtime binding",
            ));
        }
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is blocked by the Cloud stdio MCP policy".to_string(),
            });
        }
        self.call_bound_tool(
            snapshot,
            route.resource_id.as_str(),
            binding,
            original_tool_name,
            arguments,
            invocation_id,
        )
        .await
    }

    pub(super) async fn call_bound_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        resource_id: &str,
        binding: &CloudStdioProviderBinding,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is blocked by the Cloud stdio MCP policy".to_string(),
            });
        }
        let body = CloudStdioCallRequest {
            runtime_session_id: snapshot.session_id.as_str(),
            resource_id,
            invocation_id: Some(invocation_id),
            command: binding.command.as_str(),
            args: binding.args.as_slice(),
            env: &binding.env,
            cwd: binding.cwd.as_deref(),
            plugin_artifact: binding.plugin_artifact.as_ref(),
            plugin_workspace_write: binding.plugin_artifact.is_some() && binding.allow_writes,
            method: METHOD_TOOLS_CALL,
            params: json!({
                "name": original_tool_name,
                "arguments": arguments,
            }),
            expires_at_unix: snapshot.expires_at_unix,
            timeout_ms: self.request_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
        };
        let context = CloudStdioRequestContext::from_snapshot(snapshot);
        let response = self.request(target, &context, "call", &body).await?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud stdio MCP runner returned HTTP {}",
                status.as_u16()
            )));
        }
        let response = serde_json::from_slice::<CloudStdioCallResponse>(bytes.as_slice()).map_err(
            |error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP runner returned an invalid response: {error}"
                ))
            },
        )?;
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
            .cloud_stdio_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Cloud stdio MCP runtime binding is missing",
                )
            })?;
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
            || binding.provider_ref != target.provider_ref()
            || route.allow_writes != binding.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud stdio MCP route does not match its immutable runtime binding",
            ));
        }
        self.cancel_bound_invocation(snapshot, route.resource_id.as_str(), invocation_id)
            .await
    }

    pub(super) async fn cancel_bound_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        resource_id: &str,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        let body = CloudStdioCancelRequest {
            runtime_session_id: snapshot.session_id.as_str(),
            resource_id,
            invocation_id,
        };
        let context = CloudStdioRequestContext::from_snapshot(snapshot);
        let response = self.request(target, &context, "cancel", &body).await?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP cancellation response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud stdio MCP cancellation returned HTTP {}",
                status.as_u16()
            )));
        }
        let response = serde_json::from_slice::<CloudStdioCancelResponse>(bytes.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP cancellation returned invalid JSON: {error}"
                ))
            })?;
        match response.status.trim() {
            "cancelled" => Ok(ProviderCancelOutcome::Cancelled),
            "cancel_requested" | "already_completed" | "invocation_not_found" => {
                Ok(ProviderCancelOutcome::CancelRequested)
            }
            other => Err(ProviderCallError::invalid_response(format!(
                "Cloud stdio MCP cancellation returned invalid status: {other}"
            ))),
        }
    }

    pub(super) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        let Some(target) = snapshot.sandbox_target.as_ref() else {
            return;
        };
        self.close_bindings(
            target,
            snapshot.session_id.as_str(),
            snapshot.owner_user_id.as_str(),
            snapshot.project_id.as_str(),
            snapshot.run_id.as_deref(),
            snapshot.expires_at_unix,
            &snapshot.cloud_stdio_bindings,
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn close_bindings(
        &self,
        target: &SandboxExecutionTarget,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
        bindings: &HashMap<String, CloudStdioProviderBinding>,
    ) {
        let context = CloudStdioRequestContext {
            runtime_session_id,
            owner_user_id,
            project_id,
            run_id,
            expires_at_unix,
        };
        for resource_id in bindings.keys() {
            let body = CloudStdioCloseRequest {
                runtime_session_id,
                resource_id: resource_id.as_str(),
            };
            if let Err(error) = self.request(target, &context, "close", &body).await {
                tracing::warn!(
                    session_id = runtime_session_id,
                    resource_id = resource_id.as_str(),
                    error = error.message,
                    "close Cloud stdio MCP session failed"
                );
            }
        }
    }

    async fn request<T>(
        &self,
        target: &SandboxExecutionTarget,
        context: &CloudStdioRequestContext<'_>,
        action: &str,
        body: &T,
    ) -> Result<reqwest::Response, ProviderCallError>
    where
        T: Serialize + ?Sized,
    {
        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let prefix = if target.is_environment {
            "sandbox-environments"
        } else {
            "sandboxes"
        };
        let url = format!(
            "{}/api/{prefix}/{sandbox_id}/cloud-stdio-mcp/{action}",
            self.base_url
        );
        let mut request = self.authenticated(self.http.post(url))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        request
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header("x-mcp-management-owner-user-id", context.owner_user_id)
            .header("x-mcp-management-project-id", context.project_id)
            .header(
                "x-mcp-management-run-id",
                context.run_id.unwrap_or_default(),
            )
            .json(body)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Cloud stdio MCP runner request failed: {error}"
                ))
            })
    }

    fn authenticated(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Sandbox Manager cloud stdio internal secret is not configured",
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

struct CloudStdioRequestContext<'a> {
    runtime_session_id: &'a str,
    owner_user_id: &'a str,
    project_id: &'a str,
    run_id: Option<&'a str>,
    expires_at_unix: i64,
}

impl<'a> CloudStdioRequestContext<'a> {
    fn from_snapshot(snapshot: &'a RuntimeSessionSnapshot) -> Self {
        Self {
            runtime_session_id: snapshot.session_id.as_str(),
            owner_user_id: snapshot.owner_user_id.as_str(),
            project_id: snapshot.project_id.as_str(),
            run_id: snapshot.run_id.as_deref(),
            expires_at_unix: snapshot.expires_at_unix,
        }
    }
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Cloud stdio MCP configuration is unavailable: {reason}");
}

fn extract_tool_snapshot(result: Value) -> Result<Vec<Value>, String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Cloud stdio MCP tools/list response has no tools array".to_string())?;
    for tool in &tools {
        if !tool.is_object()
            || tool
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
        {
            return Err(
                "Cloud stdio MCP tools/list response contains an invalid tool definition"
                    .to_string(),
            );
        }
    }
    Ok(tools)
}

#[cfg(test)]
mod tests;
