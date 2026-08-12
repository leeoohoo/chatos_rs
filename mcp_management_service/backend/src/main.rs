// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mcp_management_service_backend::{
    build_internal_router, build_public_router, load_internal_mtls_config,
    load_mcp_management_dotenv, AppConfig, AppState,
};
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_mcp_management_dotenv();

    chatos_service_runtime::apply_config_center_env("mcp-management-service")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let pressure_config_client =
        chatos_config_sdk::ConfigClient::from_env("mcp-management-service")
            .map_err(|err| format!("build MCP Management pressure config client failed: {err}"))?;
    let pressure_snapshot = pressure_config_client
        .load_strict()
        .await
        .map_err(|err| format!("load MCP Management pressure config failed: {err}"))?;
    let pressure_policy =
        mcp_management_service_backend::pressure::McpManagementPressurePolicy::from_snapshot(
            &pressure_snapshot,
        )?;
    let config = AppConfig::from_env()?;
    let _telemetry = init_tracing(&config)?;
    let bind_addr = config.bind_addr();
    let internal_mtls_bind_addr = config.internal_mtls_bind_addr();
    let internal_mtls_config = load_internal_mtls_config(&config)?;
    let app_state = AppState::new(config.clone()).await?;
    tracing::info!(
        async_tool_dispatch_mode = app_state.config.async_tool_dispatch_topology.mode.as_str(),
        async_tool_worker_concurrency = app_state
            .config
            .async_tool_dispatch_topology
            .worker_concurrency,
        async_tool_max_delivery_attempts = app_state
            .config
            .async_tool_dispatch_topology
            .max_delivery_attempts,
        async_tool_retry_delay_ms = app_state
            .config
            .async_tool_dispatch_topology
            .retry_delay
            .as_millis(),
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
        async_tool_retry_queue = app_state
            .config
            .async_tool_dispatch_topology
            .retry_queue_name
            .as_deref()
            .unwrap_or(""),
        async_tool_dead_letter_queue = app_state
            .config
            .async_tool_dispatch_topology
            .dead_letter_queue_name
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
    if let Some(handle) = app_state
        .async_tool_dispatch
        .spawn_cancellation_consumer(app_state.clone())
    {
        background_handles.push(handle);
    }
    let service_id = std::env::var("CHATOS_SERVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("mcp-management-service-{}", std::process::id()));
    let running_version = std::env::var("CHATOS_SERVICE_VERSION").ok();
    background_handles.push(
        mcp_management_service_backend::pressure::start_pressure_reporter(
            app_state.clone(),
            pressure_config_client,
            pressure_policy,
            service_id,
            running_version,
        ),
    );
    let public_app = build_public_router(app_state.clone());
    let internal_app = build_internal_router(app_state);
    let _runtime = chatos_service_runtime::register_current_service(
        "mcp-management-service",
        config.port,
        "/health",
    )
    .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("MCP management service listening on http://{bind_addr}");
    tracing::info!(
        "MCP management internal API listening with mandatory mTLS on https://{internal_mtls_bind_addr}"
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
    for handle in background_handles {
        handle.abort();
    }
    Ok(())
}

struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        let _ = self.tracer_provider.shutdown();
    }
}

fn init_tracing(config: &AppConfig) -> Result<TelemetryGuard, String> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mcp_management_service_backend=info,tower_http=info"));
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.otlp_endpoint.clone())
        .with_timeout(config.otlp_export_timeout)
        .build()
        .map_err(|err| format!("build MCP Management OTLP trace exporter failed: {err}"))?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_service_name("mcp-management-service")
                .build(),
        )
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            config.otlp_trace_sample_ratio,
        ))))
        .with_batch_exporter(trace_exporter)
        .build();
    let tracer = tracer_provider.tracer("mcp-management-service");
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    global::set_text_map_propagator(TraceContextPropagator::new());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry_layer)
        .try_init()
        .map_err(|err| format!("initialize MCP Management tracing subscriber failed: {err}"))?;

    Ok(TelemetryGuard { tracer_provider })
}
