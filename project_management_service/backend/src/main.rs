use tracing_subscriber::EnvFilter;

use project_management_service_backend::{
    build_router, load_project_service_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_project_service_dotenv();
    init_tracing();

    let config = AppConfig::from_env()?;
    let bind_addr = config.bind_addr();
    let state = AppState::new(config.clone()).await?;
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "project_management_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("project_management_service_backend=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
