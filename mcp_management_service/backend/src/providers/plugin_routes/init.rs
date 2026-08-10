// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::PluginRouteDispatcher;

impl PluginRouteDispatcher {
    pub(in crate::providers) fn new(
        local: super::PluginLocalProvider,
        cloud: super::PluginCloudProvider,
        components: super::PluginComponentProvider,
    ) -> Self {
        Self {
            local,
            cloud,
            components,
        }
    }
}
