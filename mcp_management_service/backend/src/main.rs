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

    chatos_service_runtime::apply_config_center_env("mcp-management-service")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let mut config = AppConfig::from_env()?;
    config.resolve_service_urls().await;
    let bind_addr = config.bind_addr();
    let app_state = AppState::new(config.clone()).await?;
    tracing::info!(
        async_tool_dispatch_mode = app_state.config.async_tool_dispatch_topology.mode.as_str(),
        async_tool_worker_concurrency = app_state
            .config
            .async_tool_dispatch_topology
            .worker_concurrency,
        async_tool_local_queue_buffer = app_state
            .config
            .async_tool_dispatch_topology
            .local_queue_buffer,
        async_tool_rabbitmq_exchange = app_state
            .config
            .async_tool_dispatch_topology
            .rabbitmq_exchange
            .as_deref()
            .unwrap_or(""),
        async_tool_dispatch_queue = app_state
            .config
            .async_tool_dispatch_topology
            .queue_name
            .as_deref()
            .unwrap_or(""),
        "mcp management async tool dispatch topology configured"
    );
    let mut background_handles = Vec::new();
    if let Some(handle) = app_state
        .async_tool_dispatch
        .spawn_rabbitmq_consumer(app_state.clone())
    {
        background_handles.push(handle);
    }
    let app = build_router(app_state);
    let _runtime = chatos_service_runtime::register_current_service(
        "mcp-management-service",
        config.port,
        "/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("MCP management service listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    for handle in background_handles {
        handle.abort();
    }
    Ok(())
}
