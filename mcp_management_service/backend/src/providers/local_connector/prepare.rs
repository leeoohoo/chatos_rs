// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp_management_sdk::{McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute};
use chatos_mcp_runtime::extract_tools;
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, ResolvedMcp};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::providers::project_service::decode_jsonrpc_response;
use crate::runtime::{LocalConnectorInlineHttpRuntime, LocalConnectorMcpProviderBinding};
use crate::trace_context::InternalTraceContextExt;

use super::{
    LocalConnectorProvider, ProviderCallError, CALLER_SERVICE,
    LOCAL_CONNECTOR_INLINE_MCP_RUNTIME_HEADER, LOCAL_CONNECTOR_MCP_MANIFEST_ID_HEADER,
    MCP_RELAY_SCOPE, PLUGIN_MANAGEMENT_RESOURCE_ID_HEADER, TOKEN_AUDIENCE,
};

const DEFAULT_INLINE_HTTP_TIMEOUT_MS: u64 = 30_000;
const MAX_CONFIGURED_HEADERS: usize = 64;
const MAX_CONFIGURED_HEADER_BYTES: usize = 32 * 1024;
const MAX_TOOL_POLICY_ITEMS: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 256;

impl LocalConnectorProvider {
    pub(in crate::providers) async fn prepare_mcp_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        owner_user_id: &str,
    ) -> (
        HashMap<String, LocalConnectorMcpProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let resources = capabilities
            .mcps
            .iter()
            .map(|resolved| (resolved.resource.id.as_str(), resolved))
            .collect::<HashMap<_, _>>();
        let mut bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes.iter_mut().filter(|route| {
            route.provider_kind == McpProviderKind::LocalConnector
                && system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_none()
        }) {
            route.cancel_supported = false;
            let binding = resources
                .get(route.resource_id.as_str())
                .ok_or_else(|| "capability resource is missing".to_string())
                .and_then(|resolved| prepare_binding(resolved, route, context));
            match binding {
                Ok(binding) => {
                    match self
                        .list_mcp_tools(owner_user_id, route.resource_id.as_str(), &binding)
                        .await
                    {
                        Ok(tools) => {
                            tool_snapshots.insert(route.resource_id.clone(), tools);
                            bindings.insert(route.resource_id.clone(), binding);
                        }
                        Err(error) => make_route_unavailable(route, error.message.as_str()),
                    }
                }
                Err(reason) => make_route_unavailable(route, reason.as_str()),
            }
        }
        (bindings, tool_snapshots)
    }

    async fn list_mcp_tools(
        &self,
        owner_user_id: &str,
        resource_id: &str,
        binding: &LocalConnectorMcpProviderBinding,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
            owner_user_id,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/mcp",
                self.base_url,
                urlencoding::encode(binding.device_id.as_str())
            )
            .as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Local Connector MCP inspection URL failed: {error}"
            ))
        })?;
        if let Some(workspace_id) = binding.workspace_id.as_deref() {
            url.query_pairs_mut().append_pair("workspace_id", workspace_id);
        }
        let mut request = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .header(PLUGIN_MANAGEMENT_RESOURCE_ID_HEADER, resource_id)
            .with_internal_trace_context();
        if let Some(manifest_id) = binding.manifest_id.as_deref() {
            request = request.header(LOCAL_CONNECTOR_MCP_MANIFEST_ID_HEADER, manifest_id);
        }
        if let Some(inline_http) = binding.inline_http.as_ref() {
            let encoded = serde_json::to_string(inline_http).map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "serialize inline Local Connector MCP runtime failed: {error}"
                ))
            })?;
            request = request.header(
                LOCAL_CONNECTOR_INLINE_MCP_RUNTIME_HEADER,
                urlencoding::encode(encoded.as_str()).into_owned(),
            );
        }
        let request_id = format!("mcp_prepare_{}", Uuid::new_v4().simple());
        let response = request
            .json(&json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "tools/list",
                "params": {}
            }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Connector MCP tools/list request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Connector MCP tools/list response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Connector MCP tools/list returned HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(
            bytes.as_slice(),
            request_id.as_str(),
            "Local Connector MCP tools/list",
        )?;
        let tools = extract_tools(&result).map_err(ProviderCallError::invalid_response)?;
        if tools.is_empty() {
            return Err(ProviderCallError::invalid_response(
                "Local Connector MCP tools/list returned no tools",
            ));
        }
        Ok(tools)
    }
}

fn prepare_binding(
    resolved: &ResolvedMcp,
    route: &ResolvedMcpRoute,
    context: &ProjectExecutionContext,
) -> Result<LocalConnectorMcpProviderBinding, String> {
    if !resolved.available {
        return Err(resolved
            .reason
            .clone()
            .unwrap_or_else(|| "MCP resource is unavailable on Local Connector".to_string()));
    }
    let provider_ref = format!("mcp-resource:{}", resolved.resource.id);
    if route.provider_ref.as_deref() != Some(provider_ref.as_str()) {
        return Err("route target does not match the configured MCP resource".to_string());
    }
    let local = resolved.resource.runtime.local_connector.as_ref();
    let workspace = context.workspace.as_ref();
    let device_id = normalized(local.and_then(|item| item.device_id.as_deref()))
        .or_else(|| workspace.and_then(|item| normalized(item.device_id.as_deref())))
        .ok_or_else(|| "Local Connector device id is missing".to_string())?;
    let workspace_id = normalized(local.and_then(|item| item.workspace_id.as_deref()))
        .or_else(|| {
            workspace
                .filter(|item| item.device_id.as_deref() == Some(device_id))
                .and_then(|item| normalized(Some(item.workspace_id.as_str())))
        })
        .map(ToOwned::to_owned);
    let allowed_tool_names = configured_tool_names(
        resolved.resource.security.allowed_tool_names.as_slice(),
        "allowed_tool_names",
    )?;
    let blocked_tool_names = configured_tool_names(
        resolved.resource.security.blocked_tool_names.as_slice(),
        "blocked_tool_names",
    )?;
    if !route.allow_writes && allowed_tool_names.is_empty() {
        return Err("read-only MCP requires an explicit allowed_tool_names policy".to_string());
    }
    let runtime_kind = resolved.resource.runtime.kind.trim();
    let (manifest_id, inline_http) = match runtime_kind {
        "local_connector_stdio" | "local_connector_http" | "local_connector_builtin_proxy" => {
            let manifest_id = normalized(local.and_then(|item| item.manifest_id.as_deref()))
                .ok_or_else(|| "Local Connector MCP manifest id is missing".to_string())?;
            (Some(manifest_id.to_string()), None)
        }
        "http" => {
            let url = resolved
                .resource
                .runtime
                .url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "HTTP MCP URL is missing".to_string())?;
            validate_inline_http_url(url)?;
            validate_inline_http_headers(&resolved.resource.runtime.headers)?;
            (
                None,
                Some(LocalConnectorInlineHttpRuntime {
                    url: url.to_string(),
                    headers: resolved.resource.runtime.headers.clone(),
                    timeout_ms: DEFAULT_INLINE_HTTP_TIMEOUT_MS,
                }),
            )
        }
        _ => {
            return Err(format!(
                "unsupported Local Connector MCP runtime kind: {runtime_kind}"
            ))
        }
    };
    Ok(LocalConnectorMcpProviderBinding {
        provider_ref,
        device_id: device_id.to_string(),
        workspace_id,
        manifest_id,
        inline_http,
        allow_writes: route.allow_writes,
        allowed_tool_names,
        blocked_tool_names,
    })
}

fn validate_inline_http_url(value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|_| "HTTP MCP URL is invalid".to_string())?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("HTTP MCP URL cannot contain credentials or a fragment".to_string());
    }
    match url.scheme() {
        "https" if url.host_str().is_some() => Ok(()),
        "http" if is_loopback_host(&url) => Ok(()),
        _ => Err("HTTP MCP must use HTTPS, or HTTP with a loopback host".to_string()),
    }
}

fn is_loopback_host(url: &reqwest::Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .ok()
                .is_some_and(|ip| ip.is_loopback())
    })
}

fn validate_inline_http_headers(
    headers: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    if headers.len() > MAX_CONFIGURED_HEADERS {
        return Err(format!(
            "HTTP MCP headers exceed {MAX_CONFIGURED_HEADERS} entries"
        ));
    }
    let bytes = headers
        .iter()
        .map(|(name, value)| name.len().saturating_add(value.len()))
        .sum::<usize>();
    if bytes > MAX_CONFIGURED_HEADER_BYTES {
        return Err(format!(
            "HTTP MCP headers exceed {MAX_CONFIGURED_HEADER_BYTES} bytes"
        ));
    }
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "HTTP MCP contains an invalid header name".to_string())?;
        HeaderValue::from_str(value)
            .map_err(|_| "HTTP MCP contains an invalid header value".to_string())?;
        if managed_or_unsafe_header(name.as_str()) {
            return Err(format!(
                "HTTP MCP header is managed and cannot be configured: {name}"
            ));
        }
    }
    Ok(())
}

fn managed_or_unsafe_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "x-local-connector-inline-mcp-runtime"
            | "x-local-connector-mcp-manifest-id"
            | "x-plugin-management-resource-id"
    )
}

fn configured_tool_names(values: &[String], field: &str) -> Result<HashSet<String>, String> {
    if values.len() > MAX_TOOL_POLICY_ITEMS {
        return Err(format!("{field} exceeds {MAX_TOOL_POLICY_ITEMS} entries"));
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

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Local Connector MCP unavailable: {reason}");
}
