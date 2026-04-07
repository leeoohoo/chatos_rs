use std::sync::Arc;

use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

mod auth_session_api;
mod action_requests_api;
mod contacts_api;
mod conversations_api;
mod event_publish;
mod internal_events_api;
mod messages_api;
mod runs_api;
mod shared;
mod users_api;
mod ws_api;

pub type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/im/v1/auth/login", post(auth_session_api::login))
        .route("/api/im/v1/auth/me", get(auth_session_api::me))
        .route("/api/im/v1/ws", get(ws_api::im_events_ws))
        .route(
            "/api/im/v1/users",
            get(users_api::list_users).post(users_api::create_user),
        )
        .route("/api/im/v1/users/:username", patch(users_api::update_user))
        .route(
            "/api/im/v1/contacts",
            get(contacts_api::list_contacts).post(contacts_api::create_contact),
        )
        .route(
            "/api/im/v1/contacts/:contact_id",
            get(contacts_api::get_contact),
        )
        .route(
            "/api/im/v1/conversations",
            get(conversations_api::list_conversations).post(conversations_api::create_conversation),
        )
        .route(
            "/api/im/v1/conversations/:conversation_id",
            get(conversations_api::get_conversation).patch(conversations_api::update_conversation),
        )
        .route(
            "/api/im/v1/conversations/:conversation_id/read",
            post(conversations_api::mark_conversation_read),
        )
        .route(
            "/api/im/v1/conversations/:conversation_id/messages",
            get(messages_api::list_messages).post(messages_api::create_message),
        )
        .route(
            "/api/im/v1/internal/conversations/:conversation_id/messages",
            post(messages_api::internal_create_message),
        )
        .route(
            "/api/im/v1/conversations/:conversation_id/action-requests",
            get(action_requests_api::list_action_requests),
        )
        .route(
            "/api/im/v1/internal/action-requests",
            post(action_requests_api::create_action_request),
        )
        .route(
            "/api/im/v1/internal/action-requests/:action_request_id",
            get(action_requests_api::get_action_request)
                .patch(action_requests_api::update_action_request),
        )
        .route(
            "/api/im/v1/conversations/:conversation_id/runs",
            get(runs_api::list_runs),
        )
        .route(
            "/api/im/v1/internal/runs",
            post(runs_api::create_run),
        )
        .route(
            "/api/im/v1/internal/runs/:run_id",
            patch(runs_api::update_run),
        )
        .route(
            "/api/im/v1/internal/events",
            post(internal_events_api::publish_event),
        )
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "im_service"
    }))
}
