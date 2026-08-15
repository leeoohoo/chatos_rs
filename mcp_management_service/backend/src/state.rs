// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::async_dispatch::AsyncToolDispatch;
use crate::config::AppConfig;
use crate::project_context::ProjectContextClient;
use crate::providers::{
    ChatosProviderConfig, ProviderDispatcher, ProviderRuntimeConfig, TaskRunnerProviderConfig,
};
use crate::routing::RoutingEngine;
use crate::runtime::{
    RuntimeExecutionScopeStore, RuntimeGrantService, RuntimeInvocationQuota,
    RuntimeInvocationQuotaLimits, RuntimeInvocationStore, RuntimeSessionCacheLimits,
    RuntimeSessionCloseStore, RuntimeSessionStore, RuntimeToolBatchStore,
};
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};
#[cfg(not(test))]
const RUNTIME_SESSION_CACHE_MAX_ENTRIES_CONFIG_KEY: &str =
    "mcp_management.runtime.session_cache_max_entries";
#[cfg(not(test))]
const RUNTIME_SESSION_CACHE_MAX_BYTES_CONFIG_KEY: &str =
    "mcp_management.runtime.session_cache_max_bytes";
#[cfg(not(test))]
const INVOCATION_QUOTA_VALKEY_URL_CONFIG_KEY: &str = "mcp_management.invocation.quota_valkey_url";
#[cfg(not(test))]
const INVOCATION_QUOTA_KEY_PREFIX_CONFIG_KEY: &str = "mcp_management.invocation.quota_key_prefix";
#[cfg(not(test))]
const INVOCATION_TENANT_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.tenant_active_limit";
#[cfg(not(test))]
const INVOCATION_USER_ACTIVE_LIMIT_CONFIG_KEY: &str = "mcp_management.invocation.user_active_limit";
#[cfg(not(test))]
const INVOCATION_PROJECT_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.project_active_limit";
#[cfg(not(test))]
const INVOCATION_DEVICE_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.device_active_limit";

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub routing: RoutingEngine,
    pub plugin_management_client: PluginManagementClient,
    pub project_context_client: ProjectContextClient,
    pub providers: ProviderDispatcher,
    pub runtime_grants: RuntimeGrantService,
    pub runtime_sessions: RuntimeSessionStore,
    pub runtime_session_closes: RuntimeSessionCloseStore,
    pub runtime_execution_scopes: RuntimeExecutionScopeStore,
    pub runtime_invocations: RuntimeInvocationStore,
    pub runtime_tool_batches: RuntimeToolBatchStore,
    pub async_tool_dispatch: AsyncToolDispatch,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let (runtime_session_cache_limits, runtime_invocation_quota) =
            load_runtime_managed_resources().await?;
        let plugin_management_client = PluginManagementClient::new(
            PluginManagementClientConfig::new(
                config.plugin_management_service_base_url.clone(),
                config.plugin_management_service_base_url.clone(),
                config.downstream_request_timeout,
                config.plugin_management_internal_api_secret.clone(),
                "mcp-management-service",
                config.plugin_management_http_client.clone(),
            )
            .map_err(|err| format!("build plugin management client config failed: {err}"))?,
        )
        .map_err(|err| format!("initialize Plugin Management client failed: {err}"))?;
        let project_context_client = ProjectContextClient::new(
            config.project_service_http_client.clone(),
            config.project_service_base_url.clone(),
            config.project_service_internal_api_secret.clone(),
        )?;
        let providers = ProviderDispatcher::new(
            config.project_service_http_client.clone(),
            config.project_service_base_url.clone(),
            config.project_service_internal_api_secret.clone(),
            config.project_service_tool_timeout,
            TaskRunnerProviderConfig {
                http: task_runner_http_client(&config)?,
                base_url: config.task_runner_service_base_url.clone(),
                internal_secret: config.task_runner_internal_api_secret.clone(),
                request_timeout: config.task_runner_request_timeout,
                ask_user_request_timeout: config.task_runner_ask_user_request_timeout,
            },
            ChatosProviderConfig {
                http: config.chatos_http_client.clone(),
                base_url: config.chatos_service_base_url.clone(),
                internal_secret: config.chatos_internal_api_secret.clone(),
                request_timeout: config.downstream_request_timeout,
                ask_user_request_timeout: config.chatos_ask_user_request_timeout,
                browser_request_timeout: config.chatos_browser_request_timeout,
            },
            config.local_connector_http_client.clone(),
            config.local_connector_service_base_url.clone(),
            config.local_connector_internal_api_secret.clone(),
            config.sandbox_manager_http_client.clone(),
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
        let runtime_sessions = match config.runtime_session_database_url.as_deref() {
            Some(database_url) => {
                RuntimeSessionStore::connect(
                    database_url,
                    config.runtime_session_encryption_secret.as_str(),
                    config.external_http_request_timeout,
                    runtime_session_cache_limits,
                )
                .await?
            }
            None => RuntimeSessionStore::memory(),
        };
        let runtime_invocations = match config.runtime_session_database_url.as_deref() {
            Some(database_url) => {
                RuntimeInvocationStore::connect(database_url, runtime_invocation_quota).await?
            }
            None => {
                #[cfg(test)]
                {
                    RuntimeInvocationStore::memory()
                }
                #[cfg(not(test))]
                {
                    return Err(
                        "MCP Management Runtime Invocation database is required".to_string()
                    );
                }
            }
        };
        let runtime_session_closes = match config.runtime_session_database_url.as_deref() {
            Some(database_url) => RuntimeSessionCloseStore::connect(database_url).await?,
            None => RuntimeSessionCloseStore::memory(),
        };
        let runtime_execution_scopes = match config.runtime_session_database_url.as_deref() {
            Some(database_url) => RuntimeExecutionScopeStore::connect(database_url).await?,
            None => RuntimeExecutionScopeStore::memory(),
        };
        let runtime_tool_batches = match config.runtime_session_database_url.as_deref() {
            Some(database_url) => RuntimeToolBatchStore::connect(database_url).await?,
            None => RuntimeToolBatchStore::memory(),
        };
        let async_tool_dispatch =
            AsyncToolDispatch::new(config.async_tool_dispatch_topology.clone());
        let state = Self {
            runtime_grants: RuntimeGrantService::new(
                config.runtime_grant_secret.clone(),
                config.runtime_session_ttl,
            ),
            config,
            routing: RoutingEngine,
            plugin_management_client,
            project_context_client,
            providers,
            runtime_sessions,
            runtime_session_closes,
            runtime_execution_scopes,
            runtime_invocations,
            runtime_tool_batches,
            async_tool_dispatch,
        };
        Ok(state)
    }
}

fn task_runner_http_client(config: &AppConfig) -> Result<reqwest::Client, String> {
    #[cfg(test)]
    if config.task_runner_mtls_ca_cert_path.as_os_str().is_empty()
        && config
            .task_runner_mtls_client_identity_path
            .as_os_str()
            .is_empty()
    {
        return reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|err| format!("build test Task Runner Provider client failed: {err}"));
    }
    chatos_service_runtime::build_mtls_http_client(
        chatos_service_runtime::HttpClientTimeouts::new(config.task_runner_request_timeout),
        config.task_runner_mtls_ca_cert_path.as_path(),
        config.task_runner_mtls_client_identity_path.as_path(),
    )
}

#[cfg(not(test))]
async fn load_runtime_managed_resources(
) -> Result<(RuntimeSessionCacheLimits, RuntimeInvocationQuota), String> {
    let client = chatos_config_sdk::ConfigClient::from_env("mcp-management-service")
        .map_err(|error| format!("initialize MCP Management config client failed: {error}"))?;
    let snapshot = client
        .load_strict()
        .await
        .map_err(|error| format!("load fresh MCP Management configuration failed: {error}"))?;
    let max_entries = snapshot
        .usize(RUNTIME_SESSION_CACHE_MAX_ENTRIES_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "missing or invalid managed configuration key {RUNTIME_SESSION_CACHE_MAX_ENTRIES_CONFIG_KEY}"
            )
        })?;
    let max_bytes = snapshot
        .usize(RUNTIME_SESSION_CACHE_MAX_BYTES_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "missing or invalid managed configuration key {RUNTIME_SESSION_CACHE_MAX_BYTES_CONFIG_KEY}"
            )
        })?;
    let cache_limits = RuntimeSessionCacheLimits::new(max_entries, max_bytes)?;
    let required_string = |key: &str| {
        snapshot
            .string(key)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))
    };
    let required_u32 = |key: &str| {
        snapshot
            .u64(key)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))
    };
    let limits = RuntimeInvocationQuotaLimits::new(
        required_u32(INVOCATION_TENANT_ACTIVE_LIMIT_CONFIG_KEY)?,
        required_u32(INVOCATION_USER_ACTIVE_LIMIT_CONFIG_KEY)?,
        required_u32(INVOCATION_PROJECT_ACTIVE_LIMIT_CONFIG_KEY)?,
        required_u32(INVOCATION_DEVICE_ACTIVE_LIMIT_CONFIG_KEY)?,
    )?;
    let quota = RuntimeInvocationQuota::connect(
        required_string(INVOCATION_QUOTA_VALKEY_URL_CONFIG_KEY)?.as_str(),
        required_string(INVOCATION_QUOTA_KEY_PREFIX_CONFIG_KEY)?.as_str(),
        limits,
    )
    .await?;
    Ok((cache_limits, quota))
}

#[cfg(test)]
async fn load_runtime_managed_resources(
) -> Result<(RuntimeSessionCacheLimits, RuntimeInvocationQuota), String> {
    Ok((
        RuntimeSessionCacheLimits::new(2_048, 32 * 1024 * 1024)?,
        RuntimeInvocationQuota::memory(RuntimeInvocationQuotaLimits::new(
            100_000, 100_000, 100_000, 100_000,
        )?),
    ))
}
