// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use crate::runtime::{CloudStdioProviderBinding, ExternalHttpProviderBinding};

use super::cloud_stdio::CloudStdioProvider;
use super::external_http::ExternalHttpProvider;
use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[path = "plugin_cloud/cloud_prepare.rs"]
mod cloud_prepare;
#[path = "plugin_cloud/cloud_runtime.rs"]
mod cloud_runtime;
#[path = "plugin_cloud/prepare.rs"]
mod prepare;
#[path = "plugin_cloud/validation.rs"]
mod validation;

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chatos_plugin_management_sdk::{
        plugin_mcp_cloud_runtime_bundle_sha256, PluginComponentDescriptor, PluginComponentKind,
        PluginExecutionHost, PluginMcpCloudRuntimeBundle, PluginMcpServer, PluginPathRef,
    };

    use crate::runtime::PluginMcpRuntimeBinding;

    use super::validation::validate_runtime_bundle;

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
