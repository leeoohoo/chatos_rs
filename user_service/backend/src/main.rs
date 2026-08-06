// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use user_service_backend::{
    build_internal_router, build_public_router,
    internal_tls::{load_internal_mtls_config, UserServiceInternalTlsConfig},
    load_user_service_dotenv, AppConfig, AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_user_service_dotenv();

    chatos_service_runtime::apply_config_center_env("user-service")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let mut config = AppConfig::from_env()?;
    let _telemetry = init_tracing(&config)?;
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
        .unwrap_or_else(|_| EnvFilter::new("user_service_backend=info,tower_http=info"));
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.otlp_endpoint.clone())
        .with_timeout(config.otlp_export_timeout)
        .build()
        .map_err(|err| format!("build User Service OTLP trace exporter failed: {err}"))?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_service_name("user-service")
                .build(),
        )
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            config.otlp_trace_sample_ratio,
        ))))
        .with_batch_exporter(trace_exporter)
        .build();
    let tracer = tracer_provider.tracer("user-service");
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    global::set_text_map_propagator(TraceContextPropagator::new());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry_layer)
        .try_init()
        .map_err(|err| format!("initialize User Service tracing subscriber failed: {err}"))?;

    Ok(TelemetryGuard { tracer_provider })
}
