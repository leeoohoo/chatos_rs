// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use task_runner_service_backend::{
    build_internal_router, build_public_router, configure_task_terminal_runtime,
    internal_tls::{load_internal_mtls_config, TaskRunnerInternalTlsConfig},
    load_task_runner_dotenv,
    scheduler::spawn_task_scheduler,
    services::{spawn_chatos_callback_queue_consumer, spawn_chatos_callback_reconciler},
    spawn_ask_user_resolution_outbox_reconciler, spawn_run_cancel_outbox_reconciler,
    spawn_run_dispatch_outbox_reconciler, spawn_run_event_consumer, spawn_run_event_retention,
    spawn_run_post_process_consumer, spawn_run_post_process_outbox_reconciler,
    spawn_run_terminal_outbox_reconciler, spawn_task_terminal_retention,
    spawn_worker_control_consumer,
    worker::spawn_task_worker,
    AppConfig, AppState, RunEventRetentionPolicy, TaskTerminalRetentionPolicy,
};

const TASK_RUNNER_TOKIO_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_task_runner_dotenv();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TASK_RUNNER_TOKIO_THREAD_STACK_SIZE)
        .build()?;
    runtime.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    chatos_service_runtime::apply_config_center_env("task-runner")
        .await
        .map_err(|err| format!("apply managed config failed: {err}"))?;
    let pressure_config_client = chatos_config_sdk::ConfigClient::from_env("task-runner")
        .map_err(|err| format!("build Task Runner pressure config client failed: {err}"))?;
    let pressure_snapshot = pressure_config_client
        .load_strict()
        .await
        .map_err(|err| format!("load Task Runner pressure config failed: {err}"))?;
    let pressure_policy =
        task_runner_service_backend::pressure::TaskRunnerPressurePolicy::from_snapshot(
            &pressure_snapshot,
        )?;
    let pressure_state =
        task_runner_service_backend::pressure::TaskRunnerPressureState::new(pressure_policy);
    let run_event_retention_policy = RunEventRetentionPolicy::from_managed_env()?;
    let terminal_retention_policy = TaskTerminalRetentionPolicy::from_managed_env()?;
    configure_task_terminal_runtime(terminal_retention_policy)?;
    let mut config = AppConfig::from_env()?;
    let _telemetry = init_tracing(&config)?;
    resolve_downstream_services(&mut config).await;
    let app_state = AppState::new(config.clone()).await?;
    if config.worker_enabled() {
        chatos_mcp_runtime::initialize_mcp_invocation_result_queue(
            app_state
                .task_queue_topology
                .mcp_result_queue_config(config.worker_id.as_str())?,
        )
        .await?;
    }
    tracing::info!(
        run_dispatch_mode = app_state.task_queue_topology.run_dispatch_mode.as_str(),
        callback_delivery_mode = app_state
            .task_queue_topology
            .callback_delivery_mode
            .as_str(),
        rabbitmq_enabled = app_state.task_queue_topology.uses_rabbitmq(),
        rabbitmq_exchange = app_state.task_queue_topology.rabbitmq_exchange.as_str(),
        run_dispatch_queue = app_state.task_queue_topology.run_dispatch_queue.as_str(),
        run_dispatch_retry_queue = app_state
            .task_queue_topology
            .run_dispatch_retry_queue
            .as_str(),
        run_dispatch_retry_delay_ms = app_state
            .task_queue_topology
            .run_dispatch_retry_delay
            .as_millis(),
        run_dispatch_outbox_reconcile_ms = app_state
            .task_queue_topology
            .run_dispatch_outbox_reconcile_interval
            .as_millis(),
        run_dispatch_outbox_batch_size =
            app_state.task_queue_topology.run_dispatch_outbox_batch_size,
        worker_control_queue_prefix = app_state
            .task_queue_topology
            .worker_control_queue_prefix
            .as_str(),
        run_post_process_queue = app_state
            .task_queue_topology
            .run_post_process_queue
            .as_str(),
        run_post_process_retry_queue = app_state
            .task_queue_topology
            .run_post_process_retry_queue
            .as_str(),
        run_post_process_dead_letter_queue = app_state
            .task_queue_topology
            .run_post_process_dead_letter_queue
            .as_str(),
        run_post_process_max_delivery_attempts = app_state
            .task_queue_topology
            .run_post_process_max_delivery_attempts,
        run_post_process_retry_delay_ms = app_state
            .task_queue_topology
            .run_post_process_retry_delay
            .as_millis(),
        run_post_process_outbox_reconcile_ms = app_state
            .task_queue_topology
            .run_post_process_outbox_reconcile_interval
            .as_millis(),
        run_post_process_outbox_batch_size = app_state
            .task_queue_topology
            .run_post_process_outbox_batch_size,
        callback_delivery_queue = app_state
            .task_queue_topology
            .callback_delivery_queue
            .as_str(),
        run_events_routing_key = app_state
            .task_queue_topology
            .run_events_routing_key
            .as_str(),
        "task runner queue topology configured"
    );
    let mut background_handles = Vec::new();
    background_handles.push(spawn_task_terminal_retention());
    background_handles.push(spawn_run_dispatch_outbox_reconciler(
        app_state.task_queue_topology.clone(),
        app_state.run_service.clone(),
    ));
    background_handles.push(spawn_run_cancel_outbox_reconciler(
        app_state.task_queue_topology.clone(),
        app_state.run_service.clone(),
    ));
    background_handles.push(spawn_run_terminal_outbox_reconciler(
        app_state.task_queue_topology.clone(),
        app_state.run_service.clone(),
    ));
    background_handles.push(spawn_ask_user_resolution_outbox_reconciler(
        app_state.task_queue_topology.clone(),
        app_state.ask_user_prompt_service.clone(),
    ));
    background_handles.push(spawn_run_post_process_outbox_reconciler(
        app_state.task_queue_topology.clone(),
        app_state.run_service.clone(),
    ));

    if config.api_enabled() {
        background_handles.push(spawn_run_event_consumer(
            config.worker_id.clone(),
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
            app_state.run_event_resync_sender.clone(),
        ));
    }

    if config.scheduler_enabled() {
        background_handles.push(spawn_task_scheduler(
            config.clone(),
            app_state.task_service.clone(),
            app_state.run_service.clone(),
            pressure_state.clone(),
            app_state.runtime_stats.clone(),
        ));
        background_handles.push(spawn_run_event_retention(
            run_event_retention_policy,
            app_state.run_service.clone(),
            app_state.runtime_stats.clone(),
        ));
    }

    if config.worker_enabled() {
        background_handles.push(spawn_worker_control_consumer(
            config.clone(),
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ));
        background_handles.push(spawn_task_worker(
            config.clone(),
            app_state.run_service.clone(),
        ));
        background_handles.push(spawn_run_post_process_consumer(
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ));
    }

    if config.callback_delivery_enabled() {
        background_handles.push(spawn_chatos_callback_reconciler(
            app_state.run_service.clone(),
        ));
        if app_state.task_queue_topology.callback_delivery_mode
            == task_runner_service_backend::platform_queue::TaskQueueMode::RabbitMq
        {
            background_handles.push(spawn_chatos_callback_queue_consumer(
                config.clone(),
                app_state.task_queue_topology.clone(),
                app_state.run_service.clone(),
            ));
        }
    }

    let service_id = std::env::var("CHATOS_SERVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("task-runner-{}", std::process::id()));
    let running_version = std::env::var("CHATOS_SERVICE_VERSION").ok();
    background_handles.push(
        task_runner_service_backend::pressure::start_pressure_reporter(
            app_state.clone(),
            pressure_config_client,
            pressure_state,
            service_id,
            running_version,
        ),
    );

    if !config.api_enabled() {
        tracing::info!(
            role = config.role.as_str(),
            worker_id = config.worker_id.as_str(),
            "task_runner_service_backend running without HTTP API listener"
        );
        tokio::signal::ctrl_c().await?;
        for handle in background_handles {
            handle.abort();
        }
        return Ok(());
    }

    let bind_addr = config.bind_addr();
    let internal_tls = TaskRunnerInternalTlsConfig::from_env(config.host, config.port)?;
    let internal_mtls_config = load_internal_mtls_config(&internal_tls)?;
    let public_app = build_public_router(app_state.clone());
    let internal_app = build_internal_router(app_state);
    let _service_runtime =
        chatos_service_runtime::register_current_service("task-runner", config.port, "/api/health")
            .await;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(
        role = config.role.as_str(),
        "task_runner_service_backend listening on http://{}:{}",
        config.host,
        config.port
    );

    tracing::info!(
        "Task Runner internal API listening with mandatory mTLS on https://{}",
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

async fn resolve_downstream_services(config: &mut AppConfig) {
    config.user_service_base_url = chatos_service_runtime::resolve_service_base_url(
        "user-service",
        config.user_service_base_url.as_str(),
    )
    .await;
    config.default_sandbox_manager_base_url = chatos_service_runtime::resolve_service_base_url(
        "sandbox-manager",
        config.default_sandbox_manager_base_url.as_str(),
    )
    .await;
    if let Some(base_url) = config.project_service_base_url.clone() {
        config.project_service_base_url = Some(
            chatos_service_runtime::resolve_service_base_url("project-service", base_url.as_str())
                .await,
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
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("task_runner_service_backend=info,chatos_ai_runtime=info,tower_http=info")
    });
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.otlp_endpoint.clone())
        .with_timeout(config.otlp_export_timeout)
        .build()
        .map_err(|err| format!("build Task Runner OTLP trace exporter failed: {err}"))?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(Resource::builder().with_service_name("task-runner").build())
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            config.otlp_trace_sample_ratio,
        ))))
        .with_batch_exporter(trace_exporter)
        .build();
    let tracer = tracer_provider.tracer("task-runner");
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    global::set_text_map_propagator(TraceContextPropagator::new());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry_layer)
        .try_init()
        .map_err(|err| format!("initialize Task Runner tracing subscriber failed: {err}"))?;

    Ok(TelemetryGuard { tracer_provider })
}
