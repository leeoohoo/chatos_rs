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
mod internal_tls;
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
use internal_tls::{load_internal_mtls_config, ChatosInternalTlsConfig};

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

    chatos_mcp_runtime::initialize_mcp_invocation_result_queue(
        chatos_mcp_runtime::McpInvocationResultQueueConfig {
            rabbitmq_url: cfg.mcp_result_rabbitmq_url.clone(),
            queue_name: format!(
                "{}.{}",
                cfg.mcp_result_queue_prefix,
                mcp_result_queue_instance_component(cfg.host.as_str(), cfg.port)
            ),
        },
    )
    .await
    .map_err(|error| format!("initialize Chatos MCP result queue failed: {error}"))?;

    if let Err(err) = modules::app_startup::initialize_runtime(cfg).await {
        error!("{err}");
        return Err(err);
    }

    let _service_runtime =
        chatos_service_runtime::register_current_service("chatos-backend", cfg.port, "/health")
            .await;

    let public_app =
        api::public_router().map_err(|err| format!("Failed to build public API router: {err}"))?;
    let internal_app = api::internal_router();

    let host = cfg
        .host
        .parse::<IpAddr>()
        .map_err(|err| format!("Invalid HOST value '{}': {}", cfg.host, err))?;
    let addr = SocketAddr::new(host, cfg.port);
    let internal_tls = ChatosInternalTlsConfig::from_config(host, cfg)?;
    let internal_mtls_config = load_internal_mtls_config(&internal_tls)?;
    info!("Server running on http://{}", addr);
    info!(
        "ChatOS internal API listening with mandatory mTLS on https://{}",
        internal_tls.bind_addr
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("Failed to bind: {err}"))?;

    tokio::select! {
        result = axum::serve(listener, public_app).with_graceful_shutdown(shutdown_signal()) => {
            result.map_err(|err| format!("Public server error: {err}"))?;
        }
        result = axum_server::bind_rustls(internal_tls.bind_addr, internal_mtls_config)
            .serve(internal_app.into_make_service()) => {
            result.map_err(|err| format!("Internal mTLS server error: {err}"))?;
        }
    }
    logger::shutdown_telemetry()?;
    Ok(())
}

fn mcp_result_queue_instance_component(host: &str, port: u16) -> String {
    format!("{host}-{port}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
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
