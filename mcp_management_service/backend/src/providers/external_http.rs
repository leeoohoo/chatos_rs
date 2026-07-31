// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::{
    MCP_ERROR_AUTH_REQUIRED, METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL,
};
use chatos_plugin_management_sdk::{PluginMcpServer, ResolvedAgentCapabilities, ResolvedMcp};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::redirect::Policy;
use serde_json::{json, Value};

use crate::runtime::{
    ExternalHttpProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot,
};

use super::project_service::decode_jsonrpc_response;
use super::{
    decode_cancel_notification_response, ProviderCallError, ProviderCallOutcome,
    ProviderCancelOutcome,
};

const JSON_CONTENT_TYPE: &str = "application/json";
const MAX_CONFIGURED_HEADERS: usize = 64;
const MAX_CONFIGURED_HEADER_BYTES: usize = 32 * 1024;
const MAX_TOOL_POLICY_ITEMS: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 256;

#[derive(Clone)]
pub(super) struct ExternalHttpProvider {
    request_timeout: Duration,
    response_limit_bytes: usize,
}

impl ExternalHttpProvider {
    pub(super) fn new(request_timeout: Duration, response_limit_bytes: usize) -> Self {
        Self {
            request_timeout,
            response_limit_bytes,
        }
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        let expected_provider_ref = format!("mcp-resource:{}", route.resource_id);
        route.provider_kind == McpProviderKind::ExternalHttp
            && route.provider_ref.as_deref() == Some(expected_provider_ref.as_str())
    }

    pub(super) async fn prepare_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
    ) -> HashMap<String, ExternalHttpProviderBinding> {
        let resources = capabilities
            .mcps
            .iter()
            .map(|resolved| (resolved.resource.id.as_str(), resolved))
            .collect::<HashMap<_, _>>();
        let mut bindings = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::ExternalHttp)
        {
            let binding = match resources.get(route.resource_id.as_str()) {
                Some(resolved) => self.prepare_binding(resolved, route).await,
                None => Err("capability resource is missing".to_string()),
            };
            match binding {
                Ok(binding) => {
                    bindings.insert(route.resource_id.clone(), binding);
                }
                Err(reason) => {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.allow_writes = false;
                    route.cancel_supported = false;
                    route.reason =
                        format!("External HTTP MCP configuration is unavailable: {reason}");
                }
            }
        }
        bindings
    }

    async fn prepare_binding(
        &self,
        resolved: &ResolvedMcp,
        route: &ResolvedMcpRoute,
    ) -> Result<ExternalHttpProviderBinding, String> {
        if resolved.resource.runtime.kind.trim() != "http" {
            return Err("runtime kind is not http".to_string());
        }
        let provider_ref = format!("mcp-resource:{}", resolved.resource.id);
        if route.provider_ref.as_deref() != Some(provider_ref.as_str()) {
            return Err("route target does not match the configured resource".to_string());
        }
        let endpoint = resolved.resource.runtime.url.as_deref().unwrap_or_default();
        self.prepare_bound_http(
            provider_ref,
            endpoint,
            &resolved.resource.runtime.headers,
            route.allow_writes,
            resolved.resource.security.allowed_tool_names.as_slice(),
            resolved.resource.security.blocked_tool_names.as_slice(),
        )
        .await
    }

    pub(super) async fn prepare_plugin_binding(
        &self,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        resolved_runtime: &PluginMcpServer,
        resolved_headers: &std::collections::BTreeMap<String, String>,
    ) -> Result<ExternalHttpProviderBinding, String> {
        if route.provider_kind != McpProviderKind::PluginCloud
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.resource_id != immutable.resource_id
            || route.allow_writes != immutable.allow_writes
        {
            return Err("Plugin HTTP route does not match its immutable binding".to_string());
        }
        let PluginMcpServer::Http {
            url,
            headers,
            oauth_resource,
            ..
        } = resolved_runtime
        else {
            return Err("Plugin MCP runtime is not HTTP".to_string());
        };
        validate_plugin_resolved_headers(
            headers,
            resolved_headers,
            oauth_resource.is_some(),
            immutable.permission_snapshot.as_slice(),
        )?;
        let endpoint = validate_endpoint(url)?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| "endpoint has no host".to_string())?
            .to_ascii_lowercase();
        let permission = format!("network.domain:{host}");
        if !immutable
            .permission_snapshot
            .iter()
            .any(|configured| configured == &permission)
        {
            return Err(format!(
                "Plugin HTTP MCP requires {permission} in its immutable permission snapshot"
            ));
        }
        let effective_headers = resolved_headers
            .iter()
            .filter(|(name, _)| {
                !matches!(
                    name.trim().to_ascii_lowercase().as_str(),
                    "accept" | "content-type"
                )
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        self.prepare_bound_http(
            immutable.provider_ref.clone(),
            url,
            &effective_headers,
            route.allow_writes,
            immutable.tool_allowlist.as_slice(),
            immutable.tool_blocklist.as_slice(),
        )
        .await
    }

    async fn prepare_bound_http(
        &self,
        provider_ref: String,
        endpoint: &str,
        configured_header_values: &std::collections::BTreeMap<String, String>,
        allow_writes: bool,
        configured_allowed_tools: &[String],
        configured_blocked_tools: &[String],
    ) -> Result<ExternalHttpProviderBinding, String> {
        let endpoint = validate_endpoint(endpoint)?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| "endpoint has no host".to_string())?;
        let port = endpoint
            .port_or_known_default()
            .ok_or_else(|| "endpoint has no usable port".to_string())?;
        let addresses = tokio::time::timeout(
            Duration::from_secs(10).min(self.request_timeout),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| "endpoint DNS resolution timed out".to_string())?
        .map_err(|_| "endpoint DNS resolution failed".to_string())?
        .collect::<Vec<_>>();
        let mut addresses = addresses;
        addresses.sort_unstable();
        addresses.dedup();
        if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
            return Err("endpoint must resolve only to public network addresses".to_string());
        }
        let headers = configured_headers(configured_header_values)?;
        let allowed_tool_names =
            configured_tool_names(configured_allowed_tools, "allowed_tool_names")?;
        let blocked_tool_names =
            configured_tool_names(configured_blocked_tools, "blocked_tool_names")?;
        if !allow_writes && allowed_tool_names.is_empty() {
            return Err(
                "read-only endpoint requires an explicit allowed_tool_names policy".to_string(),
            );
        }
        let http = build_pinned_external_http_client(
            &endpoint,
            addresses.as_slice(),
            self.request_timeout,
        )?;
        Ok(ExternalHttpProviderBinding {
            provider_ref,
            endpoint,
            headers,
            http,
            resolved_addresses: addresses,
            allow_writes,
            allowed_tool_names,
            blocked_tool_names,
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
            .external_http_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "External HTTP MCP runtime binding is missing",
                )
            })?;
        if route.provider_kind != McpProviderKind::ExternalHttp
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
            || route.allow_writes != binding.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "External HTTP MCP route does not match its runtime binding",
            ));
        }
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is blocked by the External HTTP MCP policy".to_string(),
            });
        }
        self.call_bound_tool(
            binding,
            original_tool_name,
            arguments,
            invocation_id,
            "External HTTP MCP",
        )
        .await
    }

    pub(super) async fn list_tools_for_binding(
        &self,
        binding: &ExternalHttpProviderBinding,
        request_id: &str,
        provider_label: &str,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let response = binding
            .http
            .post(binding.endpoint.clone())
            .headers(binding.headers.clone())
            .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .header(ACCEPT, JSON_CONTENT_TYPE)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .map_err(|_| {
                ProviderCallError::provider_unavailable(format!("{provider_label} request failed"))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(if matches!(status.as_u16(), 401 | 403) {
                ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: format!("{provider_label} rejected its configured credentials"),
                }
            } else {
                ProviderCallError::provider_unavailable(format!(
                    "{provider_label} returned HTTP {}",
                    status.as_u16()
                ))
            });
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), request_id, provider_label)?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} tools/list response has no tools array"
                ))
            })?;
        if tools.iter().any(|tool| {
            !tool.is_object()
                || tool
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .is_none_or(str::is_empty)
        }) {
            return Err(ProviderCallError::invalid_response(format!(
                "{provider_label} tools/list returned an invalid tool definition"
            )));
        }
        Ok(tools)
    }

    pub(super) async fn call_bound_tool(
        &self,
        binding: &ExternalHttpProviderBinding,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
        provider_label: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: format!("tool is blocked by the {provider_label} policy"),
            });
        }
        let response = binding
            .http
            .post(binding.endpoint.clone())
            .headers(binding.headers.clone())
            .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .header(ACCEPT, JSON_CONTENT_TYPE)
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
            .map_err(|_| {
                ProviderCallError::provider_unavailable(format!("{provider_label} request failed"))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(if matches!(status.as_u16(), 401 | 403) {
                ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: format!("{provider_label} rejected its configured credentials"),
                }
            } else {
                ProviderCallError::provider_unavailable(format!(
                    "{provider_label} returned HTTP {}",
                    status.as_u16()
                ))
            });
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, provider_label)?;
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
        let binding = snapshot
            .external_http_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "External HTTP MCP runtime binding is missing",
                )
            })?;
        if route.provider_kind != McpProviderKind::ExternalHttp
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
        {
            return Err(ProviderCallError::provider_unavailable(
                "External HTTP MCP route does not match its runtime binding",
            ));
        }
        self.cancel_bound_invocation(binding, invocation_id, "External HTTP MCP")
            .await
    }

    pub(super) async fn cancel_bound_invocation(
        &self,
        binding: &ExternalHttpProviderBinding,
        invocation_id: &str,
        provider_label: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let response = binding
            .http
            .post(binding.endpoint.clone())
            .headers(binding.headers.clone())
            .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .header(ACCEPT, JSON_CONTENT_TYPE)
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
            .map_err(|_| {
                ProviderCallError::provider_unavailable(format!(
                    "{provider_label} cancellation request failed"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), provider_label)
    }
}

fn validate_plugin_resolved_headers(
    templates: &std::collections::BTreeMap<String, String>,
    resolved: &std::collections::BTreeMap<String, String>,
    oauth_enabled: bool,
    permissions: &[String],
) -> Result<(), String> {
    let mut expected = std::collections::BTreeSet::new();
    let mut uses_credentials = false;
    for (name, template) in templates {
        let normalized = name.trim().to_ascii_lowercase();
        HeaderName::from_bytes(normalized.as_bytes())
            .map_err(|_| format!("Plugin HTTP header is invalid: {normalized}"))?;
        let uses_template_credential = template.contains("${credential:");
        if !uses_template_credential
            && !matches!(
                normalized.as_str(),
                "accept"
                    | "accept-language"
                    | "content-type"
                    | "mcp-protocol-version"
                    | "user-agent"
                    | "x-plugin-client"
            )
        {
            return Err(format!(
                "Plugin HTTP custom header must use a credential template: {normalized}"
            ));
        }
        uses_credentials |= uses_template_credential;
        expected.insert(normalized);
    }
    if oauth_enabled {
        if expected.contains("authorization") {
            return Err(
                "Plugin HTTP MCP cannot combine OAuth with an Authorization template".to_string(),
            );
        }
        expected.insert("authorization".to_string());
    }
    let actual = resolved
        .keys()
        .map(|name| name.trim().to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    if expected != actual {
        return Err(
            "Plugin HTTP resolved headers do not match the immutable templates".to_string(),
        );
    }
    if uses_credentials
        && !permissions.iter().any(|permission| {
            permission == "credential.use" || permission.starts_with("credential.use:")
        })
    {
        return Err(
            "Plugin HTTP credentials require credential.use in the immutable permission snapshot"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<reqwest::Url, String> {
    let endpoint =
        reqwest::Url::parse(value.trim()).map_err(|_| "endpoint URL is invalid".to_string())?;
    validate_endpoint_url(&endpoint)?;
    Ok(endpoint)
}

fn validate_endpoint_url(endpoint: &reqwest::Url) -> Result<(), String> {
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("endpoint must use HTTPS without URL credentials or fragments".to_string());
    }
    Ok(())
}

pub(crate) fn build_pinned_external_http_client(
    endpoint: &reqwest::Url,
    addresses: &[SocketAddr],
    request_timeout: Duration,
) -> Result<reqwest::Client, String> {
    validate_endpoint_url(endpoint)?;
    let host = endpoint
        .host_str()
        .ok_or_else(|| "endpoint has no host".to_string())?;
    let port = endpoint
        .port_or_known_default()
        .ok_or_else(|| "endpoint has no usable port".to_string())?;
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| address.port() != port || !is_public_ip(address.ip()))
    {
        return Err(
            "endpoint must remain pinned only to public addresses on its configured port"
                .to_string(),
        );
    }
    reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10).min(request_timeout))
        .timeout(request_timeout)
        .resolve_to_addrs(host, addresses)
        .build()
        .map_err(|_| "build endpoint client failed".to_string())
}

fn configured_headers(
    configured: &std::collections::BTreeMap<String, String>,
) -> Result<HeaderMap, String> {
    if configured.len() > MAX_CONFIGURED_HEADERS {
        return Err(format!(
            "headers exceed the supported {MAX_CONFIGURED_HEADERS} entries"
        ));
    }
    let encoded_bytes = configured
        .iter()
        .map(|(name, value)| name.len().saturating_add(value.len()))
        .sum::<usize>();
    if encoded_bytes > MAX_CONFIGURED_HEADER_BYTES {
        return Err(format!(
            "headers exceed the supported {MAX_CONFIGURED_HEADER_BYTES} bytes"
        ));
    }
    let mut headers = HeaderMap::new();
    for (name, value) in configured {
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "headers contain an invalid name".to_string())?;
        if header_is_managed_or_unsafe(&name) {
            return Err(format!(
                "header {} is managed by MCP Management and cannot be configured",
                name.as_str()
            ));
        }
        let mut value = HeaderValue::from_str(value)
            .map_err(|_| "headers contain an invalid value".to_string())?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

pub(crate) fn header_is_managed_or_unsafe(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
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
            | "x-local-connector-internal-scope"
            | "x-local-connector-internal-secret"
            | "x-local-connector-internal-token"
            | "x-project-service-internal-scope"
            | "x-project-service-internal-token"
            | "x-project-service-sync-secret"
            | "x-sandbox-client-key"
            | "x-sandbox-internal-scope"
            | "x-sandbox-internal-token"
    )
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

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap as AxumHeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
        WorkspaceProviderKind,
    };

    use super::*;

    fn route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "external-1".to_string(),
            server_name: "demo".to_string(),
            provider_kind: McpProviderKind::ExternalHttp,
            provider_ref: Some("mcp-resource:external-1".to_string()),
            tool_namespace: "demo".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    fn snapshot(binding: ExternalHttpProviderBinding) -> RuntimeSessionSnapshot {
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
                sandbox_provider: SandboxProviderKind::None,
                sandbox_pairing_id: None,
                source_type: Some("cloud".to_string()),
                revision: "project-revision".to_string(),
            },
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: vec![route()],
            tools: Vec::new(),
            plugin_mcp_bindings: Default::default(),
            plugin_local_bindings: Default::default(),
            plugin_tool_component_bindings: Default::default(),
            plugin_local_tool_component_bindings: Default::default(),
            plugin_cloud_tool_component_bindings: Default::default(),
            external_http_bindings: HashMap::from([("external-1".to_string(), binding)]),
            cloud_stdio_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: i64::MAX,
        }
    }

    #[test]
    fn endpoint_requires_plain_https() {
        assert!(validate_endpoint("https://mcp.example.com/rpc?tenant=one").is_ok());
        assert!(validate_endpoint("http://mcp.example.com/rpc").is_err());
        assert!(validate_endpoint("https://user@mcp.example.com/rpc").is_err());
        assert!(validate_endpoint("https://mcp.example.com/rpc#fragment").is_err());
    }

    #[test]
    fn private_and_special_network_addresses_are_rejected() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(value.parse().expect("test IP")), "{value}");
        }
        assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
        assert!(is_public_ip(
            "2606:4700:4700::1111".parse().expect("public IPv6")
        ));
    }

    #[test]
    fn managed_and_hop_by_hop_headers_are_rejected() {
        assert!(configured_headers(&std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer secret".to_string(),
        )]))
        .is_ok());
        for name in [
            "host",
            "content-type",
            "connection",
            "x-project-service-sync-secret",
        ] {
            assert!(configured_headers(&std::collections::BTreeMap::from([(
                name.to_string(),
                "value".to_string(),
            )]))
            .is_err());
        }
    }

    #[test]
    fn plugin_http_headers_require_exact_cloud_credential_resolution() {
        assert!(validate_plugin_resolved_headers(
            &std::collections::BTreeMap::from([(
                "x-plugin-client".to_string(),
                "chatos".to_string(),
            )]),
            &std::collections::BTreeMap::from([(
                "x-plugin-client".to_string(),
                "chatos".to_string(),
            )]),
            false,
            &[],
        )
        .is_ok());
        assert!(validate_plugin_resolved_headers(
            &std::collections::BTreeMap::from([(
                "authorization".to_string(),
                "Bearer ${credential:access_token}".to_string(),
            )]),
            &std::collections::BTreeMap::from([(
                "authorization".to_string(),
                "Bearer secret".to_string(),
            )]),
            false,
            &[],
        )
        .is_err());
        assert!(validate_plugin_resolved_headers(
            &std::collections::BTreeMap::from([(
                "authorization".to_string(),
                "Bearer ${credential:access_token}".to_string(),
            )]),
            &std::collections::BTreeMap::from([(
                "authorization".to_string(),
                "Bearer secret".to_string(),
            )]),
            false,
            &["credential.use:access_token".to_string()],
        )
        .is_ok());
        assert!(validate_plugin_resolved_headers(
            &std::collections::BTreeMap::from([(
                "x-custom-auth".to_string(),
                "static-secret".to_string(),
            )]),
            &std::collections::BTreeMap::from([(
                "x-custom-auth".to_string(),
                "static-secret".to_string(),
            )]),
            false,
            &[],
        )
        .is_err());
    }

    #[test]
    fn tool_policy_uses_allowlist_then_blocklist() {
        let binding = ExternalHttpProviderBinding {
            provider_ref: "mcp-resource:one".to_string(),
            endpoint: reqwest::Url::parse("https://mcp.example.com").unwrap(),
            headers: HeaderMap::new(),
            http: reqwest::Client::new(),
            resolved_addresses: vec!["8.8.8.8:443".parse().unwrap()],
            allow_writes: false,
            allowed_tool_names: HashSet::from(["search".to_string(), "delete".to_string()]),
            blocked_tool_names: HashSet::from(["delete".to_string()]),
        };
        assert!(binding.allows_tool("search"));
        assert!(!binding.allows_tool("delete"));
        assert!(!binding.allows_tool("unknown"));
    }

    #[tokio::test]
    async fn call_uses_private_binding_headers_and_original_tool_name() {
        async fn handler(headers: AxumHeaderMap, Json(request): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer external-secret")
            );
            assert_eq!(
                request.get("method").and_then(Value::as_str),
                Some("tools/call")
            );
            assert_eq!(
                request.pointer("/params/name").and_then(Value::as_str),
                Some("search")
            );
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap(),
                "result": {"content": [{"type": "text", "text": "ok"}]}
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(handler)))
                .await
                .unwrap();
        });
        let binding = ExternalHttpProviderBinding {
            provider_ref: "mcp-resource:external-1".to_string(),
            endpoint: reqwest::Url::parse(format!("http://{address}/mcp").as_str()).unwrap(),
            headers: configured_headers(&std::collections::BTreeMap::from([(
                "authorization".to_string(),
                "Bearer external-secret".to_string(),
            )]))
            .unwrap(),
            http: reqwest::Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            resolved_addresses: vec![address],
            allow_writes: false,
            allowed_tool_names: HashSet::from(["search".to_string()]),
            blocked_tool_names: HashSet::new(),
        };
        let outcome = ExternalHttpProvider::new(Duration::from_secs(5), 64 * 1024)
            .call_tool(
                &snapshot(binding),
                &route(),
                "search",
                json!({"query": "hello"}),
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(
            outcome
                .result
                .pointer("/content/0/text")
                .and_then(Value::as_str),
            Some("ok")
        );
        server.abort();
    }

    #[tokio::test]
    async fn bound_http_cancellation_forwards_the_exact_invocation_id_and_headers() {
        async fn handler(headers: AxumHeaderMap, Json(request): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer plugin-secret")
            );
            assert_eq!(
                request.get("method").and_then(Value::as_str),
                Some(METHOD_NOTIFICATIONS_CANCELLED)
            );
            assert_eq!(
                request.pointer("/params/requestId").and_then(Value::as_str),
                Some("invocation-plugin-http")
            );
            Json(json!({"result": {"status": "cancelled"}}))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(handler)))
                .await
                .unwrap();
        });
        let binding = ExternalHttpProviderBinding {
            provider_ref: "plugin-binding:test".to_string(),
            endpoint: reqwest::Url::parse(format!("http://{address}/mcp").as_str()).unwrap(),
            headers: configured_headers(&std::collections::BTreeMap::from([(
                "authorization".to_string(),
                "Bearer plugin-secret".to_string(),
            )]))
            .unwrap(),
            http: reqwest::Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            resolved_addresses: vec![address],
            allow_writes: true,
            allowed_tool_names: HashSet::new(),
            blocked_tool_names: HashSet::new(),
        };
        let outcome = ExternalHttpProvider::new(Duration::from_secs(5), 64 * 1024)
            .cancel_bound_invocation(&binding, "invocation-plugin-http", "Plugin Cloud HTTP MCP")
            .await
            .unwrap();
        assert_eq!(outcome, ProviderCancelOutcome::Cancelled);
        server.abort();
    }
}
