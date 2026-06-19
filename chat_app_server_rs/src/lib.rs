use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::signal;
use tracing::{error, info, warn};

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

pub mod shared_runtime;

use crate::services::terminal_manager::get_terminal_manager;

pub async fn run_server_from_env() -> Result<(), String> {
    dotenvy::dotenv().ok();

    let cfg = config::Config::init_global()?;
    logger::init_logger(cfg).map_err(|err| format!("Failed to init logger: {err}"))?;

    if let Err(err) = modules::app_startup::initialize_runtime(cfg).await {
        error!("{err}");
        return Err(err);
    }

    let app = api::router().map_err(|err| format!("Failed to build API router: {err}"))?;

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

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("Failed to bind: {err}"))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| format!("Server error: {err}"))
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate = match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(err) => {
                warn!("Failed to listen for SIGTERM: {}", err);
                let _ = signal::ctrl_c().await;
                info!("Shutdown signal received via Ctrl+C");
                let manager = get_terminal_manager();
                if let Err(err) = manager.shutdown_all_project_run_terminals().await {
                    warn!("Failed to shutdown project run terminals cleanly: {}", err);
                }
                return;
            }
        };
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Shutdown signal received via Ctrl+C");
            }
            _ = terminate.recv() => {
                info!("Shutdown signal received via SIGTERM");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
        info!("Shutdown signal received via Ctrl+C");
    }

    let manager = get_terminal_manager();
    if let Err(err) = manager.shutdown_all_project_run_terminals().await {
        warn!("Failed to shutdown project run terminals cleanly: {}", err);
    }
}
