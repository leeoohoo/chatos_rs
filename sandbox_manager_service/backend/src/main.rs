// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use sandbox_manager_service_backend::{
    build_internal_router, build_public_router, load_internal_mtls_config,
    load_sandbox_manager_dotenv, AppConfig, AppState, SandboxManagerInternalTlsConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_sandbox_manager_dotenv();
    init_tracing();

    chatos_service_runtime::apply_config_center_env("sandbox-manager")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let mut config = AppConfig::from_env()?;
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    tracing::info!("sandbox backend selected: {}", config.backend.as_str());
    let bind_addr = config.bind_addr();
    let internal_tls = SandboxManagerInternalTlsConfig::from_env(config.host, config.port)?;
    let internal_mtls_config = load_internal_mtls_config(&internal_tls)?;
    let state = AppState::new(config.clone()).await?;
    let maintenance_config = config.clone();
    tokio::spawn(async move {
        match sandbox_manager_service_backend::docker_maintenance::enforce_build_cache_limit(
            &maintenance_config,
        )
        .await
        {
            Ok(message) => tracing::info!("startup {message}"),
            Err(error) => tracing::warn!("startup Docker maintenance failed: {error}"),
        }
    });
    let cleanup_handle = state.spawn_cleanup_worker();
    let public_app = build_public_router(state.clone());
    let internal_app = build_internal_router(state);
    let _service_runtime =
        chatos_service_runtime::register_current_service("sandbox-manager", config.port, "/health")
            .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "sandbox_manager_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );
    tracing::info!(
        "sandbox_manager_service_backend internal mTLS listening on https://{}",
        internal_tls.bind_addr
    );

    tokio::select! {
        result = axum::serve(listener, public_app) => {
            result?;
        }
        result = axum_server::bind_rustls(internal_tls.bind_addr, internal_mtls_config)
            .serve(internal_app.into_make_service()) => {
            result?;
        }
    }
    cleanup_handle.abort();
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sandbox_manager_service_backend=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
