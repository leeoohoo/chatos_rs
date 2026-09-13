// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "conversation_runtime/context_history.rs"]
pub mod context_history;
#[path = "conversation_runtime/memory_compat.rs"]
pub mod memory_compat;
#[path = "conversation_runtime/messages.rs"]
pub mod messages;
#[path = "conversation_runtime/review_repair.rs"]
pub mod review_repair;
#[path = "conversation_runtime/session_scope.rs"]
pub mod session_scope;
#[path = "conversation_runtime/sessions.rs"]
pub mod sessions;
#[path = "conversation_runtime/summaries.rs"]
pub mod summaries;
#[path = "conversation_runtime/user_context.rs"]
pub mod user_context;

use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::agents::router())
        .merge(api::attachments::router())
        .merge(api::messages::router())
        .merge(api::realtime::router())
        .merge(api::sessions::router())
}

pub fn public_routes() -> Router {
    Router::new().merge(api::attachments::public_router())
}
