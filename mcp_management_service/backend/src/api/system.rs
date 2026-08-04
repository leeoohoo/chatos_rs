// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::auth::require_internal_request;
use crate::config::AsyncToolDispatchMode;
use crate::error::ApiError;
use crate::runtime::{RuntimeInvocationStoreStats, RuntimeSessionStoreStats};
use crate::state::AppState;

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
    rabbitmq_enabled: bool,
    rabbitmq_exchange: Option<String>,
    dispatch_queue: Option<String>,
}

pub(super) async fn system_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SystemStatsResponse>, ApiError> {
    require_internal_request(&state.config, &headers, "system.stats.read")?;
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
            rabbitmq_enabled: topology.mode == AsyncToolDispatchMode::RabbitMq,
            rabbitmq_exchange: topology.rabbitmq_exchange.clone(),
            dispatch_queue: topology.queue_name.clone(),
        },
        runtime_sessions,
        runtime_invocations,
    }))
}
