// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use config_center_service_backend::{
    build_internal_router, build_public_router, load_config_center_dotenv,
    load_internal_mtls_config, AppConfig, AppState,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_config_center_dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "config_center_service_backend=info,tower_http=info".into()),
        )
        .init();

    let mut config = AppConfig::from_env()?;
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    let bind_addr = config.bind_addr();
    let internal_mtls_bind_addr = config.internal_mtls_bind_addr();
    let internal_mtls_config = load_internal_mtls_config(&config)?;
    let state = AppState::new(config.clone()).await?;
    config_center_service_backend::state::pressure_controller::start(state.clone()).await?;
    let public_app = build_public_router(state.clone());
    let internal_app = build_internal_router(state);
    let _runtime = chatos_service_runtime::register_current_service(
        "configuration-center",
        config.port,
        "/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("configuration center listening on http://{bind_addr}");
    tracing::info!(
        "configuration center internal API listening with mandatory mTLS on https://{internal_mtls_bind_addr}"
    );
    tokio::select! {
        result = axum::serve(listener, public_app) => {
            result?;
        }
        result = axum_server::bind_rustls(internal_mtls_bind_addr, internal_mtls_config)
            .serve(internal_app.into_make_service()) => {
            result?;
        }
    }
    Ok(())
}
