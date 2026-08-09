// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod cancel_response;
mod chatos;
mod cloud_sandbox;
mod cloud_stdio;
mod dispatcher_init;
mod dispatcher_prepare;
mod dispatcher_prepare_plugins;
mod dispatcher_prepare_sandbox;
mod dispatcher_prepare_system;
mod dispatcher_runtime;
mod dispatcher_runtime_lifecycle;
mod dispatcher_runtime_support;
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
