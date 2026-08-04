// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, SocketAddr};

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

#[cfg(feature = "test-support")]
pub use api::message_task_runner::{
    prepare_plugin_artifact_relay_request_for_test,
    validate_plugin_artifact_list_response_for_test,
    validate_plugin_artifact_read_response_for_test,
    validate_plugin_artifact_write_response_for_test, PreparedPluginArtifactRelayRequest,
};

use crate::services::terminal_manager::get_terminal_manager;

pub async fn run_server_from_env() -> Result<(), String> {
    dotenvy::dotenv().ok();

    // jsonwebtoken 10 no longer selects a process-wide crypto backend when
    // dependency feature unification makes the choice ambiguous. Install the
    // backend explicitly before any request can create or verify a token.
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();

    chatos_service_runtime::apply_config_center_env("chatos-backend")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let cfg = config::Config::init_global()?;
    logger::init_logger(cfg).map_err(|err| format!("Failed to init logger: {err}"))?;

    if let Err(err) = modules::app_startup::initialize_runtime(cfg).await {
        error!("{err}");
        return Err(err);
    }

    let _service_runtime =
        chatos_service_runtime::register_current_service("chatos-backend", cfg.port, "/health")
            .await;

    let app = api::router().map_err(|err| format!("Failed to build API router: {err}"))?;

    let host = cfg
        .host
        .parse::<IpAddr>()
        .map_err(|err| format!("Invalid HOST value '{}': {}", cfg.host, err))?;
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
