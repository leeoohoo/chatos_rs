// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod api;
mod bootstrap;
mod config;
mod domain;
mod drivers;
mod error;
mod repository;
mod service;
mod state;

use config::Config;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    init_tracing();

    chatos_service_runtime::apply_config_center_env("db-connection-hub").await;
    let config = Config::from_env();
    let app_state = bootstrap::build_app_state().await?;
    let app = api::router::build_router(app_state);

    let _service_runtime = chatos_service_runtime::register_current_service(
        "db-connection-hub",
        config.port,
        "/api/v1/health",
    )
    .await;
    let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;
    tracing::info!(host = %config.host, port = config.port, "db_connection_hub backend started");

    axum::serve(listener, app).await
}

fn init_tracing() {
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
