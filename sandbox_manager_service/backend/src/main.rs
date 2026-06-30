use tracing_subscriber::EnvFilter;

use sandbox_manager_service_backend::{
    build_router, load_sandbox_manager_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_sandbox_manager_dotenv();
    init_tracing();

    let config = AppConfig::from_env()?;
    tracing::info!("sandbox backend selected: {}", config.backend.as_str());
    let bind_addr = config.bind_addr();
    let state = AppState::new(config.clone()).await?;
    let cleanup_handle = state.spawn_cleanup_worker();
    let app = build_router(state);
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
