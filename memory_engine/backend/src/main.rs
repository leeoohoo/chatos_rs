// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod ai;
mod api;
mod cloud_agent_queue;
mod config;
mod db;
mod internal_tls;
mod jobs;
mod models;
mod pressure;
mod repositories;
mod rollup_queue;
mod services;
mod state;
mod subject_memory_queue;
mod summary_queue;

use std::sync::Arc;
use std::time::Duration;

use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{info, Level};

use crate::config::AppConfig;
use crate::internal_tls::{load_internal_mtls_config, MemoryEngineInternalTlsConfig};
use crate::state::{AppState, MemoryEngineRuntimeStats};

#[tokio::main]
async fn main() -> Result<(), String> {
    chatos_service_runtime::load_service_dotenv(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "memory_engine=info,axum=info".into()),
        )
        .init();

    chatos_service_runtime::apply_config_center_env("memory-engine")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let pressure_config_client = chatos_config_sdk::ConfigClient::from_env("memory-engine")
        .map_err(|err| format!("build Memory Engine pressure config client failed: {err}"))?;
    let pressure_snapshot = pressure_config_client
        .load_strict()
        .await
        .map_err(|err| format!("load Memory Engine pressure config failed: {err}"))?;
    repositories::control_plane::initialize_managed_memory_policy().await;
    let mut config = AppConfig::from_env()?;
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    let pool = db::init_pool(&config).await?;
    db::init_schema(&pool).await?;
    let user_service_http = build_http_client(HttpClientTimeouts::new(Duration::from_millis(
        config.user_service_request_timeout_ms.max(300),
    )))
    .map_err(|err| format!("build user_service client failed: {err}"))?;
    let rabbitmq_queue_inspector =
        chatos_queue_observability::RabbitMqQueueInspector::new(config.rabbitmq_url.clone())?;
    let pressure_policy = pressure::MemoryEnginePressurePolicy::from_snapshot(
        &pressure_snapshot,
        config.worker_summary_concurrency,
    )?;
    let cloud_agent_store = chatos_cloud_agent_runtime::CloudAgentStateStore::connect_to_database(
        config.mongodb_uri.as_str(),
        config.mongodb_database.as_str(),
    )
    .await?;

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        user_service_http,
        runtime_stats: Arc::new(MemoryEngineRuntimeStats::default()),
        rabbitmq_queue_inspector,
        pressure: pressure::MemoryEnginePressureState::new(pressure_policy),
        cloud_agent_store,
    });
    pressure::start_config_watcher(state.clone(), pressure_config_client.clone());
    let service_id = std::env::var("CHATOS_SERVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("memory-engine-{}", std::process::id()));
    let running_version = std::env::var("CHATOS_SERVICE_VERSION").ok();
    pressure::start_pressure_reporter(
        state.clone(),
        pressure_config_client,
        service_id,
        running_version,
    );

    if config.worker_enabled {
        cloud_agent_queue::start(state.clone());
        rollup_queue::start(state.clone());
        subject_memory_queue::start(state.clone());
        jobs::worker::start(state.clone());
    }

    if !config.api_enabled {
        info!("[MEMORY-ENGINE] running without HTTP API listener");
        tokio::signal::ctrl_c()
            .await
            .map_err(|err| format!("wait for shutdown signal failed: {err}"))?;
        return Ok(());
    }

    let public_app = api::build_public_router(state.clone())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::DEBUG))
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        );
    let internal_app = api::build_internal_router(state).layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::DEBUG))
            .on_request(DefaultOnRequest::new().level(Level::DEBUG))
            .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
    );

    let addr = format!("{}:{}", config.host, config.port);
    let internal_tls = MemoryEngineInternalTlsConfig::from_env(
        config
            .host
            .parse()
            .map_err(|err| format!("MEMORY_ENGINE_HOST must be a valid IP address: {err}"))?,
        config.port,
    )?;
    let internal_mtls_config = load_internal_mtls_config(&internal_tls)?;
    let _service_runtime =
        chatos_service_runtime::register_current_service("memory-engine", config.port, "/health")
            .await;
    let listener = TcpListener::bind(addr.as_str())
        .await
        .map_err(|err| format!("bind failed: {err}"))?;

    info!("[MEMORY-ENGINE] public API listening on http://{}", addr);
    info!(
        "[MEMORY-ENGINE] internal control plane listening with mandatory mTLS on https://{}",
        internal_tls.bind_addr
    );

    tokio::select! {
        result = axum::serve(listener, public_app) => {
            result.map_err(|err| format!("public server error: {err}"))?;
        }
        result = axum_server::bind_rustls(internal_tls.bind_addr, internal_mtls_config)
            .serve(internal_app.into_make_service()) => {
            result.map_err(|err| format!("internal mTLS server error: {err}"))?;
        }
    }
    Ok(())
}
