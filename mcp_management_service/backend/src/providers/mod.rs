// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod cancel_response;
mod chatos;
mod cloud_sandbox;
mod cloud_stdio;
mod dispatcher_prepare;
mod dispatcher_runtime;
mod embedded;
mod external_http;
mod local_connector;
mod local_sandbox;
mod plugin_cloud;
mod plugin_components;
mod plugin_local;
mod plugin_routes;
mod plugin_routes_prepare;
mod plugin_routes_runtime;
mod project_service;
mod sandbox_images;
mod task_runner;

use std::time::Duration;

pub(super) use cancel_response::decode_cancel_notification_response;
pub(crate) use chatos::memory_provider_ref as chatos_memory_provider_ref;
use chatos::ChatosProvider;
use cloud_sandbox::CloudSandboxProvider;
use cloud_stdio::CloudStdioProvider;
use embedded::EmbeddedProvider;
use external_http::ExternalHttpProvider;
pub(crate) use external_http::{
    build_pinned_external_http_client,
    header_is_managed_or_unsafe as external_http_header_is_managed_or_unsafe,
};
use local_connector::LocalConnectorProvider;
use local_sandbox::LocalSandboxProvider;
use plugin_cloud::PluginCloudProvider;
use plugin_components::PluginComponentProvider;
use plugin_local::PluginLocalProvider;
use plugin_routes::PluginRouteDispatcher;
use project_service::ProjectServiceProvider;
pub use project_service::{ProviderCallError, ProviderCallOutcome};
use sandbox_images::SandboxImagesProvider;
pub(crate) use sandbox_images::{
    cloud_provider_ref as sandbox_images_cloud_provider_ref,
    local_provider_ref as sandbox_images_local_provider_ref,
};
use task_runner::TaskRunnerProvider;

pub struct TaskRunnerProviderConfig {
    pub http: reqwest::Client,
    pub base_url: String,
    pub internal_secret: Option<String>,
    pub request_timeout: Duration,
    pub ask_user_request_timeout: Duration,
}

pub struct ChatosProviderConfig {
    pub http: reqwest::Client,
    pub base_url: String,
    pub internal_secret: Option<String>,
    pub request_timeout: Duration,
    pub ask_user_request_timeout: Duration,
    pub browser_request_timeout: Duration,
}

pub struct ProviderRuntimeConfig {
    pub downstream_request_timeout: Duration,
    pub external_http_request_timeout: Duration,
    pub sandbox_image_request_timeout: Duration,
    pub response_limit_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCancelOutcome {
    Cancelled,
    CancelRequested,
    NotSupported,
}

#[derive(Clone)]
pub struct ProviderDispatcher {
    local_connector: LocalConnectorProvider,
    local_sandbox: LocalSandboxProvider,
    plugins: PluginRouteDispatcher,
    project_service: ProjectServiceProvider,
    task_runner: TaskRunnerProvider,
    chatos: ChatosProvider,
    cloud_sandbox: CloudSandboxProvider,
    cloud_stdio: CloudStdioProvider,
    sandbox_images: SandboxImagesProvider,
    embedded: EmbeddedProvider,
    external_http: ExternalHttpProvider,
}

impl ProviderDispatcher {
    pub fn new(
        project_service_http: reqwest::Client,
        project_service_base_url: impl Into<String>,
        project_service_internal_secret: Option<String>,
        task_runner: TaskRunnerProviderConfig,
        chatos: ChatosProviderConfig,
        local_connector_http: reqwest::Client,
        local_connector_service_base_url: impl Into<String>,
        local_connector_internal_secret: Option<String>,
        sandbox_manager_http: reqwest::Client,
        sandbox_manager_service_base_url: impl Into<String>,
        sandbox_manager_internal_secret: Option<String>,
        sandbox_manager_request_timeout: Duration,
        embedded_work_dir: std::path::PathBuf,
        runtime: ProviderRuntimeConfig,
    ) -> Result<Self, String> {
        let local_connector_service_base_url = local_connector_service_base_url.into();
        let sandbox_manager_service_base_url = sandbox_manager_service_base_url.into();
        let cloud_stdio = CloudStdioProvider::new(
            sandbox_manager_http.clone(),
            sandbox_manager_service_base_url.clone(),
            sandbox_manager_request_timeout,
            sandbox_manager_internal_secret.clone(),
            runtime.response_limit_bytes,
        )?;
        let external_http = ExternalHttpProvider::new(
            runtime.external_http_request_timeout,
            runtime.response_limit_bytes,
        );
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
        let plugin_cloud = PluginCloudProvider::new(cloud_stdio.clone(), external_http.clone());
        Ok(Self {
            local_connector: LocalConnectorProvider::new(
                local_connector_http.clone(),
                local_connector_service_base_url.clone(),
                runtime.downstream_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            local_sandbox: LocalSandboxProvider::new(
                local_connector_http.clone(),
                local_connector_service_base_url.clone(),
                runtime.downstream_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            plugins: PluginRouteDispatcher::new(plugin_local, plugin_cloud, plugin_components),
            project_service: ProjectServiceProvider::new(
                project_service_http,
                project_service_base_url,
                project_service_internal_secret,
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
            cloud_sandbox: CloudSandboxProvider::new(
                sandbox_manager_http.clone(),
                sandbox_manager_service_base_url.clone(),
                sandbox_manager_request_timeout,
                sandbox_manager_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            cloud_stdio,
            sandbox_images: SandboxImagesProvider::new(
                sandbox_manager_http,
                sandbox_manager_service_base_url,
                sandbox_manager_internal_secret,
                local_connector_http,
                local_connector_service_base_url,
                local_connector_internal_secret,
                sandbox_manager_request_timeout,
                runtime.sandbox_image_request_timeout,
                runtime.response_limit_bytes,
            )?,
            embedded: EmbeddedProvider::new(embedded_work_dir, runtime.response_limit_bytes)?,
            external_http,
        })
    }
}
