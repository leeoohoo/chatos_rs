// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod chatos;
mod cloud_sandbox;
mod cloud_stdio;
mod embedded;
mod external_http;
mod local_connector;
mod project_service;
mod sandbox_images;
mod task_runner;

use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

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
use project_service::ProjectServiceProvider;
pub use project_service::{ProviderCallError, ProviderCallOutcome};
use sandbox_images::SandboxImagesProvider;
pub(crate) use sandbox_images::{
    cloud_provider_ref as sandbox_images_cloud_provider_ref,
    local_provider_ref as sandbox_images_local_provider_ref,
};
use task_runner::TaskRunnerProvider;

pub struct TaskRunnerProviderConfig {
    pub base_url: String,
    pub internal_secret: Option<String>,
    pub request_timeout: Duration,
    pub ask_user_request_timeout: Duration,
}

pub struct ChatosProviderConfig {
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

#[derive(Clone)]
pub struct ProviderDispatcher {
    local_connector: LocalConnectorProvider,
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
        project_service_base_url: impl Into<String>,
        project_service_internal_secret: Option<String>,
        task_runner: TaskRunnerProviderConfig,
        chatos: ChatosProviderConfig,
        local_connector_service_base_url: impl Into<String>,
        local_connector_internal_secret: Option<String>,
        sandbox_manager_service_base_url: impl Into<String>,
        sandbox_manager_internal_secret: Option<String>,
        sandbox_manager_request_timeout: Duration,
        embedded_work_dir: std::path::PathBuf,
        runtime: ProviderRuntimeConfig,
    ) -> Result<Self, String> {
        let local_connector_service_base_url = local_connector_service_base_url.into();
        let sandbox_manager_service_base_url = sandbox_manager_service_base_url.into();
        Ok(Self {
            local_connector: LocalConnectorProvider::new(
                local_connector_service_base_url.clone(),
                runtime.downstream_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            project_service: ProjectServiceProvider::new(
                project_service_base_url,
                runtime.downstream_request_timeout,
                project_service_internal_secret,
                runtime.response_limit_bytes,
            )?,
            task_runner: TaskRunnerProvider::new(
                task_runner.base_url,
                task_runner.request_timeout,
                task_runner.ask_user_request_timeout,
                task_runner.internal_secret,
                runtime.response_limit_bytes,
            )?,
            chatos: ChatosProvider::new(
                chatos.base_url,
                chatos.request_timeout,
                chatos.ask_user_request_timeout,
                chatos.browser_request_timeout,
                chatos.internal_secret,
                runtime.response_limit_bytes,
            )?,
            cloud_sandbox: CloudSandboxProvider::new(
                sandbox_manager_service_base_url.clone(),
                sandbox_manager_request_timeout,
                sandbox_manager_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            cloud_stdio: CloudStdioProvider::new(
                sandbox_manager_service_base_url.clone(),
                sandbox_manager_request_timeout,
                sandbox_manager_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            sandbox_images: SandboxImagesProvider::new(
                sandbox_manager_service_base_url,
                sandbox_manager_internal_secret,
                local_connector_service_base_url,
                local_connector_internal_secret,
                sandbox_manager_request_timeout,
                runtime.sandbox_image_request_timeout,
                runtime.response_limit_bytes,
            )?,
            embedded: EmbeddedProvider::new(embedded_work_dir, runtime.response_limit_bytes)?,
            external_http: ExternalHttpProvider::new(
                runtime.external_http_request_timeout,
                runtime.response_limit_bytes,
            ),
        })
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

    pub fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => {
                self.local_connector.supports(route) || self.sandbox_images.supports(route)
            }
            McpProviderKind::CloudSandbox => {
                self.cloud_sandbox.supports(route) || self.sandbox_images.supports(route)
            }
            McpProviderKind::CloudStdio => self.cloud_stdio.supports(route),
            McpProviderKind::Embedded => self.embedded.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            _ => false,
        }
    }

    pub fn requires_sandbox_target(&self, route: &ResolvedMcpRoute) -> bool {
        self.cloud_sandbox.supports(route) || self.cloud_stdio.supports(route)
    }

    pub async fn validate_sandbox_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        self.cloud_sandbox
            .validate_target(target, owner_user_id, project_id, run_id)
            .await
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
            McpProviderKind::Unavailable => Err(ProviderCallError::provider_unavailable(
                route.reason.clone(),
            )),
            _ => Err(ProviderCallError::provider_unavailable(format!(
                "provider adapter is not registered for {}",
                route.provider_kind.as_str()
            ))),
        }
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
    }
}
