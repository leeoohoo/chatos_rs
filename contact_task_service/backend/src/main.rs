mod api;
mod auth;
mod config;
mod db;
mod models;
mod repository;

use std::sync::Arc;

use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: db::Db,
    pub config: config::AppConfig,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "contact_task_service=info,axum=info".into()),
        )
        .init();

    let config = config::AppConfig::from_env();
    let db = db::init_pool(&config).await?;
    db::init_schema(&db).await?;

    let state = Arc::new(AppState {
        db,
        config: config.clone(),
    });
    let app = api::router(state).layer(TraceLayer::new_for_http()).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_headers(Any)
            .allow_methods(Any),
    );

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(addr.as_str())
        .await
        .map_err(|e| format!("bind failed: {e}"))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server error: {e}"))
}
