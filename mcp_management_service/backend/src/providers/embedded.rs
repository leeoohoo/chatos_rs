// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use chatos_mcp::{
    system_mcp_descriptor_by_resource_id, SystemMcpKey, WebToolsOptions, WebToolsService,
};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use super::{ProviderCallError, ProviderCallOutcome};

#[derive(Clone)]
pub(super) struct EmbeddedProvider {
    web_tools: WebToolsService,
    response_limit_bytes: usize,
}

impl EmbeddedProvider {
    pub(super) fn new(work_dir: PathBuf, response_limit_bytes: usize) -> Result<Self, String> {
        let web_tools = WebToolsService::new(WebToolsOptions {
            server_name: "web_tools".to_string(),
            workspace_dir: work_dir.join("web_tools"),
            ..WebToolsOptions::default()
        })?;
        Ok(Self {
            web_tools,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        route.provider_kind == McpProviderKind::Embedded
            && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                .is_some_and(|descriptor| descriptor.key == SystemMcpKey::WebTools)
    }

    pub(super) async fn call_tool(
        &self,
        _snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        _invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "embedded Provider does not support this route",
            ));
        }
        let result = self
            .web_tools
            .call_tool(original_tool_name, arguments)
            .map_err(ProviderCallError::provider_unavailable)?;
        let response_bytes = serde_json::to_vec(&result)
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "serialize embedded Provider result failed: {err}"
                ))
            })?
            .len();
        if response_bytes > self.response_limit_bytes {
            return Err(ProviderCallError::invalid_response(format!(
                "embedded Provider result exceeds {} bytes",
                self.response_limit_bytes
            )));
        }
        Ok(ProviderCallOutcome {
            result,
            response_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_management_sdk::McpRetryClass;

    fn route(resource_id: &str) -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: resource_id.to_string(),
            server_name: "web_tools".to_string(),
            provider_kind: McpProviderKind::Embedded,
            provider_ref: Some("mcp-management-service".to_string()),
            tool_namespace: "web_tools".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn embedded_provider_supports_only_the_stateless_web_tools_route() {
        let provider = EmbeddedProvider::new(
            std::env::temp_dir().join("chatos-mcp-management-embedded-test"),
            1024 * 1024,
        )
        .unwrap();
        assert!(provider.supports(&route("builtin_web_tools")));
        assert!(!provider.supports(&route("builtin_notepad")));
        assert!(!provider.supports(&route("builtin_browser_tools")));
    }
}
