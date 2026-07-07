// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::code_nav::router())
        .merge(api::fs::router())
        .merge(api::git::router())
        .merge(api::local_connectors::router())
        .merge(api::notepad::router())
        .merge(api::projects::router())
}
