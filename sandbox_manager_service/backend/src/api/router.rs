use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

use super::handlers;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/api/system/config", get(handlers::system_config))
        .route("/api/sandbox-pool/status", get(handlers::pool_status))
        .route(
            "/api/sandboxes/leases",
            post(handlers::create_sandbox_lease),
        )
        .route("/api/sandboxes", get(handlers::list_sandboxes))
        .route(
            "/api/sandboxes/:sandbox_id",
            get(handlers::get_sandbox).delete(handlers::destroy_sandbox),
        )
        .route(
            "/api/sandboxes/:sandbox_id/heartbeat",
            post(handlers::heartbeat_sandbox),
        )
        .route(
            "/api/sandboxes/:sandbox_id/health",
            get(handlers::health_sandbox),
        )
        .route(
            "/api/sandboxes/:sandbox_id/mcp/tools",
            get(handlers::sandbox_mcp_tools),
        )
        .route(
            "/api/sandboxes/:sandbox_id/mcp/call",
            post(handlers::sandbox_mcp_call),
        )
        .route(
            "/api/sandboxes/:sandbox_id/release",
            post(handlers::release_sandbox),
        )
        .route(
            "/api/sandboxes/:sandbox_id/events",
            get(handlers::list_sandbox_events),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}
