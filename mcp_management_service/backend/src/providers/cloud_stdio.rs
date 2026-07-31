// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};
use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_mcp_service::{MCP_ERROR_AUTH_REQUIRED, METHOD_TOOLS_CALL};
use chatos_plugin_management_sdk::{PluginMcpServer, ResolvedAgentCapabilities, ResolvedMcp};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::runtime::{CloudStdioProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::{ProviderCallError, ProviderCallOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";
const MAX_COMMAND_BYTES: usize = 256;
const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_ARGUMENTS_BYTES: usize = 128 * 1024;
const MAX_ENVIRONMENT_VARIABLES: usize = 128;
const MAX_ENVIRONMENT_BYTES: usize = 64 * 1024;
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
    command: &'a str,
    args: &'a [String],
    env: &'a BTreeMap<String, String>,
    cwd: Option<&'a str>,
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

#[derive(Debug, Deserialize)]
struct CloudStdioCallResponse {
    result: Value,
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
                    route.cancel_supported = false;
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
            command: binding.command.as_str(),
            args: binding.args.as_slice(),
            env: &binding.env,
            cwd: binding.cwd.as_deref(),
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
        } = &immutable.runtime
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
        if cwd.is_some() {
            return Err(
                "Plugin package-relative cwd requires the cloud artifact mount contract"
                    .to_string(),
            );
        }
        validate_command(command, args.as_slice())?;
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
            command: command.trim().to_string(),
            args: args.clone(),
            env: resolved_environment.clone(),
            cwd: None,
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
        _invocation_id: &str,
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
            command: binding.command.as_str(),
            args: binding.args.as_slice(),
            env: &binding.env,
            cwd: binding.cwd.as_deref(),
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

fn prepare_binding(
    resolved: &ResolvedMcp,
    route: &ResolvedMcpRoute,
) -> Result<CloudStdioProviderBinding, String> {
    if resolved.resource.runtime.kind.trim() != "stdio_cloud" {
        return Err("runtime kind is not stdio_cloud".to_string());
    }
    let provider_ref = route
        .provider_ref
        .clone()
        .filter(|value| value.starts_with("sandbox:"))
        .ok_or_else(|| "route has no bound sandbox target".to_string())?;
    let command = resolved
        .resource
        .runtime
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "runtime command is missing".to_string())?;
    validate_command(command, resolved.resource.runtime.args.as_slice())?;
    validate_arguments(resolved.resource.runtime.args.as_slice())?;
    validate_environment(&resolved.resource.runtime.env)?;
    let cwd = normalized_cwd(resolved.resource.runtime.cwd.as_deref())?;
    let allowed_tool_names = configured_tool_names(
        resolved.resource.security.allowed_tool_names.as_slice(),
        "allowed_tool_names",
    )?;
    let blocked_tool_names = configured_tool_names(
        resolved.resource.security.blocked_tool_names.as_slice(),
        "blocked_tool_names",
    )?;
    if !route.allow_writes && allowed_tool_names.is_empty() {
        return Err("read-only cloud stdio MCP requires allowed_tool_names".to_string());
    }
    Ok(CloudStdioProviderBinding {
        provider_ref,
        command: command.to_string(),
        args: resolved.resource.runtime.args.clone(),
        env: resolved.resource.runtime.env.clone(),
        cwd,
        allow_writes: route.allow_writes,
        allowed_tool_names,
        blocked_tool_names,
    })
}

fn validate_command(command: &str, args: &[String]) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty()
        || command.len() > MAX_COMMAND_BYTES
        || command
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0'))
        || matches!(command, "." | "..")
    {
        return Err("command must be a PATH-resolved executable name".to_string());
    }
    let shell = command.trim_end_matches(".exe").to_ascii_lowercase();
    let is_shell = matches!(
        shell.as_str(),
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish" | "cmd" | "powershell" | "pwsh"
    );
    let invokes_inline_command = args.iter().any(|arg| {
        matches!(
            arg.trim().to_ascii_lowercase().as_str(),
            "-c" | "/c" | "-command" | "-encodedcommand"
        )
    });
    if is_shell && invokes_inline_command {
        return Err("shell inline command execution is forbidden".to_string());
    }
    Ok(())
}

fn validate_arguments(args: &[String]) -> Result<(), String> {
    if args.len() > MAX_ARGUMENTS
        || args
            .iter()
            .any(|arg| arg.len() > MAX_ARGUMENT_BYTES || arg.contains('\0'))
        || args.iter().map(String::len).sum::<usize>() > MAX_ARGUMENTS_BYTES
    {
        return Err("arguments exceed the supported limits".to_string());
    }
    Ok(())
}

fn validate_environment(env: &BTreeMap<String, String>) -> Result<(), String> {
    if env.len() > MAX_ENVIRONMENT_VARIABLES
        || env
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > MAX_ENVIRONMENT_BYTES
    {
        return Err("environment exceeds the supported limits".to_string());
    }
    for (name, value) in env {
        validate_environment_name(name)?;
        if value.contains('\0') {
            return Err("environment contains an invalid value".to_string());
        }
    }
    Ok(())
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let normalized = name.to_ascii_uppercase();
    let controlled = matches!(
        normalized.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "CHATOS_WORKSPACE"
            | "CHATOS_SANDBOX_MCP_TOKEN"
            | "CHATOS_AGENT_TOKEN"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || normalized.starts_with("LD_")
        || normalized.starts_with("DYLD_")
        || normalized.starts_with("XDG_")
        || normalized.starts_with("MCP_MANAGEMENT_")
        || normalized.starts_with("SANDBOX_MANAGER_");
    if !valid || controlled {
        return Err("environment contains an invalid or Host-controlled name".to_string());
    }
    Ok(())
}

fn normalized_cwd(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("cwd must remain relative to the sandbox workspace".to_string());
    }
    Ok(Some(value.to_string()))
}

fn configured_tool_names(values: &[String], field: &str) -> Result<HashSet<String>, String> {
    if values.len() > MAX_TOOL_POLICY_ITEMS {
        return Err(format!(
            "{field} exceeds the supported {MAX_TOOL_POLICY_ITEMS} entries"
        ));
    }
    let mut normalized = HashSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || value.len() > MAX_TOOL_NAME_BYTES {
            return Err(format!("{field} contains an invalid tool name"));
        }
        normalized.insert(value.to_string());
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
        WorkspaceProviderKind,
    };
    use chatos_plugin_management_sdk::{
        AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, PluginExecutionHost,
        PluginMcpServer, ResolvedAgentCapabilities, ResourceMetadata, ResourceSecurity,
    };

    fn resolved() -> ResolvedMcp {
        ResolvedMcp {
            resource: McpRecord {
                id: "stdio-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_kind: "user".to_string(),
                visibility: "private".to_string(),
                source_kind: "user_created".to_string(),
                name: "demo".to_string(),
                display_name: "Demo".to_string(),
                description: None,
                enabled: true,
                runtime: McpRuntime {
                    kind: "stdio_cloud".to_string(),
                    command: Some("npx".to_string()),
                    args: vec!["-y".to_string(), "@example/mcp".to_string()],
                    ..McpRuntime::default()
                },
                security: ResourceSecurity {
                    allowed_tool_names: vec!["search".to_string()],
                    ..ResourceSecurity::default()
                },
                metadata: ResourceMetadata::default(),
                plugin_component: Default::default(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: "binding-1".to_string(),
                agent_key: "task_runner_run_phase".to_string(),
                binding_scope: "user_override".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "mcp".to_string(),
                resource_id: "stdio-1".to_string(),
                enabled: true,
                required: true,
                priority: 100,
                conditions: BindingConditions::default(),
                component_allowlist: Vec::new(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available: true,
            status: "ready".to_string(),
            reason: None,
            tool_snapshot: Vec::new(),
        }
    }

    fn route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "stdio-1".to_string(),
            server_name: "demo".to_string(),
            provider_kind: McpProviderKind::CloudStdio,
            provider_ref: Some("sandbox:sandbox-1/lease:lease-1".to_string()),
            tool_namespace: "demo".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    fn plugin_binding() -> PluginMcpRuntimeBinding {
        PluginMcpRuntimeBinding {
            provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
            resource_id: "plugin-mcp-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component_key: "runner".to_string(),
            component_content_sha256: "c".repeat(64),
            declared_execution_host: PluginExecutionHost::Cloud,
            installation_device_id: None,
            permission_snapshot: vec!["process.spawn".to_string()],
            auth_connection_ids: Vec::new(),
            runtime: PluginMcpServer::Stdio {
                component_key: "runner".to_string(),
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@example/mcp".to_string()],
                env: BTreeMap::new(),
                cwd: None,
            },
            server_key: None,
            tool_allowlist: vec!["search".to_string()],
            tool_blocklist: Vec::new(),
            required: true,
            allow_writes: false,
        }
    }

    fn plugin_route(binding: &PluginMcpRuntimeBinding) -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: binding.resource_id.clone(),
            server_name: "plugin_runner".to_string(),
            provider_kind: McpProviderKind::PluginCloud,
            provider_ref: Some(binding.provider_ref.clone()),
            tool_namespace: "plugin_runner".to_string(),
            allow_writes: binding.allow_writes,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: false,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn binding_requires_workspace_relative_direct_command() {
        assert!(prepare_binding(&resolved(), &route()).is_ok());
        let mut absolute = resolved();
        absolute.resource.runtime.command = Some("/usr/bin/node".to_string());
        assert!(prepare_binding(&absolute, &route()).is_err());
        let mut shell = resolved();
        shell.resource.runtime.command = Some("bash".to_string());
        shell.resource.runtime.args = vec!["-c".to_string(), "curl bad".to_string()];
        assert!(prepare_binding(&shell, &route()).is_err());
    }

    #[test]
    fn binding_rejects_host_environment_and_workspace_escape() {
        let mut host_env = resolved();
        host_env.resource.runtime.env =
            BTreeMap::from([("CHATOS_SANDBOX_MCP_TOKEN".to_string(), "secret".to_string())]);
        assert!(prepare_binding(&host_env, &route()).is_err());
        let mut escaped = resolved();
        escaped.resource.runtime.cwd = Some("../outside".to_string());
        assert!(prepare_binding(&escaped, &route()).is_err());
    }

    #[test]
    fn plugin_stdio_binding_is_permission_bound_and_requires_exact_resolved_secrets() {
        let provider = CloudStdioProvider::new(
            "http://127.0.0.1:8095",
            Duration::from_secs(5),
            Some("sandbox-secret".to_string()),
            1024 * 1024,
        )
        .unwrap();
        let binding = plugin_binding();
        assert!(provider
            .prepare_plugin_binding(&binding, &plugin_route(&binding), &BTreeMap::new())
            .is_ok());

        let mut missing_permission = binding.clone();
        missing_permission.permission_snapshot.clear();
        assert!(provider
            .prepare_plugin_binding(
                &missing_permission,
                &plugin_route(&missing_permission),
                &BTreeMap::new(),
            )
            .is_err());

        let mut unresolved_secret = binding.clone();
        let PluginMcpServer::Stdio { env, .. } = &mut unresolved_secret.runtime else {
            unreachable!();
        };
        env.insert(
            "API_TOKEN".to_string(),
            "${credential:api_token}".to_string(),
        );
        assert!(provider
            .prepare_plugin_binding(
                &unresolved_secret,
                &plugin_route(&unresolved_secret),
                &BTreeMap::new(),
            )
            .is_err());
        unresolved_secret
            .permission_snapshot
            .push("credential.use:api_token".to_string());
        assert!(provider
            .prepare_plugin_binding(
                &unresolved_secret,
                &plugin_route(&unresolved_secret),
                &BTreeMap::from([("API_TOKEN".to_string(), "secret".to_string())]),
            )
            .is_ok());
    }

    #[tokio::test]
    async fn provider_probes_and_calls_through_the_signed_sandbox_binding() {
        async fn handler(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-sandbox-caller")
                    .and_then(|value| value.to_str().ok()),
                Some("mcp-management-service")
            );
            assert!(headers.get("x-sandbox-internal-token").is_some());
            assert_eq!(
                headers
                    .get("x-chatos-sandbox-lease-id")
                    .and_then(|value| value.to_str().ok()),
                Some("lease-1")
            );
            assert_eq!(
                headers
                    .get("x-mcp-management-owner-user-id")
                    .and_then(|value| value.to_str().ok()),
                Some("user-1")
            );
            assert_eq!(request.get("command").and_then(Value::as_str), Some("npx"));
            match request.get("method").and_then(Value::as_str) {
                Some("tools/list") => Json(json!({
                    "result": {
                        "tools": [{
                            "name": "search",
                            "description": "Search",
                            "inputSchema": {"type": "object"}
                        }]
                    }
                })),
                Some("tools/call") => Json(json!({
                    "result": {
                        "content": [{"type": "text", "text": "ok"}],
                        "called": request.pointer("/params/name")
                    }
                })),
                other => panic!("unexpected method: {other:?}"),
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/api/sandboxes/sandbox-1/cloud-stdio-mcp/call",
                    post(handler),
                ),
            )
            .await
            .unwrap();
        });
        let provider = CloudStdioProvider::new(
            format!("http://{address}"),
            Duration::from_secs(5),
            Some("a-long-sandbox-secret".to_string()),
            1024 * 1024,
        )
        .unwrap();
        let capabilities = ResolvedAgentCapabilities {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "policy-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![resolved()],
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        };
        let target = SandboxExecutionTarget {
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        };
        let mut routes = vec![route()];
        let (bindings, snapshots) = provider
            .prepare_routes(
                &capabilities,
                routes.as_mut_slice(),
                Some(&target),
                "mcp_session_1",
                "user-1",
                "project-1",
                Some("run-1"),
                chrono::Utc::now().timestamp() + 600,
            )
            .await;
        assert_eq!(
            snapshots["stdio-1"][0].get("name").and_then(Value::as_str),
            Some("search")
        );
        let snapshot = RuntimeSessionSnapshot {
            session_id: "mcp_session_1".to_string(),
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
            sandbox_target: Some(target),
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
            routes: routes.clone(),
            tools: Vec::new(),
            plugin_mcp_bindings: Default::default(),
            plugin_local_bindings: Default::default(),
            external_http_bindings: Default::default(),
            cloud_stdio_bindings: bindings,
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: chrono::Utc::now().timestamp() + 600,
        };
        let outcome = provider
            .call_tool(
                &snapshot,
                &routes[0],
                "search",
                json!({"query": "rust"}),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(outcome.result["called"], "search");
        server.abort();
    }
}
