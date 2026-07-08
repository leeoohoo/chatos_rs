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

    chatos_service_runtime::apply_config_center_env("sandbox-manager").await;
    let mut config = AppConfig::from_env()?;
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    tracing::info!("sandbox backend selected: {}", config.backend.as_str());
    let bind_addr = config.bind_addr();
    let state = AppState::new(config.clone()).await?;
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
