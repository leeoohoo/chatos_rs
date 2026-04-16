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

    let config = Config::from_env();
    let app_state = bootstrap::build_app_state().await?;
    let app = api::router::build_router(app_state);

    let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;
    tracing::info!(host = %config.host, port = config.port, "db_connection_hub backend started");

    axum::serve(listener, app).await
}

fn init_tracing() {
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
