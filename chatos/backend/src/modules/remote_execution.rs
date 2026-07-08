// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::remote_connections::router())
        .merge(api::terminals::router())
}
