// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::plugin_cloud::PluginCloudProvider;
use super::plugin_components::PluginComponentProvider;
use super::plugin_local::PluginLocalProvider;

#[derive(Clone)]
pub(super) struct PluginRouteDispatcher {
    pub(super) local: PluginLocalProvider,
    pub(super) cloud: PluginCloudProvider,
    pub(super) components: PluginComponentProvider,
}

impl PluginRouteDispatcher {
    pub(super) fn new(
        local: PluginLocalProvider,
        cloud: PluginCloudProvider,
        components: PluginComponentProvider,
    ) -> Self {
        Self {
            local,
            cloud,
            components,
        }
    }
}
