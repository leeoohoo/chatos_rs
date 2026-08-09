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

fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/system/config", get(handlers::system_config))
        .route("/sandbox-pool/status", get(handlers::pool_status))
        .route("/sandbox-pool/config", put(handlers::update_pool_config))
        .route("/sandbox-images", get(handlers::list_sandbox_images))
        .route(
            "/sandbox-images/jobs",
            get(handlers::list_sandbox_image_jobs),
        )
        .route(
            "/sandbox-images/initialize",
            post(handlers::initialize_sandbox_image),
        )
        .route(
            "/sandbox-images/prepare-dependencies",
            post(handlers::prepare_sandbox_dependency_images),
        )
        .route(
            "/sandbox-images/mcp",
            post(handlers::sandbox_image_mcp_entrypoint),
        )
        .route(
            "/access-clients",
            get(handlers::list_access_clients).post(handlers::create_access_client),
        )
        .route(
            "/access-clients/{id}",
            put(handlers::update_access_client).delete(handlers::delete_access_client),
        )
        .route(
            "/access-clients/{id}/rotate-key",
            post(handlers::rotate_access_client_key),
        )
        .route("/sandboxes/leases", post(handlers::create_sandbox_lease))
        .route(
            "/sandbox-environments/leases",
            post(handlers::create_sandbox_environment_lease),
        )
        .route(
            "/sandbox-environments/{environment_id}",
            get(handlers::get_sandbox_environment),
        )
        .route(
            "/sandbox-environments/{environment_id}/start",
            post(handlers::start_sandbox_environment),
        )
        .route(
            "/sandbox-environments/{environment_id}/renew",
            post(handlers::renew_sandbox_environment_lease),
        )
        .route(
            "/sandbox-environments/{environment_id}/stop",
            post(handlers::stop_sandbox_environment),
        )
        .route(
            "/sandbox-environments/{environment_id}/services/{service_id}/exec",
            post(handlers::exec_sandbox_environment_service),
        )
        .route(
            "/sandbox-environments/{environment_id}/mcp",
            post(handlers::sandbox_environment_mcp_proxy),
        )
        .route(
            "/sandbox-environments/{environment_id}/browser-mcp",
            post(handlers::sandbox_environment_browser_mcp_proxy),
        )
        .route(
            "/sandbox-environments/{environment_id}/cloud-stdio-mcp/call",
            post(handlers::sandbox_environment_cloud_stdio_mcp_call),
        )
        .route(
            "/sandbox-environments/{environment_id}/cloud-stdio-mcp/cancel",
            post(handlers::sandbox_environment_cloud_stdio_mcp_cancel),
        )
        .route(
            "/sandbox-environments/{environment_id}/cloud-stdio-mcp/close",
            post(handlers::sandbox_environment_cloud_stdio_mcp_close),
        )
        .route("/sandboxes", get(handlers::list_sandboxes))
        .route(
            "/sandboxes/{sandbox_id}",
            get(handlers::get_sandbox).delete(handlers::destroy_sandbox),
        )
        .route(
            "/sandboxes/{sandbox_id}/heartbeat",
            post(handlers::heartbeat_sandbox),
        )
        .route(
            "/sandboxes/{sandbox_id}/health",
            get(handlers::health_sandbox),
        )
        .route(
            "/sandboxes/{sandbox_id}/mcp",
            post(handlers::sandbox_mcp_proxy),
        )
        .route(
            "/sandboxes/{sandbox_id}/browser-mcp",
            post(handlers::sandbox_browser_mcp_proxy),
        )
        .route(
            "/sandboxes/{sandbox_id}/cloud-stdio-mcp/call",
            post(handlers::sandbox_cloud_stdio_mcp_call),
        )
        .route(
            "/sandboxes/{sandbox_id}/cloud-stdio-mcp/cancel",
            post(handlers::sandbox_cloud_stdio_mcp_cancel),
        )
        .route(
            "/sandboxes/{sandbox_id}/cloud-stdio-mcp/close",
            post(handlers::sandbox_cloud_stdio_mcp_close),
        )
        .route(
            "/sandboxes/{sandbox_id}/release",
            post(handlers::release_sandbox),
        )
        .route(
            "/sandboxes/{sandbox_id}/events",
            get(handlers::list_sandbox_events),
        )
}

pub fn build_public_router(state: AppState) -> Router {
    let protected_api = protected_routes().layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_public_sandbox_auth,
    ));

    Router::new()
        .route("/health", get(handlers::health))
        .nest("/api", protected_api)
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

pub fn build_internal_router(state: AppState) -> Router {
    let protected_api = protected_routes().layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_internal_sandbox_auth,
    ));

    Router::new()
        .nest("/api/internal", protected_api)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}
