// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use user_service_backend::{
    build_internal_router, build_public_router,
    internal_tls::{load_internal_mtls_config, UserServiceInternalTlsConfig},
    load_user_service_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_user_service_dotenv();
    init_tracing();

    chatos_service_runtime::apply_config_center_env("user-service")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let mut config = AppConfig::from_env()?;
    resolve_downstream_services(&mut config).await;
    let bind_addr = config.bind_addr();
    let internal_tls = UserServiceInternalTlsConfig::from_env(config.host, config.port)?;
    let internal_mtls_config = load_internal_mtls_config(&internal_tls)?;
    let state = AppState::new(config.clone()).await?;
    let public_app = build_public_router(state.clone());
    let internal_app = build_internal_router(state);
    let _service_runtime = chatos_service_runtime::register_current_service(
        "user-service",
        config.port,
        "/api/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "user_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    tracing::info!(
        "User Service internal API listening with mandatory mTLS on https://{}",
        internal_tls.bind_addr
    );

    tokio::select! {
        result = axum::serve(
            listener,
            public_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        ) => {
            result?;
        }
        result = axum_server::bind_rustls(internal_tls.bind_addr, internal_mtls_config)
            .serve(internal_app.into_make_service()) => {
            result?;
        }
    }
    Ok(())
}

async fn resolve_downstream_services(config: &mut AppConfig) {
    if let Some(base_url) = config.harness_base_url.clone() {
        config.harness_base_url = Some(
            chatos_service_runtime::resolve_service_base_url("harness", base_url.as_str()).await,
        );
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("user_service_backend=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
