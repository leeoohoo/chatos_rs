// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::collections::HashSet;

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, SandboxExecutionTarget,
    WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{
    plugin_mcp_cloud_runtime_bundle_sha256, PluginExecutionHost, PluginManagementClient,
    PluginMcpCloudRuntimeBundle, PluginMcpServer, ResolvePluginMcpCloudCredentialsRequest,
};
use serde_json::Value;

use crate::runtime::{
    CloudStdioProviderBinding, ExternalHttpProviderBinding, PluginMcpRuntimeBinding,
    RuntimeSessionSnapshot,
};

use super::cloud_stdio::CloudStdioProvider;
use super::external_http::ExternalHttpProvider;
use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[derive(Clone)]
pub(super) struct PluginCloudProvider {
    cloud_stdio: CloudStdioProvider,
    external_http: ExternalHttpProvider,
}

impl PluginCloudProvider {
    pub(super) fn new(
        cloud_stdio: CloudStdioProvider,
        external_http: ExternalHttpProvider,
    ) -> Self {
        Self {
            cloud_stdio,
            external_http,
        }
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        route.provider_kind == McpProviderKind::PluginCloud
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|value| value.starts_with("plugin-binding:"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_routes(
        &self,
        plugin_management: &PluginManagementClient,
        immutable_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, CloudStdioProviderBinding>,
        HashMap<String, ExternalHttpProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let mut stdio_bindings = HashMap::new();
        let mut http_bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::PluginCloud)
        {
            route.cancel_supported = false;
            let result = match immutable_bindings.get(route.resource_id.as_str()) {
                Some(immutable) => {
                    self.prepare_route(
                        plugin_management,
                        immutable,
                        route,
                        context,
                        target,
                        runtime_session_id,
                        owner_user_id,
                        project_id,
                        run_id,
                        expires_at_unix,
                    )
                    .await
                }
                None => Err(ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP binding is missing",
                )),
            };
            match result {
                Ok(PreparedPluginCloudRoute::Stdio { binding, tools }) => {
                    route.cancel_supported = true;
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                    stdio_bindings.insert(route.resource_id.clone(), *binding);
                }
                Ok(PreparedPluginCloudRoute::Http { binding, tools }) => {
                    route.cancel_supported = true;
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                    http_bindings.insert(route.resource_id.clone(), *binding);
                }
                Err(error) => make_route_unavailable(route, error.message.as_str()),
            }
        }
        (stdio_bindings, http_bindings, tool_snapshots)
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_route(
        &self,
        plugin_management: &PluginManagementClient,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        context: &ProjectExecutionContext,
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> Result<PreparedPluginCloudRoute, ProviderCallError> {
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.resource_id != immutable.resource_id
            || route.allow_writes != immutable.allow_writes
            || immutable.declared_execution_host == PluginExecutionHost::Local
            || (immutable.declared_execution_host == PluginExecutionHost::Portable
                && context.workspace_provider == WorkspaceProviderKind::LocalConnector)
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud route does not match its immutable host binding",
            ));
        }
        let bundle = plugin_management
            .get_plugin_mcp_cloud_runtime_bundle_for_service(
                immutable.plugin_id.as_str(),
                immutable.release_id.as_str(),
                immutable.component_key.as_str(),
            )
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "resolve Plugin MCP cloud runtime Bundle failed: {error}"
                ))
            })?;
        validate_runtime_bundle(immutable, &bundle)?;
        let credentials = plugin_management
            .resolve_plugin_mcp_cloud_credentials_for_service(
                immutable.plugin_id.as_str(),
                immutable.release_id.as_str(),
                immutable.component_key.as_str(),
                &ResolvePluginMcpCloudCredentialsRequest {
                    owner_user_id: owner_user_id.to_string(),
                    expected_component_content_sha256: immutable.component_content_sha256.clone(),
                    permission_snapshot: immutable.permission_snapshot.clone(),
                    auth_connection_ids: immutable.auth_connection_ids.clone(),
                    minimum_valid_until_unix: Some(expires_at_unix),
                },
            )
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "resolve Plugin cloud credentials failed: {error}"
                ))
            })?;
        if credentials.credential_snapshot_sha256.len() != 64
            || !credentials
                .credential_snapshot_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            || credentials.oauth_connection_id.as_ref().is_some_and(|id| {
                !immutable
                    .auth_connection_ids
                    .iter()
                    .any(|authorized| authorized == id)
            })
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin cloud credential response is not bound to the immutable Session",
            ));
        }
        match bundle.effective_runtime() {
            PluginMcpServer::Stdio { .. } => {
                if !credentials.headers.is_empty() || credentials.oauth_connection_id.is_some() {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin stdio credential response contains HTTP-only values",
                    ));
                }
                let target = target.ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "Plugin Cloud stdio requires a bound sandbox target",
                    )
                })?;
                let binding = self
                    .cloud_stdio
                    .prepare_plugin_binding(immutable, route, &credentials.environment, &bundle)
                    .map_err(ProviderCallError::provider_unavailable)?;
                let tools = self
                    .cloud_stdio
                    .list_plugin_tools(
                        target,
                        runtime_session_id,
                        owner_user_id,
                        project_id,
                        run_id,
                        expires_at_unix,
                        route.resource_id.as_str(),
                        &binding,
                    )
                    .await?;
                validate_tool_snapshot(tools.as_slice())?;
                Ok(PreparedPluginCloudRoute::Stdio {
                    binding: Box::new(binding),
                    tools,
                })
            }
            PluginMcpServer::Http { .. } => {
                if !credentials.environment.is_empty() {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin HTTP credential response contains stdio-only values",
                    ));
                }
                let binding = self
                    .external_http
                    .prepare_plugin_binding(
                        immutable,
                        route,
                        bundle.effective_runtime(),
                        &credentials.headers,
                    )
                    .await
                    .map_err(ProviderCallError::provider_unavailable)?;
                let request_id = format!("{runtime_session_id}.{}.tools-list", route.resource_id);
                let tools = self
                    .external_http
                    .list_tools_for_binding(&binding, request_id.as_str(), "Plugin Cloud HTTP MCP")
                    .await?;
                validate_tool_snapshot(tools.as_slice())?;
                Ok(PreparedPluginCloudRoute::Http {
                    binding: Box::new(binding),
                    tools,
                })
            }
            PluginMcpServer::ConfigFile { .. } => Err(ProviderCallError::invalid_response(
                "resolved Plugin Cloud config-file runtime is still a config file",
            )),
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
        let immutable = snapshot
            .plugin_mcp_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP runtime binding is missing",
                )
            })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.allow_writes != immutable.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud route does not match its immutable runtime binding",
            ));
        }
        let transport = match &immutable.runtime {
            PluginMcpServer::Stdio { .. } => "stdio",
            PluginMcpServer::Http { .. } => "http",
            PluginMcpServer::ConfigFile { .. } => {
                let has_stdio = snapshot
                    .cloud_stdio_bindings
                    .contains_key(route.resource_id.as_str());
                let has_http = snapshot
                    .external_http_bindings
                    .contains_key(route.resource_id.as_str());
                match (has_stdio, has_http) {
                    (true, false) => "stdio",
                    (false, true) => "http",
                    _ => {
                        return Err(ProviderCallError::provider_unavailable(
                            "Plugin Cloud config-file runtime binding is missing or ambiguous",
                        ))
                    }
                }
            }
        };
        match transport {
            "stdio" => {
                let binding = snapshot
                    .cloud_stdio_bindings
                    .get(route.resource_id.as_str())
                    .ok_or_else(|| {
                        ProviderCallError::provider_unavailable(
                            "Plugin Cloud stdio binding is missing",
                        )
                    })?;
                if binding.provider_ref != immutable.provider_ref
                    || binding.allow_writes != immutable.allow_writes
                {
                    return Err(ProviderCallError::provider_unavailable(
                        "Plugin Cloud stdio binding drifted from its immutable snapshot",
                    ));
                }
                self.cloud_stdio
                    .call_bound_tool(
                        snapshot,
                        route.resource_id.as_str(),
                        binding,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            "http" => {
                let binding = snapshot
                    .external_http_bindings
                    .get(route.resource_id.as_str())
                    .ok_or_else(|| {
                        ProviderCallError::provider_unavailable(
                            "Plugin Cloud HTTP binding is missing",
                        )
                    })?;
                if binding.provider_ref != immutable.provider_ref
                    || binding.allow_writes != immutable.allow_writes
                {
                    return Err(ProviderCallError::provider_unavailable(
                        "Plugin Cloud HTTP binding drifted from its immutable snapshot",
                    ));
                }
                self.external_http
                    .call_bound_tool(
                        binding,
                        original_tool_name,
                        arguments,
                        invocation_id,
                        "Plugin Cloud HTTP MCP",
                    )
                    .await
            }
            _ => unreachable!("validated Plugin Cloud transport"),
        }
    }

    pub(super) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let immutable = snapshot
            .plugin_mcp_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP runtime binding is missing",
                )
            })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.allow_writes != immutable.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud route does not match its immutable runtime binding",
            ));
        }
        let stdio = snapshot
            .cloud_stdio_bindings
            .get(route.resource_id.as_str());
        let http = snapshot
            .external_http_bindings
            .get(route.resource_id.as_str());
        match (stdio, http) {
            (Some(binding), None)
                if binding.provider_ref == immutable.provider_ref
                    && binding.allow_writes == immutable.allow_writes =>
            {
                self.cloud_stdio
                    .cancel_bound_invocation(snapshot, route.resource_id.as_str(), invocation_id)
                    .await
            }
            (None, Some(binding))
                if binding.provider_ref == immutable.provider_ref
                    && binding.allow_writes == immutable.allow_writes =>
            {
                self.external_http
                    .cancel_bound_invocation(binding, invocation_id, "Plugin Cloud HTTP MCP")
                    .await
            }
            _ => Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud runtime binding is missing, ambiguous, or drifted",
            )),
        }
    }
}

enum PreparedPluginCloudRoute {
    Stdio {
        binding: Box<CloudStdioProviderBinding>,
        tools: Vec<Value>,
    },
    Http {
        binding: Box<ExternalHttpProviderBinding>,
        tools: Vec<Value>,
    },
}

fn validate_runtime_bundle(
    immutable: &PluginMcpRuntimeBinding,
    bundle: &PluginMcpCloudRuntimeBundle,
) -> Result<(), ProviderCallError> {
    let bundle_sha256 = plugin_mcp_cloud_runtime_bundle_sha256(bundle)
        .map_err(ProviderCallError::invalid_response)?;
    if bundle_sha256 != bundle.bundle_sha256
        || bundle.bundle_sha256 != immutable.component_content_sha256
        || bundle.plugin_id != immutable.plugin_id
        || bundle.release_id != immutable.release_id
        || bundle.version != immutable.version
        || bundle.artifact_sha256 != immutable.artifact_sha256
        || bundle.normalized_manifest_sha256 != immutable.normalized_manifest_sha256
        || bundle.component.component_key != immutable.component_key
        || bundle.component.execution_host != immutable.declared_execution_host
        || bundle.runtime != immutable.runtime
        || bundle.server_key.trim().is_empty()
        || bundle.resolved_runtime.component_key() != bundle.server_key
        || matches!(bundle.resolved_runtime, PluginMcpServer::ConfigFile { .. })
        || immutable
            .server_key
            .as_deref()
            .is_some_and(|server_key| server_key != bundle.server_key)
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP cloud runtime Bundle does not match the immutable Session binding",
        ));
    }
    Ok(())
}

fn validate_tool_snapshot(tools: &[Value]) -> Result<(), ProviderCallError> {
    if tools.is_empty() || tools.len() > MAX_PLUGIN_TOOLS {
        return Err(ProviderCallError::invalid_response(
            "Plugin Cloud MCP tool snapshot must contain between 1 and 200 tools",
        ));
    }
    let encoded = serde_json::to_vec(tools).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin Cloud MCP tool snapshot failed: {error}"
        ))
    })?;
    if encoded.len() > MAX_PLUGIN_TOOL_SNAPSHOT_BYTES {
        return Err(ProviderCallError::invalid_response(
            "Plugin Cloud MCP tool snapshot exceeds 512 KiB",
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
                    "Plugin Cloud MCP tool snapshot contains an unnamed tool",
                )
            })?;
        if !names.insert(name) {
            return Err(ProviderCallError::invalid_response(
                "Plugin Cloud MCP tool snapshot contains duplicate tool names",
            ));
        }
    }
    Ok(())
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Plugin Cloud Provider unavailable: {reason}");
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chatos_plugin_management_sdk::{
        plugin_mcp_cloud_runtime_bundle_sha256, PluginComponentDescriptor, PluginComponentKind,
        PluginMcpServer, PluginPathRef,
    };

    use super::*;

    fn bundle_and_binding() -> (PluginMcpCloudRuntimeBundle, PluginMcpRuntimeBinding) {
        let runtime = PluginMcpServer::Stdio {
            component_key: "search".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@example/search-mcp".to_string()],
            env: BTreeMap::new(),
            cwd: None,
        };
        let mut bundle = PluginMcpCloudRuntimeBundle {
            plugin_id: "plugin-search".to_string(),
            release_id: "release-search-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_ref: "https://plugins.example.com/search-1.0.0.zip".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component: PluginComponentDescriptor {
                component_key: "search".to_string(),
                kind: PluginComponentKind::McpServer,
                display_name: "Search".to_string(),
                execution_host: PluginExecutionHost::Cloud,
                runtime_kind: "stdio".to_string(),
                entrypoint: None,
                required: true,
                permissions: Vec::new(),
                metadata: BTreeMap::new(),
            },
            runtime: runtime.clone(),
            resolved_runtime: runtime.clone(),
            server_key: runtime.component_key().to_string(),
            bundle_sha256: String::new(),
        };
        bundle.bundle_sha256 = plugin_mcp_cloud_runtime_bundle_sha256(&bundle).unwrap();
        let binding = PluginMcpRuntimeBinding {
            provider_ref: format!("plugin-binding:{}", "c".repeat(64)),
            resource_id: "plugin_mcp_search".to_string(),
            plugin_id: bundle.plugin_id.clone(),
            release_id: bundle.release_id.clone(),
            version: bundle.version.clone(),
            artifact_sha256: bundle.artifact_sha256.clone(),
            normalized_manifest_sha256: bundle.normalized_manifest_sha256.clone(),
            component_key: bundle.component.component_key.clone(),
            component_content_sha256: bundle.bundle_sha256.clone(),
            declared_execution_host: PluginExecutionHost::Cloud,
            installation_device_id: None,
            permission_snapshot: vec!["process.spawn".to_string()],
            auth_connection_ids: Vec::new(),
            runtime,
            server_key: None,
            tool_allowlist: vec!["search".to_string()],
            tool_blocklist: Vec::new(),
            required: true,
            allow_writes: false,
        };
        (bundle, binding)
    }

    #[test]
    fn runtime_bundle_is_bound_to_release_component_and_manifest_identity() {
        let (bundle, binding) = bundle_and_binding();
        validate_runtime_bundle(&binding, &bundle).unwrap();

        let mut drifted = bundle.clone();
        drifted.version = "2.0.0".to_string();
        assert!(validate_runtime_bundle(&binding, &drifted).is_err());

        let mut forged = bundle;
        forged.bundle_sha256 = "d".repeat(64);
        assert!(validate_runtime_bundle(&binding, &forged).is_err());
    }

    #[test]
    fn config_file_bundle_freezes_one_concrete_runtime() {
        let (mut bundle, mut binding) = bundle_and_binding();
        let declared = PluginMcpServer::ConfigFile {
            component_key: "search".to_string(),
            path: PluginPathRef::new("./.mcp.json"),
        };
        let resolved = PluginMcpServer::Http {
            component_key: "remote-search".to_string(),
            url: "https://search.example.com/mcp".to_string(),
            headers: BTreeMap::new(),
            oauth_resource: None,
            connect_timeout_ms: None,
        };
        bundle.component.runtime_kind = "config_file".to_string();
        bundle.component.entrypoint = Some(PluginPathRef::new("./.mcp.json"));
        bundle.runtime = declared.clone();
        bundle.resolved_runtime = resolved;
        bundle.server_key = "remote-search".to_string();
        bundle.bundle_sha256 = plugin_mcp_cloud_runtime_bundle_sha256(&bundle).unwrap();
        binding.runtime = declared;
        binding.component_content_sha256 = bundle.bundle_sha256.clone();

        validate_runtime_bundle(&binding, &bundle).unwrap();
        bundle.server_key = "other".to_string();
        assert!(validate_runtime_bundle(&binding, &bundle).is_err());
    }
}
