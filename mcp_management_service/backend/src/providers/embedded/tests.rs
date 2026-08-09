// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, McpRetryClass, ResolvedMcpRoute};

use super::*;

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
