// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use super::{
    ChatosProvider, ChatosProviderConfig, EmbeddedProvider, LocalConnectorProvider,
    PluginComponentProvider, PluginLocalProvider, PluginRouteDispatcher, ProjectServiceProvider,
    ProviderDispatcher, ProviderRuntimeConfig, TaskRunnerProvider, TaskRunnerProviderConfig,
};

impl ProviderDispatcher {
    pub fn new(
        project_service_http: reqwest::Client,
        project_service_base_url: impl Into<String>,
        project_service_internal_secret: Option<String>,
        project_service_tool_timeout: Duration,
        task_runner: TaskRunnerProviderConfig,
        chatos: ChatosProviderConfig,
        local_connector_http: reqwest::Client,
        local_connector_service_base_url: impl Into<String>,
        local_connector_internal_secret: Option<String>,
        embedded_work_dir: std::path::PathBuf,
        runtime: ProviderRuntimeConfig,
    ) -> Result<Self, String> {
        let local_connector_service_base_url = local_connector_service_base_url.into();
        let plugin_local = PluginLocalProvider::new(
            local_connector_http.clone(),
            local_connector_service_base_url.clone(),
            runtime.downstream_request_timeout,
            local_connector_internal_secret.clone(),
            runtime.response_limit_bytes,
        )?;
        let plugin_components = PluginComponentProvider::new(
            local_connector_http.clone(),
            local_connector_service_base_url.clone(),
            runtime.downstream_request_timeout,
            local_connector_internal_secret.clone(),
            runtime.response_limit_bytes,
        )?;
        Ok(Self {
            local_connector: LocalConnectorProvider::new(
                local_connector_http.clone(),
                local_connector_service_base_url.clone(),
                runtime.local_connector_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            plugins: PluginRouteDispatcher::new(plugin_local, plugin_components),
            project_service: ProjectServiceProvider::new(
                project_service_http,
                project_service_base_url,
                project_service_internal_secret,
                project_service_tool_timeout,
                runtime.response_limit_bytes,
            )?,
            task_runner: TaskRunnerProvider::new(
                task_runner.http,
                task_runner.base_url,
                task_runner.request_timeout,
                task_runner.ask_user_request_timeout,
                task_runner.internal_secret,
                runtime.response_limit_bytes,
            )?,
            chatos: ChatosProvider::new(
                chatos.http,
                chatos.base_url,
                chatos.request_timeout,
                chatos.ask_user_request_timeout,
                chatos.browser_request_timeout,
                chatos.internal_secret,
                runtime.response_limit_bytes,
            )?,
            embedded: EmbeddedProvider::new(embedded_work_dir, runtime.response_limit_bytes)?,
        })
    }
}
