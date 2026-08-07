// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt::Write;

use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use chatos_queue_observability::RabbitMqQueueRuntimeStats;

use crate::async_dispatch::AsyncToolDispatchRuntimeStats;
use crate::auth::require_internal_request;
use crate::config::AsyncToolDispatchMode;
use crate::error::ApiError;
use crate::runtime::{RuntimeInvocationStoreStats, RuntimeSessionStoreStats};
use crate::state::AppState;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

#[derive(Debug, Serialize)]
pub(super) struct SystemStatsResponse {
    ok: bool,
    service: &'static str,
    now: String,
    async_tool_dispatch: AsyncToolDispatchStatsResponse,
    runtime_sessions: RuntimeSessionStoreStats,
    runtime_invocations: RuntimeInvocationStoreStats,
}

#[derive(Debug, Serialize)]
struct AsyncToolDispatchStatsResponse {
    mode: &'static str,
    worker_concurrency: usize,
    local_queue_buffer: usize,
    queue_max_length: u32,
    queue_max_bytes: u64,
    rabbitmq_reconnect_ms: u64,
    max_delivery_attempts: u32,
    retry_delay_ms: u64,
    result_outbox_reconcile_ms: u64,
    result_outbox_batch_size: i64,
    rabbitmq_enabled: bool,
    rabbitmq_exchange: Option<String>,
    cancellation_exchange: Option<String>,
    dispatch_queue: Option<String>,
    retry_queue: Option<String>,
    dead_letter_queue: Option<String>,
    rabbitmq_queues: RabbitMqQueueRuntimeStats,
    runtime: AsyncToolDispatchRuntimeStats,
}

pub(super) async fn system_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SystemStatsResponse>, ApiError> {
    require_internal_request(&state.config, &headers, "system.stats.read")?;
    let rabbitmq_queues = state.async_tool_dispatch.rabbitmq_queue_stats().await;
    let topology = state.async_tool_dispatch.topology();
    let runtime_sessions = state
        .runtime_sessions
        .stats()
        .await
        .map_err(ApiError::internal)?;
    let runtime_invocations = state
        .runtime_invocations
        .stats()
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(SystemStatsResponse {
        ok: true,
        service: "mcp-management-service",
        now: chrono::Utc::now().to_rfc3339(),
        async_tool_dispatch: AsyncToolDispatchStatsResponse {
            mode: topology.mode.as_str(),
            worker_concurrency: topology.worker_concurrency,
            local_queue_buffer: topology.local_queue_buffer,
            queue_max_length: topology.queue_max_length,
            queue_max_bytes: topology.queue_max_bytes,
            rabbitmq_reconnect_ms: topology.rabbitmq_reconnect_delay.as_millis() as u64,
            max_delivery_attempts: topology.max_delivery_attempts,
            retry_delay_ms: topology.retry_delay.as_millis() as u64,
            result_outbox_reconcile_ms: topology.result_outbox_reconcile_interval.as_millis()
                as u64,
            result_outbox_batch_size: topology.result_outbox_batch_size,
            rabbitmq_enabled: topology.mode == AsyncToolDispatchMode::RabbitMq,
            rabbitmq_exchange: topology.rabbitmq_exchange.clone(),
            cancellation_exchange: topology.cancellation_exchange.clone(),
            dispatch_queue: topology.queue_name.clone(),
            retry_queue: topology.retry_queue_name.clone(),
            dead_letter_queue: topology.dead_letter_queue_name.clone(),
            rabbitmq_queues,
            runtime: state.async_tool_dispatch.runtime_stats(),
        },
        runtime_sessions,
        runtime_invocations,
    }))
}

pub(super) async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.async_tool_dispatch.rabbitmq_queue_stats().await;
    let mut body =
        chatos_queue_observability::render_prometheus_metrics("mcp-management-service", &stats);
    match state.runtime_sessions.stats().await {
        Ok(stats) => append_runtime_session_metrics(&mut body, &stats),
        Err(_) => append_runtime_session_metrics_unavailable(&mut body),
    }
    match state.runtime_invocations.stats().await {
        Ok(stats) => append_runtime_invocation_metrics(&mut body, &stats),
        Err(_) => append_runtime_invocation_metrics_unavailable(&mut body),
    }
    ([(header::CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)], body)
}

fn append_runtime_session_metrics(body: &mut String, stats: &RuntimeSessionStoreStats) {
    body.push_str(
        "# HELP chatos_mcp_runtime_session_metrics_available Whether MCP Runtime Session metrics were available for this scrape.\n\
# TYPE chatos_mcp_runtime_session_metrics_available gauge\n\
chatos_mcp_runtime_session_metrics_available{service=\"mcp-management-service\"} 1\n",
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_sessions_active",
        "Active MCP Runtime Sessions stored by the service.",
        stats.active_session_count,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_session_cache_entries",
        "MCP Runtime Session snapshots currently held in the local cache.",
        stats.cached_session_count,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_session_cache_bytes",
        "Approximate bytes currently held by the MCP Runtime Session cache.",
        stats.cached_total_bytes,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_session_cache_snapshot_bytes_avg",
        "Average approximate bytes per cached MCP Runtime Session snapshot.",
        stats.cached_avg_snapshot_bytes,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_session_cache_snapshot_bytes_p95",
        "Approximate p95 bytes per cached MCP Runtime Session snapshot.",
        stats.cached_p95_snapshot_bytes,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_session_cache_entry_limit",
        "Configured MCP Runtime Session cache entry limit; zero means not separately limited.",
        stats.cache_entry_limit.unwrap_or_default(),
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_session_cache_byte_limit",
        "Configured MCP Runtime Session cache byte limit; zero means not separately limited.",
        stats.cache_byte_limit.unwrap_or_default(),
    );
    append_counter(
        body,
        "chatos_mcp_runtime_session_cache_hits_total",
        "MCP Runtime Session cache hits.",
        stats.cache_hits_total,
    );
    append_counter(
        body,
        "chatos_mcp_runtime_session_cache_misses_total",
        "MCP Runtime Session cache misses.",
        stats.cache_misses_total,
    );
    append_counter(
        body,
        "chatos_mcp_runtime_session_cache_capacity_evictions_total",
        "MCP Runtime Session cache evictions caused by entry or byte capacity.",
        stats.cache_capacity_evictions_total,
    );
    append_counter(
        body,
        "chatos_mcp_runtime_session_cache_expired_evictions_total",
        "Expired MCP Runtime Session snapshots removed from the cache.",
        stats.cache_expired_evictions_total,
    );
    append_counter(
        body,
        "chatos_mcp_runtime_session_cache_oversized_rejections_total",
        "MCP Runtime Session snapshots rejected because one snapshot exceeded the byte budget.",
        stats.cache_oversized_rejections_total,
    );
}

fn append_runtime_session_metrics_unavailable(body: &mut String) {
    body.push_str(
        "# HELP chatos_mcp_runtime_session_metrics_available Whether MCP Runtime Session metrics were available for this scrape.\n\
# TYPE chatos_mcp_runtime_session_metrics_available gauge\n\
chatos_mcp_runtime_session_metrics_available{service=\"mcp-management-service\"} 0\n",
    );
}

fn append_runtime_invocation_metrics(body: &mut String, stats: &RuntimeInvocationStoreStats) {
    body.push_str(
        "# HELP chatos_mcp_runtime_invocation_metrics_available Whether MCP Runtime Invocation metrics were available for this scrape.\n\
# TYPE chatos_mcp_runtime_invocation_metrics_available gauge\n\
chatos_mcp_runtime_invocation_metrics_available{service=\"mcp-management-service\"} 1\n",
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_invocations_active",
        "Active MCP Runtime Invocations currently holding quota.",
        stats.total_active,
    );
    body.push_str(
        "# HELP chatos_mcp_runtime_invocation_registration_failures_total MCP Runtime Invocation registration failures by category.\n\
# TYPE chatos_mcp_runtime_invocation_registration_failures_total counter\n",
    );
    for (category, value) in [
        (
            "duplicate_active_id",
            stats.registration.duplicate_active_id,
        ),
        ("capacity_exhausted", stats.registration.capacity_exhausted),
        ("store_unavailable", stats.registration.store_unavailable),
        ("session_closed", stats.registration.session_closed),
        ("invalid_record", stats.registration.invalid_record),
    ] {
        let _ = writeln!(
            body,
            "chatos_mcp_runtime_invocation_registration_failures_total{{service=\"mcp-management-service\",category=\"{category}\"}} {value}"
        );
    }
    append_counter(
        body,
        "chatos_mcp_runtime_invocation_session_closed_reclaimed_total",
        "Active MCP Runtime Invocations reclaimed when their Runtime Session closed.",
        stats.session_closed_reclaimed_total,
    );
    append_counter(
        body,
        "chatos_mcp_runtime_invocation_quota_release_failures_total",
        "MCP Runtime Invocation quota release failures.",
        stats.quota_release_failures_total,
    );
    append_counter(
        body,
        "chatos_mcp_runtime_invocation_store_recoveries_total",
        "Observed successful registrations after an MCP Runtime Invocation store failure.",
        stats.store_recoveries_total,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_invocation_duration_completed",
        "Retained terminal MCP Runtime Invocations contributing duration evidence.",
        stats.duration.completed_count,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_invocation_duration_ms_total",
        "Total duration in milliseconds across retained terminal MCP Runtime Invocations.",
        stats.duration.total_ms,
    );
    append_gauge(
        body,
        "chatos_mcp_runtime_invocation_duration_ms_max",
        "Maximum duration in milliseconds across retained terminal MCP Runtime Invocations.",
        stats.duration.max_ms,
    );
}

fn append_runtime_invocation_metrics_unavailable(body: &mut String) {
    body.push_str(
        "# HELP chatos_mcp_runtime_invocation_metrics_available Whether MCP Runtime Invocation metrics were available for this scrape.\n\
# TYPE chatos_mcp_runtime_invocation_metrics_available gauge\n\
chatos_mcp_runtime_invocation_metrics_available{service=\"mcp-management-service\"} 0\n",
    );
}

fn append_gauge(body: &mut String, name: &str, help: &str, value: impl std::fmt::Display) {
    let _ = writeln!(body, "# HELP {name} {help}");
    let _ = writeln!(body, "# TYPE {name} gauge");
    let _ = writeln!(body, "{name}{{service=\"mcp-management-service\"}} {value}");
}

fn append_counter(body: &mut String, name: &str, help: &str, value: u64) {
    let _ = writeln!(body, "# HELP {name} {help}");
    let _ = writeln!(body, "# TYPE {name} counter");
    let _ = writeln!(body, "{name}{{service=\"mcp-management-service\"}} {value}");
}
