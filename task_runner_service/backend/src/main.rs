// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing_subscriber::EnvFilter;

use task_runner_service_backend::{
    build_router, load_task_runner_dotenv, scheduler::spawn_task_scheduler,
    worker::spawn_task_worker, AppConfig, AppState,
};

const TASK_RUNNER_TOKIO_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match chatos_mcp_runtime::process_isolation::maybe_run_exec_helper_from_env() {
        Ok(false) => {}
        Ok(true) => return Ok(()),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(126);
        }
    }

    load_task_runner_dotenv();
    init_tracing();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TASK_RUNNER_TOKIO_THREAD_STACK_SIZE)
        .build()?;
    runtime.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env()?;
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

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("task_runner_service_backend=info,chatos_ai_runtime=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
