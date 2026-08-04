// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::{AuthService, SseTicketStore};
use crate::config::AppConfig;
use crate::mcp_server::TaskRunnerMcpService;
use crate::platform_queue::TaskQueueTopology;
use crate::services::{
    McpCatalogService, ModelConfigService, RemoteServerService, RunService, TaskProjectService,
    TaskService, ToolingStateService,
};
use crate::store::AppStore;
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};
use memory_engine_sdk::UpsertSourceRequest;
use serde_json::json;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct TaskRunnerRuntimeStats {
    worker_claim_failures_total: Arc<AtomicU64>,
    active_run_event_streams: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
pub struct ActiveRunEventStreamLease {
    active_run_event_streams: Arc<AtomicUsize>,
}

impl TaskRunnerRuntimeStats {
    pub fn record_worker_claim_failure(&self) {
        self.worker_claim_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn worker_claim_failures_total(&self) -> u64 {
        self.worker_claim_failures_total.load(Ordering::Relaxed)
    }

    pub fn active_run_event_streams(&self) -> usize {
        self.active_run_event_streams.load(Ordering::Relaxed)
    }

    pub fn acquire_run_event_stream(&self) -> ActiveRunEventStreamLease {
        self.active_run_event_streams
            .fetch_add(1, Ordering::Relaxed);
        ActiveRunEventStreamLease {
            active_run_event_streams: Arc::clone(&self.active_run_event_streams),
        }
    }
}

impl Drop for ActiveRunEventStreamLease {
    fn drop(&mut self) {
        self.active_run_event_streams
            .fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub task_queue_topology: TaskQueueTopology,
    pub task_service: TaskService,
    pub model_config_service: ModelConfigService,
    pub remote_server_service: RemoteServerService,
    pub task_project_service: TaskProjectService,
    pub run_service: RunService,
    pub ask_user_prompt_service: AskUserPromptService,
    pub mcp_catalog_service: McpCatalogService,
    pub tooling_state_service: ToolingStateService,
    pub task_runner_mcp_service: TaskRunnerMcpService,
    pub auth_service: AuthService,
    pub sse_tickets: SseTicketStore,
    pub runtime_stats: TaskRunnerRuntimeStats,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        ensure_task_runner_memory_engine_source(&config).await?;
        let task_queue_topology = TaskQueueTopology::from_managed_env()?;
        let store = AppStore::new(&config).await?;
        let auth_service = AuthService::new(config.clone(), store.clone());
        auth_service.ensure_default_admin(&config).await?;
        let plugin_management_config = PluginManagementClientConfig::from_env("task-runner")
            .await
            .map_err(|err| format!("load plugin management client config failed: {err}"))?;
        let plugin_management_client = PluginManagementClient::new(plugin_management_config)
            .map_err(|err| format!("initialize plugin management client failed: {err}"))?;
        let task_service = TaskService::new_with_plugin_management(
            config.clone(),
            store.clone(),
            plugin_management_client.clone(),
        );
        let model_config_service = ModelConfigService::new(store.clone());
        let task_project_service =
            TaskProjectService::new_with_config(store.clone(), config.clone());
        task_project_service.ensure_public_project().await?;
        let remote_server_service = RemoteServerService::new(store.clone());
        let ask_user_prompt_service =
            AskUserPromptService::new_with_config(store.clone(), config.clone());
        let runtime_stats = TaskRunnerRuntimeStats::default();
        let run_service = RunService::new_with_plugin_management(
            config.clone(),
            task_queue_topology.clone(),
            store.clone(),
            ask_user_prompt_service.clone(),
            plugin_management_client,
            runtime_stats.clone(),
        );
        let mcp_catalog_service =
            McpCatalogService::new(task_service.clone(), ask_user_prompt_service.clone());
        let tooling_state_service = ToolingStateService::new(config.clone());
        let task_runner_mcp_service = TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service.clone(),
            run_service.clone(),
            ask_user_prompt_service.clone(),
        );
        Ok(Self {
            config,
            task_queue_topology,
            task_service,
            model_config_service,
            remote_server_service,
            task_project_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
            tooling_state_service,
            task_runner_mcp_service,
            auth_service,
            sse_tickets: SseTicketStore::default(),
            runtime_stats,
        })
    }
}

async fn ensure_task_runner_memory_engine_source(config: &AppConfig) -> Result<(), String> {
    let Some(client) = config.memory_client()? else {
        return Ok(());
    };
    let source_id = config.memory_engine_source_id.trim();
    if source_id.is_empty() {
        return Ok(());
    }
    client
        .upsert_source(
            source_id,
            &UpsertSourceRequest {
                tenant_id: None,
                source_type: "task_runner".to_string(),
                name: "Task Runner".to_string(),
                description: Some(
                    "Task Runner managed source for task threads, run records, summaries, and subject memories."
                        .to_string(),
                ),
                config: Some(json!({
                    "platform_managed": true,
                    "owner_service": "task_runner_service_backend",
                    "mapping_version": "task_runner.v1",
                    "capabilities": [
                        "threads",
                        "records",
                        "summaries",
                        "subject_memories"
                    ],
                })),
                sdk_enabled: Some(true),
                status: Some("active".to_string()),
            },
        )
        .await?;
    Ok(())
}
