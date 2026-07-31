// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
mod health;
mod mcp;
mod routes;
mod runtime_sessions;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/api/internal/catalog", get(catalog::catalog))
        .route("/api/internal/routes/resolve", post(routes::resolve_routes))
        .route(
            "/api/internal/runtime/sessions/resolve",
            post(runtime_sessions::resolve_runtime_session),
        )
        .route(
            "/api/internal/runtime/sessions/{session_id}/routes",
            get(runtime_sessions::runtime_session_routes),
        )
        .route("/mcp", post(mcp::mcp_entrypoint))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use chatos_mcp_management_sdk::McpCatalogResponse;
    use tower::ServiceExt;

    use super::*;
    use crate::config::AppConfig;

    fn state() -> AppState {
        AppState::new(AppConfig::test()).unwrap()
    }

    #[tokio::test]
    async fn catalog_endpoint_requires_and_accepts_scoped_internal_token() {
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-test-secret",
            "task-runner",
            "mcp-management-service",
            "catalog.read",
            60,
        )
        .unwrap();
        let request = Request::builder()
            .uri("/api/internal/catalog")
            .header("x-mcp-management-caller-service", "task-runner")
            .header("x-mcp-management-internal-token", token)
            .body(Body::empty())
            .unwrap();
        let response = build_router(state()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let catalog = serde_json::from_slice::<McpCatalogResponse>(&body).unwrap();
        assert_eq!(catalog.total, chatos_mcp::system_mcp_catalog().len());
    }

    #[tokio::test]
    async fn catalog_endpoint_rejects_missing_internal_identity() {
        let request = Request::builder()
            .uri("/api/internal/catalog")
            .body(Body::empty())
            .unwrap();
        let response = build_router(state()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
