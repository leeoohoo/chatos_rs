// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use project_management_service_backend::{
    build_internal_router, build_public_router,
    internal_tls::{load_internal_mtls_config, ProjectServiceInternalTlsConfig},
    load_project_service_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_project_service_dotenv();
    init_tracing();

    chatos_service_runtime::apply_config_center_env("project-service")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let mut config = AppConfig::from_env()?;
    resolve_downstream_services(&mut config).await;
    chatos_mcp_runtime::initialize_mcp_invocation_result_queue(
        chatos_mcp_runtime::McpInvocationResultQueueConfig {
            rabbitmq_url: config.mcp_result_rabbitmq_url.clone(),
            queue_name: format!(
                "{}.{}",
                config.mcp_result_queue_prefix,
                mcp_result_queue_instance_component(config.host, config.port)
            ),
        },
    )
    .await
    .map_err(|error| format!("initialize Project Service MCP result queue failed: {error}"))?;
    let bind_addr = config.bind_addr();
    let internal_tls = ProjectServiceInternalTlsConfig::from_env(config.host, config.port)?;
    let internal_mtls_config = load_internal_mtls_config(&internal_tls)?;
    let state = AppState::new(config.clone()).await?;
    let public_app = build_public_router(state.clone());
    let internal_app = build_internal_router(state);
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

    tracing::info!(
        "Project Service internal API listening with mandatory mTLS on https://{}",
        internal_tls.bind_addr
    );

    tokio::select! {
        result = axum::serve(listener, public_app) => {
            result?;
        }
        result = axum_server::bind_rustls(internal_tls.bind_addr, internal_mtls_config)
            .serve(internal_app.into_make_service()) => {
            result?;
        }
    }
    Ok(())
}

fn mcp_result_queue_instance_component(host: std::net::IpAddr, port: u16) -> String {
    format!("{host}-{port}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

async fn resolve_downstream_services(config: &mut AppConfig) {
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("project_management_service_backend=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
