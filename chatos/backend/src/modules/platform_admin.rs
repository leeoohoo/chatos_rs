// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Router;

use crate::api;

#[path = "platform_admin/system_context_ai.rs"]
pub mod system_context_ai;

pub fn public_routes() -> Router {
    Router::new().merge(api::auth::router())
}

pub fn protected_routes() -> Router {
    Router::new()
        .merge(api::auth::protected_router())
        .nest("/api/applications", api::applications::router())
        .merge(api::configs::router())
        .merge(api::contacts::router())
        .merge(api::system_contexts::router())
        .merge(api::user_settings::router())
}
