// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod context;
mod error;
mod health;
mod workspaces;

use axum::routing::get;
use axum::Router;

use crate::LocalRuntime;

pub(crate) fn connector_capability_router() -> Router<LocalRuntime> {
    Router::new()
        .route("/api/local/runtime/health", get(health::health))
        .route("/api/local/runtime/devices", get(workspaces::list_devices))
        .route(
            "/api/local/runtime/workspaces",
            get(workspaces::list_workspaces),
        )
        .route(
            "/api/local/runtime/workspaces/{workspace_id}/directories",
            get(workspaces::list_directory).post(workspaces::create_directory),
        )
}
