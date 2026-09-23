// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::async_dispatch::AsyncToolDispatch;
use crate::config::AppConfig;
use crate::providers::{
    ChatosProviderConfig, ProviderDispatcher, ProviderRuntimeConfig, TaskRunnerProviderConfig,
};
use crate::routing::RoutingEngine;
use crate::runtime::{
    RuntimeExecutionScopeStore, RuntimeGrantService, RuntimeInvocationQuota,
    RuntimeInvocationQuotaLimits, RuntimeInvocationStore, RuntimeRetention,
    RuntimeSessionCacheLimits, RuntimeSessionCloseStore, RuntimeSessionStore,
    RuntimeToolBatchStore,
};
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};
use std::sync::Arc;
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
    pub(crate) postgres_pool: Option<chatos_postgres::PgPool>,
    pub routing: RoutingEngine,
    pub plugin_management_client: PluginManagementClient,
    pub providers: ProviderDispatcher,
    pub(crate) skill_attestations:
        Arc<crate::providers::plugin_components::SkillActivationAttestationService>,
    pub runtime_grants: RuntimeGrantService,
    pub runtime_sessions: RuntimeSessionStore,
    pub runtime_session_closes: RuntimeSessionCloseStore,
    pub runtime_retention: Option<RuntimeRetention>,
    pub runtime_execution_scopes: RuntimeExecutionScopeStore,
    pub runtime_invocations: RuntimeInvocationStore,
    pub runtime_tool_batches: RuntimeToolBatchStore,
    pub async_tool_dispatch: AsyncToolDispatch,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let (runtime_session_cache_limits, runtime_invocation_quota) =
            load_runtime_managed_resources().await?;
        let runtime_database_pool = match config.runtime_session_database_url.as_deref() {
            Some(database_url) => Some(crate::postgres::connect(database_url).await?),
            None => None,
        };
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
        let skill_attestations = Arc::new(match runtime_database_pool.as_ref() {
            Some(pool) => {
                crate::providers::plugin_components::SkillActivationAttestationService::from_pool(
                    config.runtime_grant_secret.as_str(),
                    pool.clone(),
                )
                .await?
            }
            None => crate::providers::plugin_components::SkillActivationAttestationService::new(
                config.runtime_grant_secret.as_str(),
            )?,
        });
        let providers = ProviderDispatcher::new(
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
            ProviderRuntimeConfig {
                downstream_request_timeout: config.downstream_request_timeout,
                local_connector_request_timeout: config.local_connector_request_timeout,
                response_limit_bytes: config.provider_response_limit_bytes,
            },
            skill_attestations.clone(),
        )?;
        let runtime_sessions = match runtime_database_pool.as_ref() {
            Some(pool) => RuntimeSessionStore::from_pool(
                pool.clone(),
                config.runtime_session_encryption_secret.as_str(),
                runtime_session_cache_limits,
            )?,
            None => RuntimeSessionStore::memory(),
        };
        let runtime_invocations = match runtime_database_pool.as_ref() {
            Some(pool) => RuntimeInvocationStore::from_pool(pool.clone(), runtime_invocation_quota),
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
        let runtime_session_closes = match runtime_database_pool.as_ref() {
            Some(pool) => RuntimeSessionCloseStore::from_pool(pool.clone()),
            None => RuntimeSessionCloseStore::memory(),
        };
        let runtime_execution_scopes = match runtime_database_pool.as_ref() {
            Some(pool) => RuntimeExecutionScopeStore::from_pool(pool.clone()),
            None => RuntimeExecutionScopeStore::memory(),
        };
        let runtime_tool_batches = match runtime_database_pool.as_ref() {
            Some(pool) => RuntimeToolBatchStore::from_pool(pool.clone()),
            None => RuntimeToolBatchStore::memory(),
        };
        let runtime_retention = match runtime_database_pool.as_ref() {
            Some(pool) => Some(RuntimeRetention::new(
                pool.clone(),
                config.runtime_retention_interval,
                config.runtime_retention_batch_size,
            )?),
            None => None,
        };
        let async_tool_dispatch =
            AsyncToolDispatch::new(config.async_tool_dispatch_topology.clone());
        let state = Self {
            postgres_pool: runtime_database_pool,
            runtime_grants: RuntimeGrantService::new(
                config.runtime_grant_secret.clone(),
                config.runtime_session_ttl,
            ),
            config,
            routing: RoutingEngine,
            plugin_management_client,
            providers,
            skill_attestations,
            runtime_sessions,
            runtime_session_closes,
            runtime_retention,
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
