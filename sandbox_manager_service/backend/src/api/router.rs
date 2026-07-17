// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::middleware;
use axum::routing::{get, post, put};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::state::AppState;

use super::handlers;

pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/system/config", get(handlers::system_config))
        .route("/api/sandbox-pool/status", get(handlers::pool_status))
        .route(
            "/api/sandbox-pool/config",
            put(handlers::update_pool_config),
        )
        .route("/api/sandbox-images", get(handlers::list_sandbox_images))
        .route(
            "/api/sandbox-images/jobs",
            get(handlers::list_sandbox_image_jobs),
        )
        .route(
            "/api/sandbox-images/initialize",
            post(handlers::initialize_sandbox_image),
        )
        .route(
            "/api/sandbox-images/prepare-dependencies",
            post(handlers::prepare_sandbox_dependency_images),
        )
        .route(
            "/api/sandbox-images/mcp",
            post(handlers::sandbox_image_mcp_entrypoint),
        )
        .route(
            "/api/access-clients",
            get(handlers::list_access_clients).post(handlers::create_access_client),
        )
        .route(
            "/api/access-clients/{id}",
            put(handlers::update_access_client).delete(handlers::delete_access_client),
        )
        .route(
            "/api/access-clients/{id}/rotate-key",
            post(handlers::rotate_access_client_key),
        )
        .route(
            "/api/sandboxes/leases",
            post(handlers::create_sandbox_lease),
        )
        .route(
            "/api/sandbox-environments/leases",
            post(handlers::create_sandbox_environment_lease),
        )
        .route(
            "/api/sandbox-environments/{environment_id}",
            get(handlers::get_sandbox_environment),
        )
        .route(
            "/api/sandbox-environments/{environment_id}/start",
            post(handlers::start_sandbox_environment),
        )
        .route(
            "/api/sandbox-environments/{environment_id}/stop",
            post(handlers::stop_sandbox_environment),
        )
        .route(
            "/api/sandbox-environments/{environment_id}/services/{service_id}/exec",
            post(handlers::exec_sandbox_environment_service),
        )
        .route(
            "/api/sandbox-environments/{environment_id}/mcp",
            post(handlers::sandbox_environment_mcp_proxy),
        )
        .route("/api/sandboxes", get(handlers::list_sandboxes))
        .route(
            "/api/sandboxes/{sandbox_id}",
            get(handlers::get_sandbox).delete(handlers::destroy_sandbox),
        )
        .route(
            "/api/sandboxes/{sandbox_id}/heartbeat",
            post(handlers::heartbeat_sandbox),
        )
        .route(
            "/api/sandboxes/{sandbox_id}/health",
            get(handlers::health_sandbox),
        )
        .route(
            "/api/sandboxes/{sandbox_id}/mcp",
            post(handlers::sandbox_mcp_proxy),
        )
        .route(
            "/api/sandboxes/{sandbox_id}/release",
            post(handlers::release_sandbox),
        )
        .route(
            "/api/sandboxes/{sandbox_id}/events",
            get(handlers::list_sandbox_events),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_sandbox_auth,
        ));

    Router::new()
        .route("/health", get(handlers::health))
        .merge(protected_api)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}
