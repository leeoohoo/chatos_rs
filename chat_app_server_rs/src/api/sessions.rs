use axum::{
    routing::{delete, get},
    Router,
};

mod contracts;
mod history;
mod mcp_server_handlers;
mod message_handlers;
mod session_handlers;
mod summary_handlers;
mod support;
mod ws_handlers;

use self::mcp_server_handlers::{add_mcp_server, delete_mcp_server, list_mcp_servers};
use self::message_handlers::{
    create_session_message, get_session_messages, get_session_turn_process_messages,
    get_session_turn_process_messages_by_turn, get_session_turn_runtime_context_by_turn,
    get_session_turn_runtime_context_latest,
};
use self::session_handlers::{
    create_session, delete_session, get_session, list_sessions, update_session,
};
use self::summary_handlers::{
    clear_session_memory_summaries, delete_session_memory_summary, list_session_memory_summaries,
};
use self::ws_handlers::session_events_ws;

pub fn router() -> Router {
    Router::new()
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route(
            "/api/sessions/:id",
            get(get_session).put(update_session).delete(delete_session),
        )
        .route(
            "/api/sessions/:session_id/mcp-servers",
            get(list_mcp_servers).post(add_mcp_server),
        )
        .route(
            "/api/sessions/:session_id/mcp-servers/:mcp_config_id",
            delete(delete_mcp_server),
        )
        .route(
            "/api/sessions/:session_id/messages",
            get(get_session_messages).post(create_session_message),
        )
        .route("/api/sessions/:session_id/ws", get(session_events_ws))
        .route(
            "/api/sessions/:session_id/turns/:user_message_id/process",
            get(get_session_turn_process_messages),
        )
        .route(
            "/api/sessions/:session_id/turns/by-turn/:turn_id/process",
            get(get_session_turn_process_messages_by_turn),
        )
        .route(
            "/api/sessions/:session_id/turns/latest/runtime-context",
            get(get_session_turn_runtime_context_latest),
        )
        .route(
            "/api/sessions/:session_id/turns/by-turn/:turn_id/runtime-context",
            get(get_session_turn_runtime_context_by_turn),
        )
        .route(
            "/api/sessions/:session_id/summaries",
            get(list_session_memory_summaries).delete(clear_session_memory_summaries),
        )
        .route(
            "/api/sessions/:session_id/summaries/:summary_id",
            delete(delete_session_memory_summary),
        )
}
