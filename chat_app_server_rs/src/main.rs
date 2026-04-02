mod api;
mod builtin;
mod config;
mod core;
mod db;
mod logger;
mod models;
mod modules;
mod repositories;
mod services;
mod utils;

use std::net::SocketAddr;

use tokio::signal;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let cfg = match config::Config::init_global() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Failed to load config: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = logger::init_logger(cfg) {
        eprintln!("Failed to init logger: {err}");
        std::process::exit(1);
    }

    if let Err(err) =
        core::remote_connection_error_codes::export_remote_connection_error_code_catalog_doc()
    {
        warn!("Failed to export remote connection error code catalog: {err}");
    }

    // Initialize DB
    if let Err(err) = db::init_global().await {
        error!("Failed to init database: {err}");
        std::process::exit(1);
    }

    services::task_execution_runner::start();
    info!("Memory-only mode enabled, skip local session background jobs");

    cfg.print();

    let app = api::router();

    let addr = SocketAddr::new(
        cfg.host
            .parse()
            .unwrap_or_else(|_| "0.0.0.0".parse().unwrap()),
        cfg.port,
    );
    info!("Server running on http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            error!("Failed to bind: {err}");
            std::process::exit(1);
        }
    };

    let server = axum::serve(listener, app);

    if let Err(err) = server.with_graceful_shutdown(shutdown_signal()).await {
        error!("Server error: {err}");
    }
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
    info!("Shutdown signal received");
}
