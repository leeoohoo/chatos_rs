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

use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::AppConfig;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), String> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "memory_engine=info,axum=info".into()),
        )
        .init();

    let config = AppConfig::from_env();
    let pool = db::init_pool(&config).await?;
    db::init_schema(&pool).await?;

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
    });

    if config.worker_enabled {
        jobs::worker::start(state.clone());
    }

    let app = api::router(state)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        );

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(addr.as_str())
        .await
        .map_err(|err| format!("bind failed: {err}"))?;

    info!("[MEMORY-ENGINE] listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|err| format!("server error: {err}"))
}
