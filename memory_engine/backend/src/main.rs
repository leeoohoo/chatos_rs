// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod ai;
mod api;
mod config;
mod db;
mod jobs;
mod models;
mod repositories;
mod services;
mod state;

use std::sync::Arc;
use std::time::Duration;

use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{info, Level};

use crate::config::AppConfig;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), String> {
    chatos_service_runtime::load_service_dotenv(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "memory_engine=info,axum=info".into()),
        )
        .init();

    chatos_service_runtime::apply_config_center_env("memory-engine").await;
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

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        user_service_http,
    });

    if config.worker_enabled {
        jobs::worker::start(state.clone());
    }

    let app = api::router(state)
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

    let addr = format!("{}:{}", config.host, config.port);
    let _service_runtime =
        chatos_service_runtime::register_current_service("memory-engine", config.port, "/health")
            .await;
    let listener = TcpListener::bind(addr.as_str())
        .await
        .map_err(|err| format!("bind failed: {err}"))?;

    info!("[MEMORY-ENGINE] listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|err| format!("server error: {err}"))
}
