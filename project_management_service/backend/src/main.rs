// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use project_management_service_backend::{
    build_router, load_project_service_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_project_service_dotenv();
    init_tracing();

    chatos_service_runtime::apply_config_center_env("project-service").await;
    let mut config = AppConfig::from_env()?;
    resolve_downstream_services(&mut config).await;
    let bind_addr = config.bind_addr();
    let state = AppState::new(config.clone()).await?;
    let app = build_router(state);
    let _service_runtime = chatos_service_runtime::register_current_service(
        "project-service",
        config.port,
        "/api/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "project_management_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    axum::serve(listener, app).await?;
    Ok(())
}

async fn resolve_downstream_services(config: &mut AppConfig) {
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    if let Some(base_url) = config.task_runner_base_url.clone() {
        config.task_runner_base_url = Some(
            chatos_service_runtime::resolve_service_base_url("task-runner", base_url.as_str())
                .await,
        );
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("project_management_service_backend=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
