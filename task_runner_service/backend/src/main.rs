// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use task_runner_service_backend::{
    build_router, load_task_runner_dotenv, scheduler::spawn_task_scheduler,
    services::spawn_chatos_callback_reconciler, worker::spawn_task_worker, AppConfig, AppState,
};

const TASK_RUNNER_TOKIO_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_task_runner_dotenv();
    init_tracing();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TASK_RUNNER_TOKIO_THREAD_STACK_SIZE)
        .build()?;
    runtime.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    chatos_service_runtime::apply_config_center_env("task-runner").await;
    let mut config = AppConfig::from_env()?;
    resolve_downstream_services(&mut config).await;
    let app_state = AppState::new(config.clone()).await?;
    let mut background_handles = Vec::new();

    if config.scheduler_enabled() {
        background_handles.push(spawn_task_scheduler(
            config.clone(),
            app_state.task_service.clone(),
            app_state.run_service.clone(),
        ));
    }

    if config.worker_enabled() {
        background_handles.push(spawn_task_worker(
            config.clone(),
            app_state.run_service.clone(),
        ));
    }

    if config.chatos_callback_url.is_some() {
        background_handles.push(spawn_chatos_callback_reconciler(
            app_state.run_service.clone(),
        ));
    }

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
    let app = build_router(app_state);
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

    axum::serve(listener, app).await?;
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
    if let Some(base_url) = config.memory_engine_base_url.clone() {
        config.memory_engine_base_url = Some(
            chatos_service_runtime::resolve_service_url(
                "memory-engine",
                base_url.as_str(),
                "/api/memory-engine/v1",
            )
            .await,
        );
    }
    if let Some(base_url) = config.project_service_base_url.clone() {
        config.project_service_base_url = Some(
            chatos_service_runtime::resolve_service_base_url("project-service", base_url.as_str())
                .await,
        );
    }
    if let Some(callback_url) = config.chatos_callback_url.clone() {
        config.chatos_callback_url = Some(
            chatos_service_runtime::resolve_service_url(
                "chatos-backend",
                callback_url.as_str(),
                "/api/agent/chat/task-runner/callback",
            )
            .await,
        );
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("task_runner_service_backend=info,chatos_ai_runtime=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
