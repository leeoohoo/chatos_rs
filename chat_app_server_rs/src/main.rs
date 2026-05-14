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

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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

    if let Err(err) = modules::app_startup::initialize_runtime(cfg).await {
        error!("{err}");
        std::process::exit(1);
    }

    let app = match api::router() {
        Ok(app) => app,
        Err(err) => {
            error!("Failed to build API router: {err}");
            std::process::exit(1);
        }
    };

    let host = match cfg.host.parse::<IpAddr>() {
        Ok(host) => host,
        Err(err) => {
            warn!(
                "Invalid HOST value '{}': {}. Falling back to 0.0.0.0",
                cfg.host, err
            );
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        }
    };
    let addr = SocketAddr::new(host, cfg.port);
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
