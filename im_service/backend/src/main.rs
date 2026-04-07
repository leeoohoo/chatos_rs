mod api;
mod config;
mod db;
mod event_hub;
mod models;
mod repositories;
mod state;

use std::sync::Arc;

use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::AppConfig;
use crate::event_hub::ImEventHub;
use crate::repositories::auth;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), String> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "im_service=info,axum=info".into()),
        )
        .init();

    let config = AppConfig::from_env();
    let pool = db::init_pool(&config).await?;
    db::init_schema(&pool).await?;
    let synced_users = auth::sync_users_from_memory(&pool, config.mongodb_uri.as_str()).await?;
    info!("[IM-SERVICE] synced {} users from memory_server", synced_users);
    auth::ensure_default_admin(&pool).await?;

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        event_hub: Arc::new(ImEventHub::new()),
    });

    let app = api::router(state.clone())
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
        .map_err(|e| format!("bind failed: {e}"))?;

    info!("[IM-SERVICE] listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server error: {e}"))
}
