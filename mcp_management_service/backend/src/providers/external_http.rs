// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_plugin_management_sdk::{PluginMcpServer, ResolvedAgentCapabilities, ResolvedMcp};

use crate::runtime::{ExternalHttpProviderBinding, PluginMcpRuntimeBinding};

use super::ProviderCallError;

mod runtime_calls;
mod validation;
use validation::*;
pub(crate) use validation::{build_pinned_external_http_client, header_is_managed_or_unsafe};

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
}

#[cfg(test)]
mod tests;
