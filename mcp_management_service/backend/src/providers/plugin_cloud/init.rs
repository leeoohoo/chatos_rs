// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{McpProviderKind, PluginCloudProvider, ResolvedMcpRoute};

impl PluginCloudProvider {
    pub(in crate::providers) fn new(external_http: super::ExternalHttpProvider) -> Self {
        Self { external_http }
    }

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        route.provider_kind == McpProviderKind::PluginCloud
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|value| value.starts_with("plugin-binding:"))
    }
}
