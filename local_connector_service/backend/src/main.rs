// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use local_connector_service_backend::{
    build_router, load_local_connector_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_local_connector_dotenv();
    init_tracing();

    chatos_service_runtime::apply_config_center_env("local-connector-service")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let pressure_config_client =
        chatos_config_sdk::ConfigClient::from_env("local-connector-service")
            .map_err(|err| format!("build Local Connector pressure config client failed: {err}"))?;
    let pressure_snapshot = pressure_config_client
        .load_strict()
        .await
        .map_err(|err| format!("load Local Connector pressure config failed: {err}"))?;
    let pressure_policy =
        local_connector_service_backend::pressure::LocalConnectorPressurePolicy::from_snapshot(
            &pressure_snapshot,
        )?;
    let pressure_state =
        local_connector_service_backend::pressure::LocalConnectorPressureState::new(
            pressure_policy,
        );
    let mut config = AppConfig::from_env()?;
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    let bind_addr = config.bind_addr();
    let state = AppState::new(config.clone(), pressure_state).await?;
    let service_id = std::env::var("CHATOS_SERVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("local-connector-service-{}", std::process::id()));
    let running_version = std::env::var("CHATOS_SERVICE_VERSION").ok();
    let _pressure_reporter = local_connector_service_backend::pressure::start_pressure_reporter(
        state.clone(),
        pressure_config_client,
        service_id,
        running_version,
    );
    let app = build_router(state);
    let _service_runtime = chatos_service_runtime::register_current_service(
        "local-connector-service",
        config.port,
        "/api/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "local_connector_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("local_connector_service_backend=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
