// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mcp_management_service_backend::{
    build_router, load_mcp_management_dotenv, AppConfig, AppState,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_mcp_management_dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mcp_management_service_backend=info,tower_http=info".into()),
        )
        .init();

    chatos_service_runtime::apply_config_center_env("mcp-management-service").await;
    let mut config = AppConfig::from_env()?;
    config.resolve_service_urls().await;
    let bind_addr = config.bind_addr();
    let app = build_router(AppState::new(config.clone())?);
    let _runtime = chatos_service_runtime::register_current_service(
        "mcp-management-service",
        config.port,
        "/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("MCP management service listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
