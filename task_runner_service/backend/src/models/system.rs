// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_queue_observability::RabbitMqQueueRuntimeStats;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub now: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerSystemStatsResponse {
    pub ok: bool,
    pub service: &'static str,
    pub now: String,
    pub runtime: TaskRunnerRuntimeStatsSnapshot,
    pub queue: TaskRunnerQueueStatsSnapshot,
    pub runs: TaskRunnerRunStatsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerRuntimeStatsSnapshot {
    pub worker_claim_failures_total: u64,
    pub run_dispatch_fairness_deferrals_total: u64,
    pub active_run_event_streams: usize,
    pub pending_sse_tickets: usize,
    pub rabbitmq_consumer_reconnects_total: u64,
    pub run_dispatch_consumer_connected: bool,
    pub worker_control_consumer_connected: bool,
    pub run_post_process_consumer_connected: bool,
    pub callback_consumer_connected: bool,
    pub run_event_consumer_connected: bool,
    pub run_event_consumer_reconnects_total: u64,
    pub run_event_consumer_events_total: u64,
    pub run_event_retention_runs_total: u64,
    pub run_event_retention_deleted_total: u64,
    pub run_event_retention_failures_total: u64,
    pub run_event_retention_last_deleted: u64,
    pub run_event_retention_last_completed_at_unix: u64,
    pub ask_user_prompt_retention_runs_total: u64,
    pub ask_user_prompt_retention_deleted_total: u64,
    pub ask_user_prompt_retention_failures_total: u64,
    pub ask_user_prompt_retention_last_deleted: u64,
    pub ask_user_prompt_retention_last_completed_at_unix: u64,
    pub scheduler_pressure_paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerQueueStatsSnapshot {
    pub rabbitmq_enabled: bool,
    pub callback_delivery_mode: String,
    pub run_events_publish_mode: String,
    pub rabbitmq_exchange: String,
    pub rabbitmq_reconnect_ms: u64,
    pub rabbitmq_queues: RabbitMqQueueRuntimeStats,
    pub worker_consumers_expected: bool,
    pub callback_consumer_expected: bool,
    pub event_outbox_reconcile_ms: u64,
    pub event_outbox_batch_size: usize,
    pub worker_control_queue_prefix: String,
    pub run_post_process_queue: String,
    pub run_post_process_retry_queue: String,
    pub run_post_process_dead_letter_queue: String,
    pub run_post_process_max_delivery_attempts: u32,
    pub run_post_process_retry_delay_ms: u64,
    pub run_post_process_outbox_reconcile_ms: u64,
    pub run_post_process_outbox_batch_size: usize,
    pub callback_delivery_queue: String,
    pub run_events_routing_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerRunStatsSnapshot {
    pub total: usize,
    pub active: usize,
    pub queued: usize,
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub blocked: usize,
    pub dispatch_paused: usize,
    pub callback_pending: usize,
    pub callback_enqueued: usize,
    pub dispatch_outbox_pending: usize,
    pub cancellation_outbox_pending: usize,
    pub post_process_outbox_pending: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunExecutionStats {
    pub total: usize,
    pub active: usize,
    pub queued: usize,
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub blocked: usize,
    pub dispatch_paused: usize,
    pub callback_pending: usize,
    pub callback_enqueued: usize,
    pub dispatch_outbox_pending: usize,
    pub cancellation_outbox_pending: usize,
    pub post_process_outbox_pending: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunEventPruneResult {
    pub eligible_runs: usize,
    pub deleted_events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AskUserPromptPruneResult {
    pub eligible_prompts: usize,
    pub deleted_prompts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfigResponse {
    pub host: String,
    pub port: u16,
    pub store_mode: String,
    pub database_url: String,
    pub memory_engine_base_url: Option<String>,
    pub memory_engine_source_id: String,
    pub memory_engine_configured: bool,
    pub default_tenant_id: String,
    pub default_subject_id: String,
    pub default_workspace_dir: String,
    pub memory_timeout_ms: u64,
    pub default_execution_timeout_ms: u64,
    pub execution_timeout_ms: u64,
    pub scheduler_poll_interval_ms: u64,
    pub worker_claim_ttl_ms: u64,
    pub worker_concurrency: usize,
    pub auto_memory_summary: bool,
    pub default_task_execution_max_iterations: usize,
    pub task_execution_max_iterations: usize,
    pub task_runner_review_read_only_iterations: usize,
    pub task_runner_review_missing_read_failures: usize,
    pub task_runner_review_repeat_interval_iterations: usize,
    pub default_tool_result_model_max_chars: usize,
    pub tool_result_model_max_chars: usize,
    pub default_tool_results_model_total_max_chars: usize,
    pub tool_results_model_total_max_chars: usize,
    pub task_queue_rabbitmq_enabled: bool,
    pub task_queue_callback_delivery_mode: String,
    pub task_queue_run_events_publish_mode: String,
    pub task_queue_rabbitmq_exchange: String,
    pub task_queue_event_outbox_reconcile_ms: u64,
    pub task_queue_event_outbox_batch_size: usize,
    pub task_queue_worker_control_queue_prefix: String,
    pub task_queue_callback_delivery_queue: String,
    pub task_queue_run_events_routing_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerInternalPromptPreviewResponse {
    pub locale: String,
    pub task_prompt_template: String,
    pub global_execution_prompt: String,
    pub process_log_system_prompt: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}
