// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod prompt;
mod requirement;
mod runs;

use axum::routing::{get, post};
use axum::Router;

use crate::LocalRuntime;

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route(
            "/api/local/runtime/projects/{project_id}/requirements/{requirement_id}/execute",
            post(requirement::execute_requirement),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/requirements/{requirement_id}/stop",
            post(requirement::stop_requirement),
        )
        .route("/api/local/runtime/task-runs/{run_id}", get(runs::get_run))
        .route(
            "/api/local/runtime/task-runs/{run_id}/cancel",
            post(runs::cancel_run),
        )
        .route(
            "/api/local/runtime/task-runs/{run_id}/retry",
            post(runs::retry_run),
        )
}
