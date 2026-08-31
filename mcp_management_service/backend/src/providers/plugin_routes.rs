// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::plugin_components::PluginComponentProvider;
use super::plugin_local::PluginLocalProvider;

mod init;

#[derive(Clone)]
pub(super) struct PluginRouteDispatcher {
    pub(super) local: PluginLocalProvider,
    pub(super) components: PluginComponentProvider,
}
