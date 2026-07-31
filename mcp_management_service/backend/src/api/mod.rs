// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
mod health;
mod routes;

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
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::net::{IpAddr, Ipv4Addr};

    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use chatos_mcp_management_sdk::McpCatalogResponse;
    use tower::ServiceExt;

    use super::*;
    use crate::config::AppConfig;

    fn state() -> AppState {
        AppState::new(AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 39280,
            internal_api_secret: "a-long-test-secret".to_string(),
            require_signed_internal_requests: true,
            allowed_internal_callers: BTreeSet::from(["task-runner".to_string()]),
        })
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
