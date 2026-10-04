// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
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
    pub dispatch_backend: &'static str,
    pub ready_events: u64,
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
    let ready_events = if config.plugin_catalog_sync_enabled {
        state
            .store
            .plugin_catalog_sync_backlog()
            .await
            .map_err(|error| {
                ApiError::internal(format!(
                    "load Plugin Catalog database backlog failed: {error}"
                ))
            })?
    } else {
        0
    };
    let pressure_level = state.pressure.snapshot().level;

    Ok(Json(PluginManagementSystemStatsResponse {
        ok: true,
        plugin_catalog: PluginCatalogSystemStats {
            enabled: config.plugin_catalog_sync_enabled,
            consumer_concurrency: config.plugin_catalog_consumer_concurrency,
            dispatch_backend: "postgres",
            ready_events,
            pressure_level,
            scheduled_sync_pressure_paused: pressure_level == PlatformPressureLevel::Critical,
        },
    }))
}

pub(super) async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let pressure_level = state.pressure.snapshot().level;
    let mut body = String::new();
    body.push_str(&chatos_postgres::render_pool_metrics(
        state.store.pool(),
        "plugin-management",
    ));
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
