// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod cancel_response;
mod chatos;
mod cloud_sandbox;
mod cloud_stdio;
mod embedded;
mod external_http;
mod local_connector;
mod local_sandbox;
mod plugin_cloud;
mod plugin_components;
mod plugin_local;
mod project_service;
mod sandbox_images;
mod task_runner;

use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

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
    plugin_cloud: PluginCloudProvider,
    plugin_components: PluginComponentProvider,
    plugin_local: PluginLocalProvider,
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
            plugin_local: PluginLocalProvider::new(
                local_connector_http.clone(),
                local_connector_service_base_url.clone(),
                runtime.downstream_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            plugin_components: PluginComponentProvider::new(
                local_connector_http.clone(),
                local_connector_service_base_url.clone(),
                runtime.downstream_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            plugin_cloud: PluginCloudProvider::new(cloud_stdio.clone(), external_http.clone()),
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

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_plugin_tool_component_routes(
        &self,
        plugin_management: &chatos_plugin_management_sdk::PluginManagementClient,
        immutable_bindings: &std::collections::HashMap<
            String,
            crate::runtime::PluginToolComponentRuntimeBinding,
        >,
        routes: &mut [ResolvedMcpRoute],
        context: &chatos_mcp_management_sdk::ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        std::collections::HashMap<String, crate::runtime::PluginLocalToolComponentBinding>,
        std::collections::HashMap<String, crate::runtime::PluginCloudToolComponentBinding>,
        std::collections::HashMap<String, Vec<Value>>,
    ) {
        self.plugin_components
            .prepare_routes(
                plugin_management,
                immutable_bindings,
                routes,
                context,
                runtime_session_id,
                owner_user_id,
                expires_at_unix,
            )
            .await
    }

    pub async fn close_prepared_plugin_tool_component_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &std::collections::HashMap<
            String,
            crate::runtime::PluginLocalToolComponentBinding,
        >,
    ) {
        self.plugin_components
            .close_local_bindings(owner_user_id, runtime_session_id, bindings)
            .await;
    }

    pub async fn prepare_external_http_routes(
        &self,
        capabilities: &chatos_plugin_management_sdk::ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
    ) -> std::collections::HashMap<String, crate::runtime::ExternalHttpProviderBinding> {
        self.external_http
            .prepare_routes(capabilities, routes)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_plugin_local_routes(
        &self,
        immutable_bindings: &std::collections::HashMap<
            String,
            crate::runtime::PluginMcpRuntimeBinding,
        >,
        routes: &mut [ResolvedMcpRoute],
        context: &chatos_mcp_management_sdk::ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        std::collections::HashMap<String, crate::runtime::PluginLocalProviderBinding>,
        std::collections::HashMap<String, Vec<Value>>,
    ) {
        self.plugin_local
            .prepare_routes(
                immutable_bindings,
                routes,
                context,
                runtime_session_id,
                owner_user_id,
                expires_at_unix,
            )
            .await
    }

    pub async fn close_prepared_plugin_local_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &std::collections::HashMap<String, crate::runtime::PluginLocalProviderBinding>,
    ) {
        self.plugin_local
            .close_bindings(owner_user_id, runtime_session_id, bindings)
            .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_plugin_cloud_routes(
        &self,
        plugin_management: &chatos_plugin_management_sdk::PluginManagementClient,
        immutable_bindings: &std::collections::HashMap<
            String,
            crate::runtime::PluginMcpRuntimeBinding,
        >,
        routes: &mut [ResolvedMcpRoute],
        context: &chatos_mcp_management_sdk::ProjectExecutionContext,
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        std::collections::HashMap<String, crate::runtime::CloudStdioProviderBinding>,
        std::collections::HashMap<String, crate::runtime::ExternalHttpProviderBinding>,
        std::collections::HashMap<String, Vec<Value>>,
    ) {
        self.plugin_cloud
            .prepare_routes(
                plugin_management,
                immutable_bindings,
                routes,
                context,
                target,
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_chatos_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: &str,
        source_session_id: Option<&str>,
        expires_at_unix: i64,
    ) -> std::collections::HashMap<String, Vec<Value>> {
        self.chatos
            .prepare_routes(
                routes,
                runtime_session_id,
                owner_user_id,
                agent_key,
                project_id,
                source_session_id,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_task_runner_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: &str,
        run_id: Option<&str>,
        turn_id: Option<&str>,
        task_id: Option<&str>,
        source_session_id: Option<&str>,
        source_user_message_id: Option<&str>,
        default_model_config_id: Option<&str>,
        task_profile: Option<&str>,
        expected_project_task_ids: &[String],
        expires_at_unix: i64,
    ) -> std::collections::HashMap<String, Vec<Value>> {
        self.task_runner
            .prepare_routes(
                routes,
                runtime_session_id,
                owner_user_id,
                agent_key,
                project_id,
                run_id,
                turn_id,
                task_id,
                source_session_id,
                source_user_message_id,
                default_model_config_id,
                task_profile,
                expected_project_task_ids,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_cloud_stdio_routes(
        &self,
        capabilities: &chatos_plugin_management_sdk::ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        std::collections::HashMap<String, crate::runtime::CloudStdioProviderBinding>,
        std::collections::HashMap<String, Vec<Value>>,
    ) {
        self.cloud_stdio
            .prepare_routes(
                capabilities,
                routes,
                target,
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn close_prepared_cloud_stdio_bindings(
        &self,
        target: &SandboxExecutionTarget,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
        bindings: &std::collections::HashMap<String, crate::runtime::CloudStdioProviderBinding>,
    ) {
        self.cloud_stdio
            .close_bindings(
                target,
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
                bindings,
            )
            .await;
    }

    pub fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => {
                self.local_connector.supports(route)
                    || self.local_sandbox.supports(route)
                    || self.sandbox_images.supports(route)
            }
            McpProviderKind::CloudSandbox => {
                self.cloud_sandbox.supports(route) || self.sandbox_images.supports(route)
            }
            McpProviderKind::CloudStdio => self.cloud_stdio.supports(route),
            McpProviderKind::Embedded => self.embedded.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            McpProviderKind::PluginLocal => {
                self.plugin_local.supports(route) || self.plugin_components.supports(route)
            }
            McpProviderKind::PluginCloud => {
                self.plugin_cloud.supports(route) || self.plugin_components.supports(route)
            }
            _ => false,
        }
    }

    pub fn requires_sandbox_target(&self, route: &ResolvedMcpRoute) -> bool {
        self.cloud_sandbox.supports(route)
            || self.local_sandbox.supports(route)
            || self.cloud_stdio.supports(route)
    }

    pub fn supports_cancellation(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => {
                self.local_connector.supports(route) || self.local_sandbox.supports(route)
            }
            McpProviderKind::CloudSandbox => self.cloud_sandbox.supports(route),
            McpProviderKind::CloudStdio => self.cloud_stdio.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            McpProviderKind::PluginLocal => self.plugin_local.supports(route),
            McpProviderKind::PluginCloud => self.plugin_cloud.supports(route),
            _ => false,
        }
    }

    pub async fn validate_sandbox_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        match target.provider {
            chatos_mcp_management_sdk::SandboxProviderKind::Cloud => {
                self.cloud_sandbox
                    .validate_target(target, owner_user_id, project_id, run_id)
                    .await
            }
            chatos_mcp_management_sdk::SandboxProviderKind::LocalConnector => {
                self.local_sandbox
                    .validate_target(target, owner_user_id, project_id, run_id)
                    .await
            }
            chatos_mcp_management_sdk::SandboxProviderKind::None => Err(
                ProviderCallError::provider_unavailable("sandbox target provider is not resolved"),
            ),
        }
    }

    pub async fn resolve_local_sandbox_pairing(
        &self,
        context: &chatos_mcp_management_sdk::ProjectExecutionContext,
    ) -> Result<Option<String>, ProviderCallError> {
        self.local_sandbox.resolve_active_pairing(context).await
    }

    pub async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness
                if self.project_service.supports(route) =>
            {
                self.project_service
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::InternalService if self.task_runner.supports(route) => {
                self.task_runner
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::InternalService if self.chatos.supports(route) => {
                self.chatos
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::LocalConnector | McpProviderKind::CloudSandbox
                if self.sandbox_images.supports(route) =>
            {
                self.sandbox_images
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::LocalConnector if self.local_connector.supports(route) => {
                self.local_connector
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::LocalConnector if self.local_sandbox.supports(route) => {
                self.local_sandbox
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::CloudSandbox if self.cloud_sandbox.supports(route) => {
                self.cloud_sandbox
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::CloudStdio if self.cloud_stdio.supports(route) => {
                self.cloud_stdio
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::Embedded if self.embedded.supports(route) => {
                self.embedded
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::ExternalHttp if self.external_http.supports(route) => {
                self.external_http
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::PluginLocal if self.plugin_local.supports(route) => {
                self.plugin_local
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::PluginLocal if self.plugin_components.supports(route) => {
                self.plugin_components
                    .call_tool(snapshot, route, original_tool_name, arguments)
                    .await
            }
            McpProviderKind::PluginCloud if self.plugin_cloud.supports(route) => {
                self.plugin_cloud
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::PluginCloud if self.plugin_components.supports(route) => {
                self.plugin_components
                    .call_tool(snapshot, route, original_tool_name, arguments)
                    .await
            }
            McpProviderKind::Unavailable => Err(ProviderCallError::provider_unavailable(
                route.reason.clone(),
            )),
            _ => Err(ProviderCallError::provider_unavailable(format!(
                "provider adapter is not registered for {}",
                route.provider_kind.as_str()
            ))),
        }
    }

    pub async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        if !route.cancel_supported {
            return Ok(ProviderCancelOutcome::NotSupported);
        }
        let cancellation = async {
            match route.provider_kind {
                McpProviderKind::InternalService | McpProviderKind::Harness
                    if self.project_service.supports(route) =>
                {
                    self.project_service
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::InternalService if self.task_runner.supports(route) => {
                    self.task_runner
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::InternalService if self.chatos.supports(route) => {
                    self.chatos
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::LocalConnector if self.local_connector.supports(route) => {
                    self.local_connector
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::LocalConnector if self.local_sandbox.supports(route) => {
                    self.local_sandbox
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::CloudSandbox if self.cloud_sandbox.supports(route) => {
                    self.cloud_sandbox
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::CloudStdio if self.cloud_stdio.supports(route) => {
                    self.cloud_stdio
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::ExternalHttp if self.external_http.supports(route) => {
                    self.external_http
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::PluginLocal if self.plugin_local.supports(route) => {
                    self.plugin_local
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::PluginCloud if self.plugin_cloud.supports(route) => {
                    self.plugin_cloud
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                _ => Ok(ProviderCancelOutcome::NotSupported),
            }
        };
        tokio::time::timeout(Duration::from_secs(5), cancellation)
            .await
            .map_err(|_| {
                ProviderCallError::provider_unavailable("Provider cancellation request timed out")
            })?
    }

    pub async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        if let Err(error) = self.chatos.close_session(snapshot).await {
            tracing::warn!(
                session_id = snapshot.session_id.as_str(),
                error_code = error.code,
                "failed to close ChatOS MCP Provider session state"
            );
        }
        self.cloud_stdio.close_session(snapshot).await;
        self.plugin_local.close_session(snapshot).await;
        self.plugin_components.close_session(snapshot).await;
    }
}
