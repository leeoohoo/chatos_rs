// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod batch_schedule;
mod runtime_settings;
mod tasks;
mod validation;

impl TaskService {
    pub(crate) fn config(&self) -> &AppConfig {
        &self.config
    }

    pub(crate) fn new(config: AppConfig, store: AppStore) -> Self {
        Self {
            config,
            store,
            plugin_management_client: None,
        }
    }

    pub(crate) fn new_with_plugin_management(
        config: AppConfig,
        store: AppStore,
        plugin_management_client: PluginManagementClient,
    ) -> Self {
        Self {
            config,
            store,
            plugin_management_client: Some(plugin_management_client),
        }
    }

    pub fn resolve_task_mcp(&self, task: &TaskRecord) -> TaskMcpResolutionResponse {
        task_mcp_resolution_response(task)
    }
}
