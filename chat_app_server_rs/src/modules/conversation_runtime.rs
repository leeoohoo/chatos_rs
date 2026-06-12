#[path = "conversation_runtime/bootstrap.rs"]
pub mod bootstrap;
#[path = "conversation_runtime/chat_execution.rs"]
pub mod chat_execution;
#[path = "conversation_runtime/chat_runner.rs"]
pub mod chat_runner;
#[path = "conversation_runtime/chat_usecase.rs"]
pub mod chat_usecase;
#[path = "conversation_runtime/context_history.rs"]
pub mod context_history;
#[path = "conversation_runtime/guidance.rs"]
pub mod guidance;
#[path = "conversation_runtime/memory_compat.rs"]
pub mod memory_compat;
#[path = "conversation_runtime/messages.rs"]
pub mod messages;
#[path = "conversation_runtime/review_repair.rs"]
pub mod review_repair;
#[path = "conversation_runtime/runtime_context.rs"]
pub mod runtime_context;
#[path = "conversation_runtime/session_mcp_servers.rs"]
pub mod session_mcp_servers;
#[path = "conversation_runtime/session_scope.rs"]
pub mod session_scope;
#[path = "conversation_runtime/sessions.rs"]
pub mod sessions;
#[path = "conversation_runtime/snapshot.rs"]
pub mod snapshot;
#[path = "conversation_runtime/summaries.rs"]
pub mod summaries;
#[path = "conversation_runtime/task_board.rs"]
pub mod task_board;
#[path = "conversation_runtime/tools_panel.rs"]
pub mod tools_panel;
#[path = "conversation_runtime/turn_lifecycle.rs"]
pub mod turn_lifecycle;
#[path = "conversation_runtime/user_context.rs"]
pub mod user_context;

use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::agents::router())
        .merge(api::agent_chat::router())
        .merge(api::message_task_runner::router())
        .merge(api::messages::router())
        .merge(api::realtime::router())
        .merge(api::sessions::router())
        .merge(api::task_manager::router())
        .merge(api::ui_prompts::router())
}

pub fn public_routes() -> Router {
    Router::new().merge(api::agent_chat::public_router())
}
