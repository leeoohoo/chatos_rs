// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod config;
mod registration;
mod release_storage;
mod router;
mod service_status;
mod site_manifest;

use config::{load_official_website_dotenv, AppConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_official_website_dotenv();
    init_tracing();

    chatos_service_runtime::apply_config_center_env("official-website").await;
    let config = AppConfig::from_env()?;
    let bind_addr = config.bind_addr();
    let app = router::build_router(config.clone());
    let _service_runtime = chatos_service_runtime::register_current_service(
        "official-website",
        config.port,
        "/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        "official_website_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("official_website_service_backend=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
