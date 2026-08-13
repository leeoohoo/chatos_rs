// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use crate::models::{
    now_rfc3339, TaskRunnerQueueStatsSnapshot, TaskRunnerRunStatsSnapshot,
    TaskRunnerRuntimeStatsSnapshot, TaskRunnerSystemStatsResponse,
};
use axum::http::header;
use axum::response::IntoResponse;
use chatos_queue_observability::{RabbitMqQueueRuntimeStats, RabbitMqQueueSpec};
use serde::Deserialize;
use serde::Serialize;

use super::internal_auth::{
    require_task_runner_internal_request, EXECUTION_OPTIONS_READ_SCOPE, MCP_MANAGEMENT_CALLER,
    PROJECT_SERVICE_CALLER, SYSTEM_STATS_READ_SCOPE,
};
use super::*;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

#[derive(Debug, Serialize)]
pub(super) struct InternalExecutionOptionsResponse {
    pub model_config_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ReplayRunPostProcessRequest {
    pub operation_id: String,
    pub run_id: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ReplayRunPostProcessResponse {
    pub operation_id: String,
    pub run_id: String,
    pub event_enqueued: bool,
    pub dead_letter_archived: bool,
}

pub(super) async fn replay_run_post_process(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<ReplayRunPostProcessRequest>,
) -> Result<Json<ReplayRunPostProcessResponse>, ApiError> {
    require_admin_user(&current_user)?;
    let run_id = input.run_id.trim();
    let reason = input.reason.trim();
    let operation_id = input.operation_id.trim();
    if operation_id.is_empty()
        || operation_id.len() > 100
        || run_id.is_empty()
        || reason.len() < 8
        || reason.len() > 500
    {
        return Err(ApiError::bad_request(
            "operation_id, run_id and an 8..500 character replay reason are required",
        ));
    }
    tracing::warn!(
        run_id,
        actor_user_id = current_user.id.as_str(),
        actor_username = current_user.username.as_str(),
        operation_id,
        reason,
        "administrator requested Run post-process dead-letter replay"
    );
    let (run, dead_letter_archived) = state
        .run_service
        .replay_run_post_process_dead_letter(run_id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(ReplayRunPostProcessResponse {
        operation_id: operation_id.to_string(),
        run_id: run.id,
        event_enqueued: run.post_process_event_enqueued,
        dead_letter_archived,
    }))
}

pub(super) async fn get_user_execution_options(
    Path(owner_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<InternalExecutionOptionsResponse>, ApiError> {
    require_task_runner_internal_request(
        &state.config,
        &headers,
        &[PROJECT_SERVICE_CALLER],
        EXECUTION_OPTIONS_READ_SCOPE,
    )
    .map_err(|err| ApiError {
        status: err.status,
        message: err.message,
    })?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err(ApiError::bad_request("owner_user_id is required"));
    }

    let model_config_ids = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|model| model.enabled)
        .filter(|model| owns_resource(model.owner_user_id.as_deref(), owner_user_id))
        .map(|model| model.id)
        .collect::<BTreeSet<_>>();

    Ok(Json(InternalExecutionOptionsResponse {
        model_config_ids: model_config_ids.into_iter().collect(),
    }))
}

pub(super) async fn get_system_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TaskRunnerSystemStatsResponse>, ApiError> {
    require_task_runner_internal_request(
        &state.config,
        &headers,
        &[PROJECT_SERVICE_CALLER, MCP_MANAGEMENT_CALLER],
        SYSTEM_STATS_READ_SCOPE,
    )
    .map_err(|err| ApiError {
        status: err.status,
        message: err.message,
    })?;
    let rabbitmq_queues = task_runner_rabbitmq_queue_stats(&state).await;
    let run_stats = state
        .run_service
        .execution_stats()
        .await
        .map_err(ApiError::internal)?;
    let sse_ticket_stats = state.sse_tickets.stats();
    Ok(Json(TaskRunnerSystemStatsResponse {
        ok: true,
        service: "task_runner_service_backend",
        now: now_rfc3339(),
        runtime: TaskRunnerRuntimeStatsSnapshot {
            worker_claim_failures_total: state.runtime_stats.worker_claim_failures_total(),
            run_dispatch_fairness_deferrals_total: state
                .runtime_stats
                .run_dispatch_fairness_deferrals_total(),
            active_run_event_streams: state.runtime_stats.active_run_event_streams(),
            pending_sse_tickets: sse_ticket_stats.active_ticket_count,
            rabbitmq_consumer_reconnects_total: state
                .runtime_stats
                .rabbitmq_consumer_reconnects_total(),
            run_dispatch_consumer_connected: state.runtime_stats.run_dispatch_consumer_connected(),
            worker_control_consumer_connected: state
                .runtime_stats
                .worker_control_consumer_connected(),
            run_post_process_consumer_connected: state
                .runtime_stats
                .run_post_process_consumer_connected(),
            callback_consumer_connected: state.runtime_stats.callback_consumer_connected(),
            run_event_consumer_connected: state.runtime_stats.run_event_consumer_connected(),
            run_event_consumer_reconnects_total: state
                .runtime_stats
                .run_event_consumer_reconnects_total(),
            run_event_consumer_events_total: state.runtime_stats.run_event_consumer_events_total(),
            run_event_retention_runs_total: state.runtime_stats.run_event_retention_runs_total(),
            run_event_retention_deleted_total: state
                .runtime_stats
                .run_event_retention_deleted_total(),
            run_event_retention_failures_total: state
                .runtime_stats
                .run_event_retention_failures_total(),
            run_event_retention_last_deleted: state
                .runtime_stats
                .run_event_retention_last_deleted(),
            run_event_retention_last_completed_at_unix: state
                .runtime_stats
                .run_event_retention_last_completed_at_unix(),
            ask_user_prompt_retention_runs_total: state
                .runtime_stats
                .ask_user_prompt_retention_runs_total(),
            ask_user_prompt_retention_deleted_total: state
                .runtime_stats
                .ask_user_prompt_retention_deleted_total(),
            ask_user_prompt_retention_failures_total: state
                .runtime_stats
                .ask_user_prompt_retention_failures_total(),
            ask_user_prompt_retention_last_deleted: state
                .runtime_stats
                .ask_user_prompt_retention_last_deleted(),
            ask_user_prompt_retention_last_completed_at_unix: state
                .runtime_stats
                .ask_user_prompt_retention_last_completed_at_unix(),
            scheduler_pressure_paused: state.runtime_stats.scheduler_pressure_paused(),
        },
        queue: TaskRunnerQueueStatsSnapshot {
            rabbitmq_enabled: state.task_queue_topology.uses_rabbitmq(),
            callback_delivery_mode: state
                .task_queue_topology
                .callback_delivery_mode
                .as_str()
                .to_string(),
            run_events_publish_mode: state
                .task_queue_topology
                .run_events_publish_mode
                .as_str()
                .to_string(),
            rabbitmq_exchange: state.task_queue_topology.rabbitmq_exchange.clone(),
            rabbitmq_reconnect_ms: state
                .task_queue_topology
                .rabbitmq_reconnect_delay
                .as_millis() as u64,
            rabbitmq_queues,
            worker_consumers_expected: state.config.worker_enabled(),
            callback_consumer_expected: state.config.callback_delivery_enabled()
                && state.task_queue_topology.callback_delivery_mode
                    == crate::platform_queue::TaskQueueMode::RabbitMq,
            event_outbox_reconcile_ms: state
                .task_queue_topology
                .event_outbox_reconcile_interval
                .as_millis() as u64,
            event_outbox_batch_size: state.task_queue_topology.event_outbox_batch_size,
            worker_control_queue_prefix: state
                .task_queue_topology
                .worker_control_queue_prefix
                .clone(),
            run_post_process_queue: state.task_queue_topology.run_post_process_queue.clone(),
            run_post_process_retry_queue: state
                .task_queue_topology
                .run_post_process_retry_queue
                .clone(),
            run_post_process_dead_letter_queue: state
                .task_queue_topology
                .run_post_process_dead_letter_queue
                .clone(),
            run_post_process_max_delivery_attempts: state
                .task_queue_topology
                .run_post_process_max_delivery_attempts,
            run_post_process_retry_delay_ms: state
                .task_queue_topology
                .run_post_process_retry_delay
                .as_millis() as u64,
            run_post_process_outbox_reconcile_ms: state
                .task_queue_topology
                .run_post_process_outbox_reconcile_interval
                .as_millis() as u64,
            run_post_process_outbox_batch_size: state
                .task_queue_topology
                .run_post_process_outbox_batch_size,
            callback_delivery_queue: state.task_queue_topology.callback_delivery_queue.clone(),
            run_events_routing_key: state.task_queue_topology.run_events_routing_key.clone(),
        },
        runs: TaskRunnerRunStatsSnapshot {
            total: run_stats.total,
            active: run_stats.active,
            queued: run_stats.queued,
            running: run_stats.running,
            succeeded: run_stats.succeeded,
            failed: run_stats.failed,
            cancelled: run_stats.cancelled,
            blocked: run_stats.blocked,
            dispatch_paused: run_stats.dispatch_paused,
            callback_pending: run_stats.callback_pending,
            callback_enqueued: run_stats.callback_enqueued,
            dispatch_outbox_pending: run_stats.dispatch_outbox_pending,
            cancellation_outbox_pending: run_stats.cancellation_outbox_pending,
            post_process_outbox_pending: run_stats.post_process_outbox_pending,
        },
    }))
}

pub(super) async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let stats = task_runner_rabbitmq_queue_stats(&state).await;
    let mut body = chatos_queue_observability::render_prometheus_metrics("task-runner", &stats);
    body.push_str(
        "# HELP chatos_task_runner_run_dispatch_fairness_deferrals_total Fair scheduling triggers deferred because all eligible project execution lanes were occupied.\n\
# TYPE chatos_task_runner_run_dispatch_fairness_deferrals_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_dispatch_fairness_deferrals_total {}\n",
            state.runtime_stats.run_dispatch_fairness_deferrals_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_consumer_connected Whether this API instance is consuming Run events from RabbitMQ.\n\
# TYPE chatos_task_runner_run_event_consumer_connected gauge\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_consumer_connected {}\n",
            u8::from(state.runtime_stats.run_event_consumer_connected())
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_consumer_reconnects_total Run event RabbitMQ consumer reconnect attempts.\n\
# TYPE chatos_task_runner_run_event_consumer_reconnects_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_consumer_reconnects_total {}\n",
            state.runtime_stats.run_event_consumer_reconnects_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_consumer_events_total Valid Run events consumed from RabbitMQ.\n\
# TYPE chatos_task_runner_run_event_consumer_events_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_consumer_events_total {}\n",
            state.runtime_stats.run_event_consumer_events_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_retention_runs_total Run event retention cleanup attempts.\n\
# TYPE chatos_task_runner_run_event_retention_runs_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_retention_runs_total {}\n",
            state.runtime_stats.run_event_retention_runs_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_retention_deleted_total Expired terminal Run events deleted by retention cleanup.\n\
# TYPE chatos_task_runner_run_event_retention_deleted_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_retention_deleted_total {}\n",
            state.runtime_stats.run_event_retention_deleted_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_retention_failures_total Run event retention cleanup failures.\n\
# TYPE chatos_task_runner_run_event_retention_failures_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_retention_failures_total {}\n",
            state.runtime_stats.run_event_retention_failures_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_retention_last_deleted Events deleted by the most recent retention cleanup.\n\
# TYPE chatos_task_runner_run_event_retention_last_deleted gauge\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_retention_last_deleted {}\n",
            state.runtime_stats.run_event_retention_last_deleted()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_run_event_retention_last_completed_at_unix Unix timestamp of the most recent retention cleanup completion.\n\
# TYPE chatos_task_runner_run_event_retention_last_completed_at_unix gauge\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_run_event_retention_last_completed_at_unix {}\n",
            state
                .runtime_stats
                .run_event_retention_last_completed_at_unix()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_ask_user_prompt_retention_runs_total Ask User prompt retention cleanup attempts.\n\
# TYPE chatos_task_runner_ask_user_prompt_retention_runs_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_ask_user_prompt_retention_runs_total {}\n",
            state.runtime_stats.ask_user_prompt_retention_runs_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_ask_user_prompt_retention_deleted_total Expired terminal Ask User prompts deleted by retention cleanup.\n\
# TYPE chatos_task_runner_ask_user_prompt_retention_deleted_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_ask_user_prompt_retention_deleted_total {}\n",
            state
                .runtime_stats
                .ask_user_prompt_retention_deleted_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_ask_user_prompt_retention_failures_total Ask User prompt retention cleanup failures.\n\
# TYPE chatos_task_runner_ask_user_prompt_retention_failures_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_ask_user_prompt_retention_failures_total {}\n",
            state
                .runtime_stats
                .ask_user_prompt_retention_failures_total()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_ask_user_prompt_retention_last_deleted Ask User prompts deleted by the most recent retention cleanup.\n\
# TYPE chatos_task_runner_ask_user_prompt_retention_last_deleted gauge\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_ask_user_prompt_retention_last_deleted {}\n",
            state.runtime_stats.ask_user_prompt_retention_last_deleted()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_ask_user_prompt_retention_last_completed_at_unix Unix timestamp of the most recent Ask User prompt retention cleanup completion.\n\
# TYPE chatos_task_runner_ask_user_prompt_retention_last_completed_at_unix gauge\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_ask_user_prompt_retention_last_completed_at_unix {}\n",
            state
                .runtime_stats
                .ask_user_prompt_retention_last_completed_at_unix()
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_task_runner_scheduler_pressure_paused Whether scheduled task discovery is paused by critical platform pressure.\n\
# TYPE chatos_task_runner_scheduler_pressure_paused gauge\n",
    );
    body.push_str(
        format!(
            "chatos_task_runner_scheduler_pressure_paused {}\n",
            u8::from(state.runtime_stats.scheduler_pressure_paused())
        )
        .as_str(),
    );
    ([(header::CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)], body)
}

async fn task_runner_rabbitmq_queue_stats(state: &AppState) -> RabbitMqQueueRuntimeStats {
    let Some(inspector) = state.rabbitmq_queue_inspector.as_ref() else {
        return RabbitMqQueueRuntimeStats::disabled();
    };
    let topology = &state.task_queue_topology;
    let mut specs = vec![
        RabbitMqQueueSpec::new(
            "cloud_agent_runtime",
            crate::cloud_agent_queue::TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
        ),
        RabbitMqQueueSpec::new(
            "cloud_agent_runtime_retry",
            crate::cloud_agent_queue::TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY,
        ),
        RabbitMqQueueSpec::new("run_post_process", topology.run_post_process_queue.as_str()),
        RabbitMqQueueSpec::new(
            "run_post_process_retry",
            topology.run_post_process_retry_queue.as_str(),
        ),
        RabbitMqQueueSpec::new(
            "run_post_process_dead_letter",
            topology.run_post_process_dead_letter_queue.as_str(),
        ),
    ];
    if topology.callback_delivery_mode == crate::platform_queue::TaskQueueMode::RabbitMq {
        specs.push(RabbitMqQueueSpec::new(
            "callback_delivery",
            topology.callback_delivery_queue.as_str(),
        ));
    }
    inspector.inspect(specs.as_slice()).await
}

fn owns_resource(owner_user_id: Option<&str>, expected_owner_user_id: &str) -> bool {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
}

#[cfg(test)]
#[path = "internal/tests.rs"]
mod tests;
