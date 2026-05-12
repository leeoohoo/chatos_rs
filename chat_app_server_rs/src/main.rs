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

    match services::auth_user_backfill::backfill_legacy_auth_users().await {
        Ok(report) => {
            info!(
                "Legacy auth-user backfill finished: legacy_count={} created_count={} skipped_existing_count={} skipped_invalid_count={}",
                report.legacy_count,
                report.created_count,
                report.skipped_existing_count,
                report.skipped_invalid_count
            );
        }
        Err(err) => {
            warn!("Legacy auth-user backfill failed: {err}");
        }
    }

    match services::memory_engine_source_bootstrap::ensure_chatos_memory_engine_source().await {
        Ok(report) => {
            info!(
                "Chatos memory_engine source ensured: source_id={} source_type={} status={} sdk_enabled={}",
                report.source_id,
                report.source_type,
                report.status,
                report.sdk_enabled
            );
        }
        Err(err) => {
            warn!("Chatos memory_engine source bootstrap failed: {err}");
        }
    }

    services::workspace_realtime_watcher::start_workspace_realtime_watcher();

    info!("Memory-only mode enabled, skip local session background jobs");

    cfg.print();

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
