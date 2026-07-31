// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
mod health;
mod invocations;
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
        .route(
            "/api/internal/runtime/sessions/{session_id}/close",
            post(runtime_sessions::close_runtime_session),
        )
        .route(
            "/api/internal/runtime/invocations/{invocation_id}/cancel",
            post(invocations::cancel_runtime_invocation),
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
    use mongodb::bson::DateTime;
    use tower::ServiceExt;

    use super::*;
    use crate::config::AppConfig;
    use crate::runtime::{RuntimeInvocationRecord, RuntimeInvocationStatus};

    async fn state() -> AppState {
        AppState::new(AppConfig::test()).await.unwrap()
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
        let response = build_router(state().await).oneshot(request).await.unwrap();
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
        let response = build_router(state().await).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn internal_cancel_endpoint_is_scoped_to_the_invocation_owner_service() {
        let state = state().await;
        state
            .runtime_invocations
            .register(RuntimeInvocationRecord {
                invocation_id: "invocation-api-test".to_string(),
                session_id: "session-api-test".to_string(),
                request_id_key: "\"request-api-test\"".to_string(),
                caller_service: "task-runner".to_string(),
                resource_id: "mcp-1".to_string(),
                exposed_tool_name: "demo_read".to_string(),
                mutation_may_have_started: false,
                cancel_supported: true,
                status: RuntimeInvocationStatus::Running,
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                expires_at: DateTime::from_millis(
                    (chrono::Utc::now().timestamp() + 60).saturating_mul(1_000),
                ),
                expires_at_unix: chrono::Utc::now().timestamp() + 60,
            })
            .await
            .unwrap();
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-test-secret",
            "task-runner",
            "mcp-management-service",
            "runtime.invocations.cancel",
            60,
        )
        .unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("/api/internal/runtime/invocations/invocation-api-test/cancel")
            .header("x-mcp-management-caller-service", "task-runner")
            .header("x-mcp-management-internal-token", token)
            .body(Body::empty())
            .unwrap();
        let response = build_router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(
            body.get("status").and_then(serde_json::Value::as_str),
            Some("cancel_requested")
        );
    }
}
