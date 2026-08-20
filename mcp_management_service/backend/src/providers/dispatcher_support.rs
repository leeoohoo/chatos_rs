// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::ProviderDispatcher;

impl ProviderDispatcher {
    pub fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => self.local_connector.supports(route),
            McpProviderKind::Embedded => self.embedded.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            McpProviderKind::PluginLocal | McpProviderKind::PluginCloud => {
                self.plugins.supports(route)
            }
            _ => false,
        }
    }

    pub fn supports_cancellation(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => self.local_connector.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            McpProviderKind::PluginLocal | McpProviderKind::PluginCloud => {
                self.plugins.supports_cancellation(route)
            }
            _ => false,
        }
    }
}
