// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
mod ask_user_prompt_retention;
pub mod ask_user_prompt_service;
pub mod auth;
pub mod config;
mod http_body;
pub mod internal_tls;
#[path = "services/tool_runtime/mcp_server.rs"]
pub mod mcp_server;
pub mod models;
pub mod notepad_store;
pub mod platform_queue;
pub mod pressure;
pub mod remote_server_runtime;
mod run_dispatch_queue;
mod run_event_queue;
mod run_event_retention;
mod run_post_process_queue;
pub mod scheduler;
pub mod services;
pub mod state;
pub mod store;
pub mod terminal_store;
mod trace_context;
pub mod worker;
mod worker_control_queue;

pub use api::{build_internal_router, build_public_router};
pub use ask_user_prompt_retention::{
    spawn_ask_user_prompt_retention, AskUserPromptRetentionPolicy,
};
pub use config::{load_task_runner_dotenv, AppConfig, TaskRunnerRole};
pub use run_dispatch_queue::spawn_run_dispatch_outbox_reconciler;
pub use run_event_queue::spawn_run_event_consumer;
pub use run_event_retention::{spawn_run_event_retention, RunEventRetentionPolicy};
pub use run_post_process_queue::{
    spawn_run_post_process_consumer, spawn_run_post_process_outbox_reconciler,
};
pub use state::AppState;
pub use terminal_store::{
    configure_task_terminal_runtime, spawn_task_terminal_retention, TaskTerminalRetentionPolicy,
};
pub use worker_control_queue::{
    spawn_ask_user_resolution_outbox_reconciler, spawn_run_cancel_outbox_reconciler,
    spawn_run_terminal_outbox_reconciler, spawn_worker_control_consumer,
};
