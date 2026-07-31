// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::project_context::ProjectContextClient;
use crate::providers::{
    ChatosProviderConfig, ProviderDispatcher, ProviderRuntimeConfig, TaskRunnerProviderConfig,
};
use crate::routing::RoutingEngine;
use crate::runtime::{RuntimeGrantService, RuntimeSessionStore};
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub routing: RoutingEngine,
    pub plugin_management_client: PluginManagementClient,
    pub project_context_client: ProjectContextClient,
    pub providers: ProviderDispatcher,
    pub runtime_grants: RuntimeGrantService,
    pub runtime_sessions: RuntimeSessionStore,
}

impl AppState {
    pub fn new(config: AppConfig) -> Result<Self, String> {
        let plugin_management_client = PluginManagementClient::new(PluginManagementClientConfig {
            base_url: config.plugin_management_service_base_url.clone(),
            request_timeout: config.downstream_request_timeout,
            internal_api_secret: config.plugin_management_internal_api_secret.clone(),
            caller_service: "mcp-management-service".to_string(),
        })
        .map_err(|err| format!("initialize Plugin Management client failed: {err}"))?;
        let project_context_client = ProjectContextClient::new(
            config.project_service_base_url.clone(),
            config.downstream_request_timeout,
            config.project_service_internal_api_secret.clone(),
        )?;
        let providers = ProviderDispatcher::new(
            config.project_service_base_url.clone(),
            config.project_service_internal_api_secret.clone(),
            TaskRunnerProviderConfig {
                base_url: config.task_runner_service_base_url.clone(),
                internal_secret: config.task_runner_internal_api_secret.clone(),
                request_timeout: config.task_runner_request_timeout,
                ask_user_request_timeout: config.task_runner_ask_user_request_timeout,
            },
            ChatosProviderConfig {
                base_url: config.chatos_service_base_url.clone(),
                internal_secret: config.chatos_internal_api_secret.clone(),
                request_timeout: config.downstream_request_timeout,
                ask_user_request_timeout: config.chatos_ask_user_request_timeout,
            },
            config.local_connector_service_base_url.clone(),
            config.local_connector_internal_api_secret.clone(),
            config.sandbox_manager_service_base_url.clone(),
            config.sandbox_manager_internal_api_secret.clone(),
            config.sandbox_manager_request_timeout,
            config.embedded_work_dir.clone(),
            ProviderRuntimeConfig {
                downstream_request_timeout: config.downstream_request_timeout,
                external_http_request_timeout: config.external_http_request_timeout,
                sandbox_image_request_timeout: config.sandbox_image_request_timeout,
                response_limit_bytes: config.provider_response_limit_bytes,
            },
        )?;
        Ok(Self {
            runtime_grants: RuntimeGrantService::new(
                config.runtime_grant_secret.clone(),
                config.runtime_session_ttl,
            ),
            config,
            routing: RoutingEngine,
            plugin_management_client,
            project_context_client,
            providers,
            runtime_sessions: RuntimeSessionStore::default(),
        })
    }
}
