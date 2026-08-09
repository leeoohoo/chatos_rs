// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use chatos_mcp::{
    system_mcp_descriptor_by_resource_id, SystemMcpKey, WebToolsOptions, WebToolsService,
};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::EmbeddedProvider;

impl EmbeddedProvider {
    pub(in crate::providers) fn new(
        work_dir: PathBuf,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
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

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        route.provider_kind == McpProviderKind::Embedded
            && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                .is_some_and(|descriptor| descriptor.key == SystemMcpKey::WebTools)
    }
}
