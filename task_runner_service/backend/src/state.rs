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
use chatos_cloud_agent_runtime::CloudAgentStateStore;
use chatos_plugin_management_sdk::{PluginManagementClient, PluginManagementClientConfig};
use chatos_queue_observability::RabbitMqQueueInspector;
use memory_engine_sdk::UpsertSourceRequest;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Default)]
pub struct TaskRunnerRuntimeStats {
    worker_claim_failures_total: Arc<AtomicU64>,
    run_dispatch_fairness_deferrals_total: Arc<AtomicU64>,
    active_run_event_streams: Arc<AtomicUsize>,
    rabbitmq_consumer_reconnects_total: Arc<AtomicU64>,
    run_dispatch_consumer_connected: Arc<AtomicBool>,
    worker_control_consumer_connected: Arc<AtomicBool>,
    run_post_process_consumer_connected: Arc<AtomicBool>,
    callback_consumer_connected: Arc<AtomicBool>,
    run_event_consumer_connected: Arc<AtomicBool>,
    run_event_consumer_reconnects_total: Arc<AtomicU64>,
    run_event_consumer_events_total: Arc<AtomicU64>,
    run_event_retention_runs_total: Arc<AtomicU64>,
    run_event_retention_deleted_total: Arc<AtomicU64>,
    run_event_retention_failures_total: Arc<AtomicU64>,
    run_event_retention_last_deleted: Arc<AtomicU64>,
    run_event_retention_last_completed_at_unix: Arc<AtomicU64>,
    ask_user_prompt_retention_runs_total: Arc<AtomicU64>,
    ask_user_prompt_retention_deleted_total: Arc<AtomicU64>,
    ask_user_prompt_retention_failures_total: Arc<AtomicU64>,
    ask_user_prompt_retention_last_deleted: Arc<AtomicU64>,
    ask_user_prompt_retention_last_completed_at_unix: Arc<AtomicU64>,
    scheduler_pressure_paused: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct ActiveRunEventStreamLease {
    active_run_event_streams: Arc<AtomicUsize>,
}

fn current_unix_timestamp() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp()).unwrap_or_default()
}

impl TaskRunnerRuntimeStats {
    pub fn record_worker_claim_failure(&self) {
        self.worker_claim_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn worker_claim_failures_total(&self) -> u64 {
        self.worker_claim_failures_total.load(Ordering::Relaxed)
    }

    pub fn record_run_dispatch_fairness_deferral(&self) {
        self.run_dispatch_fairness_deferrals_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn run_dispatch_fairness_deferrals_total(&self) -> u64 {
        self.run_dispatch_fairness_deferrals_total
            .load(Ordering::Relaxed)
    }

    pub fn active_run_event_streams(&self) -> usize {
        self.active_run_event_streams.load(Ordering::Relaxed)
    }

    pub fn record_rabbitmq_consumer_reconnect(&self) {
        self.rabbitmq_consumer_reconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn rabbitmq_consumer_reconnects_total(&self) -> u64 {
        self.rabbitmq_consumer_reconnects_total
            .load(Ordering::Relaxed)
    }

    pub fn set_run_dispatch_consumer_connected(&self, connected: bool) {
        self.run_dispatch_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub fn run_dispatch_consumer_connected(&self) -> bool {
        self.run_dispatch_consumer_connected.load(Ordering::Relaxed)
    }

    pub fn set_worker_control_consumer_connected(&self, connected: bool) {
        self.worker_control_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub fn worker_control_consumer_connected(&self) -> bool {
        self.worker_control_consumer_connected
            .load(Ordering::Relaxed)
    }

    pub fn set_run_post_process_consumer_connected(&self, connected: bool) {
        self.run_post_process_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub fn run_post_process_consumer_connected(&self) -> bool {
        self.run_post_process_consumer_connected
            .load(Ordering::Relaxed)
    }

    pub fn set_callback_consumer_connected(&self, connected: bool) {
        self.callback_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub fn callback_consumer_connected(&self) -> bool {
        self.callback_consumer_connected.load(Ordering::Relaxed)
    }

    pub fn set_run_event_consumer_connected(&self, connected: bool) {
        self.run_event_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub fn run_event_consumer_connected(&self) -> bool {
        self.run_event_consumer_connected.load(Ordering::Relaxed)
    }

    pub fn record_run_event_consumer_reconnect(&self) {
        self.run_event_consumer_reconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn run_event_consumer_reconnects_total(&self) -> u64 {
        self.run_event_consumer_reconnects_total
            .load(Ordering::Relaxed)
    }

    pub fn record_run_event_consumed(&self) {
        self.run_event_consumer_events_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn run_event_consumer_events_total(&self) -> u64 {
        self.run_event_consumer_events_total.load(Ordering::Relaxed)
    }

    pub fn record_run_event_retention_success(&self, deleted_events: u64) {
        self.run_event_retention_runs_total
            .fetch_add(1, Ordering::Relaxed);
        self.run_event_retention_deleted_total
            .fetch_add(deleted_events, Ordering::Relaxed);
        self.run_event_retention_last_deleted
            .store(deleted_events, Ordering::Relaxed);
        self.run_event_retention_last_completed_at_unix
            .store(current_unix_timestamp(), Ordering::Relaxed);
    }

    pub fn record_run_event_retention_failure(&self) {
        self.run_event_retention_runs_total
            .fetch_add(1, Ordering::Relaxed);
        self.run_event_retention_failures_total
            .fetch_add(1, Ordering::Relaxed);
        self.run_event_retention_last_deleted
            .store(0, Ordering::Relaxed);
        self.run_event_retention_last_completed_at_unix
            .store(current_unix_timestamp(), Ordering::Relaxed);
    }

    pub fn run_event_retention_runs_total(&self) -> u64 {
        self.run_event_retention_runs_total.load(Ordering::Relaxed)
    }

    pub fn run_event_retention_deleted_total(&self) -> u64 {
        self.run_event_retention_deleted_total
            .load(Ordering::Relaxed)
    }

    pub fn run_event_retention_failures_total(&self) -> u64 {
        self.run_event_retention_failures_total
            .load(Ordering::Relaxed)
    }

    pub fn run_event_retention_last_deleted(&self) -> u64 {
        self.run_event_retention_last_deleted
            .load(Ordering::Relaxed)
    }

    pub fn run_event_retention_last_completed_at_unix(&self) -> u64 {
        self.run_event_retention_last_completed_at_unix
            .load(Ordering::Relaxed)
    }

    pub fn record_ask_user_prompt_retention_success(&self, deleted_prompts: u64) {
        self.ask_user_prompt_retention_runs_total
            .fetch_add(1, Ordering::Relaxed);
        self.ask_user_prompt_retention_deleted_total
            .fetch_add(deleted_prompts, Ordering::Relaxed);
        self.ask_user_prompt_retention_last_deleted
            .store(deleted_prompts, Ordering::Relaxed);
        self.ask_user_prompt_retention_last_completed_at_unix
            .store(current_unix_timestamp(), Ordering::Relaxed);
    }

    pub fn record_ask_user_prompt_retention_failure(&self) {
        self.ask_user_prompt_retention_runs_total
            .fetch_add(1, Ordering::Relaxed);
        self.ask_user_prompt_retention_failures_total
            .fetch_add(1, Ordering::Relaxed);
        self.ask_user_prompt_retention_last_deleted
            .store(0, Ordering::Relaxed);
        self.ask_user_prompt_retention_last_completed_at_unix
            .store(current_unix_timestamp(), Ordering::Relaxed);
    }

    pub fn ask_user_prompt_retention_runs_total(&self) -> u64 {
        self.ask_user_prompt_retention_runs_total
            .load(Ordering::Relaxed)
    }

    pub fn ask_user_prompt_retention_deleted_total(&self) -> u64 {
        self.ask_user_prompt_retention_deleted_total
            .load(Ordering::Relaxed)
    }

    pub fn ask_user_prompt_retention_failures_total(&self) -> u64 {
        self.ask_user_prompt_retention_failures_total
            .load(Ordering::Relaxed)
    }

    pub fn ask_user_prompt_retention_last_deleted(&self) -> u64 {
        self.ask_user_prompt_retention_last_deleted
            .load(Ordering::Relaxed)
    }

    pub fn ask_user_prompt_retention_last_completed_at_unix(&self) -> u64 {
        self.ask_user_prompt_retention_last_completed_at_unix
            .load(Ordering::Relaxed)
    }

    pub fn set_scheduler_pressure_paused(&self, paused: bool) {
        self.scheduler_pressure_paused
            .store(paused, Ordering::Relaxed);
    }

    pub fn scheduler_pressure_paused(&self) -> bool {
        self.scheduler_pressure_paused.load(Ordering::Relaxed)
    }

    pub fn acquire_run_event_stream(&self) -> ActiveRunEventStreamLease {
        self.active_run_event_streams
            .fetch_add(1, Ordering::Relaxed);
        ActiveRunEventStreamLease {
            active_run_event_streams: Arc::clone(&self.active_run_event_streams),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TaskRunnerRuntimeStats;

    #[test]
    fn rabbitmq_consumer_runtime_stats_track_connection_lifecycle() {
        let stats = TaskRunnerRuntimeStats::default();

        stats.set_run_dispatch_consumer_connected(true);
        stats.set_worker_control_consumer_connected(true);
        stats.set_run_post_process_consumer_connected(true);
        stats.set_callback_consumer_connected(true);
        stats.set_run_event_consumer_connected(true);
        stats.record_run_dispatch_fairness_deferral();
        stats.record_rabbitmq_consumer_reconnect();
        stats.record_run_event_consumer_reconnect();
        stats.record_run_event_consumed();
        stats.record_run_event_retention_success(12);
        stats.record_run_event_retention_failure();
        stats.record_ask_user_prompt_retention_success(9);
        stats.record_ask_user_prompt_retention_failure();
        stats.set_scheduler_pressure_paused(true);

        assert!(stats.run_dispatch_consumer_connected());
        assert!(stats.worker_control_consumer_connected());
        assert!(stats.run_post_process_consumer_connected());
        assert!(stats.callback_consumer_connected());
        assert!(stats.run_event_consumer_connected());
        assert_eq!(stats.run_dispatch_fairness_deferrals_total(), 1);
        assert_eq!(stats.rabbitmq_consumer_reconnects_total(), 1);
        assert_eq!(stats.run_event_consumer_reconnects_total(), 1);
        assert_eq!(stats.run_event_consumer_events_total(), 1);
        assert_eq!(stats.run_event_retention_runs_total(), 2);
        assert_eq!(stats.run_event_retention_deleted_total(), 12);
        assert_eq!(stats.run_event_retention_failures_total(), 1);
        assert_eq!(stats.run_event_retention_last_deleted(), 0);
        assert!(stats.run_event_retention_last_completed_at_unix() > 0);
        assert_eq!(stats.ask_user_prompt_retention_runs_total(), 2);
        assert_eq!(stats.ask_user_prompt_retention_deleted_total(), 9);
        assert_eq!(stats.ask_user_prompt_retention_failures_total(), 1);
        assert_eq!(stats.ask_user_prompt_retention_last_deleted(), 0);
        assert!(stats.ask_user_prompt_retention_last_completed_at_unix() > 0);
        assert!(stats.scheduler_pressure_paused());
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
    pub rabbitmq_queue_inspector: Option<RabbitMqQueueInspector>,
    pub run_event_resync_sender: broadcast::Sender<()>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        ensure_task_runner_memory_engine_source(&config).await?;
        let task_queue_topology = TaskQueueTopology::from_managed_env()?;
        let (run_event_resync_sender, _) = broadcast::channel(128);
        crate::run_event_queue::initialize_run_event_bus(
            task_queue_topology.clone(),
            run_event_resync_sender.clone(),
        )?;
        let rabbitmq_queue_inspector = if task_queue_topology.uses_rabbitmq() {
            let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
                "Task Runner RabbitMQ queue inspector requires the managed RabbitMQ URL".to_string()
            })?;
            Some(RabbitMqQueueInspector::new(rabbitmq_url)?)
        } else {
            None
        };
        let store = AppStore::new(&config).await?;
        let cloud_agent_store = match config.store_mode {
            crate::config::StoreMode::Memory => CloudAgentStateStore::memory(),
            crate::config::StoreMode::Mongo => {
                CloudAgentStateStore::connect(config.database_url.as_str()).await?
            }
        };
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
        let remote_server_service = RemoteServerService::new(config.clone(), store.clone());
        let ask_user_prompt_service = AskUserPromptService::new_with_config(
            store.clone(),
            config.clone(),
            task_queue_topology.clone(),
        );
        let runtime_stats = TaskRunnerRuntimeStats::default();
        let run_service = RunService::new_with_plugin_management(
            config.clone(),
            task_queue_topology.clone(),
            store.clone(),
            ask_user_prompt_service.clone(),
            plugin_management_client,
            runtime_stats.clone(),
            cloud_agent_store,
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
            rabbitmq_queue_inspector,
            run_event_resync_sender,
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
