// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use sandbox_manager_service_backend::{
    build_router, load_sandbox_manager_dotenv, AppConfig, AppState,
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
    let app = build_router(state);
    let _service_runtime =
        chatos_service_runtime::register_current_service("sandbox-manager", config.port, "/health")
            .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "sandbox_manager_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    axum::serve(listener, app).await?;
    cleanup_handle.abort();
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sandbox_manager_service_backend=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
