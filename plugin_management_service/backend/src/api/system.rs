// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
use chatos_queue_observability::{RabbitMqQueueRuntimeStats, RabbitMqQueueSpec};
use serde::Serialize;

use super::{
    require_internal_api_secret, require_internal_caller_service, ApiError, SYSTEM_STATS_READ_SCOPE,
};
use crate::pressure::PlatformPressureLevel;
use crate::state::AppState;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

#[derive(Debug, Serialize)]
pub(super) struct PluginManagementSystemStatsResponse {
    pub ok: bool,
    pub plugin_catalog: PluginCatalogSystemStats,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginCatalogSystemStats {
    pub enabled: bool,
    pub consumer_concurrency: usize,
    pub queue: String,
    pub retry_queue: String,
    pub schedule_queue: String,
    pub dead_letter_queue: String,
    pub rabbitmq_queues: RabbitMqQueueRuntimeStats,
    pub pressure_level: PlatformPressureLevel,
    pub scheduled_sync_pressure_paused: bool,
}

pub(super) async fn get_system_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PluginManagementSystemStatsResponse>, ApiError> {
    let caller_service = require_internal_caller_service(&headers)?;
    require_internal_api_secret(&state, &headers, caller_service, SYSTEM_STATS_READ_SCOPE)?;

    let config = &state.config;
    let rabbitmq_queues = rabbitmq_queue_stats(&state).await;
    let pressure_level = state.pressure.snapshot().level;

    Ok(Json(PluginManagementSystemStatsResponse {
        ok: true,
        plugin_catalog: PluginCatalogSystemStats {
            enabled: config.plugin_catalog_sync_enabled,
            consumer_concurrency: config.plugin_catalog_consumer_concurrency,
            queue: config.plugin_catalog_queue.clone(),
            retry_queue: config.plugin_catalog_retry_queue.clone(),
            schedule_queue: config.plugin_catalog_schedule_queue.clone(),
            dead_letter_queue: config.plugin_catalog_dead_letter_queue.clone(),
            rabbitmq_queues,
            pressure_level,
            scheduled_sync_pressure_paused: pressure_level == PlatformPressureLevel::Critical,
        },
    }))
}

pub(super) async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let stats = rabbitmq_queue_stats(&state).await;
    let pressure_level = state.pressure.snapshot().level;
    let mut body =
        chatos_queue_observability::render_prometheus_metrics("plugin-management-service", &stats);
    body.push_str(
        "# HELP chatos_plugin_management_scheduled_sync_pressure_paused Whether scheduled Catalog sync is deferred by critical platform pressure.\n\
# TYPE chatos_plugin_management_scheduled_sync_pressure_paused gauge\n",
    );
    body.push_str(&format!(
        "chatos_plugin_management_scheduled_sync_pressure_paused {}\n",
        u8::from(pressure_level == PlatformPressureLevel::Critical)
    ));
    ([(header::CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)], body)
}

async fn rabbitmq_queue_stats(state: &AppState) -> RabbitMqQueueRuntimeStats {
    let config = &state.config;
    if !config.plugin_catalog_sync_enabled {
        return RabbitMqQueueRuntimeStats::disabled();
    }
    state
        .rabbitmq_queue_inspector
        .inspect(&[
            RabbitMqQueueSpec::new("catalog_sync", config.plugin_catalog_queue.as_str()),
            RabbitMqQueueSpec::new("catalog_retry", config.plugin_catalog_retry_queue.as_str()),
            RabbitMqQueueSpec::new(
                "catalog_schedule",
                config.plugin_catalog_schedule_queue.as_str(),
            ),
            RabbitMqQueueSpec::new(
                "catalog_dead_letter",
                config.plugin_catalog_dead_letter_queue.as_str(),
            ),
        ])
        .await
}
