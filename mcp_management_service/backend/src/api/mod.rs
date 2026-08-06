// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
mod health;
mod invocations;
pub(crate) mod mcp;
mod queue_operations;
mod routes;
mod runtime_session_metadata;
mod runtime_sessions;
mod system;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

fn router_layers(router: Router, surface: &'static str) -> Router {
    router
        .layer(middleware::from_fn(
            crate::trace_context::accept_remote_parent,
        ))
        .layer(TraceLayer::new_for_http().make_span_with(
            move |request: &axum::http::Request<axum::body::Body>| {
                let route = request
                    .extensions()
                    .get::<axum::extract::MatchedPath>()
                    .map(axum::extract::MatchedPath::as_str)
                    .unwrap_or("/unmatched");
                tracing::info_span!(
                    "http.request",
                    otel.kind = "server",
                    otel.name = %format!("{} {route}", request.method()),
                    http.request.method = %request.method(),
                    http.route = route,
                    surface
                )
            },
        ))
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

pub fn build_public_router(state: AppState) -> Router {
    router_layers(
        Router::new()
            .route("/health", get(health::health))
            .route("/metrics", get(system::prometheus_metrics))
            .route("/mcp", post(mcp::mcp_entrypoint))
            .with_state(state),
        "public",
    )
}

pub fn build_internal_router(state: AppState) -> Router {
    router_layers(
        Router::new()
            .route("/api/internal/catalog", get(catalog::catalog))
            .route("/api/internal/system/stats", get(system::system_stats))
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
                "/api/internal/runtime/invocations/{invocation_id}",
                get(invocations::get_runtime_invocation),
            )
            .route(
                "/api/internal/runtime/invocations/{invocation_id}/cancel",
                post(invocations::cancel_runtime_invocation),
            )
            .route(
                "/api/internal/queue-operations/async-tool/archive",
                post(queue_operations::archive_async_tool_dead_letter),
            )
            .with_state(state),
        "internal",
    )
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
            "a-long-task-runner-secret",
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
        let response = build_internal_router(state().await)
            .oneshot(request)
            .await
            .unwrap();
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
        let response = build_internal_router(state().await)
            .oneshot(request)
            .await
            .unwrap();
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
                tenant_id: "tenant-1".to_string(),
                owner_user_id: "user-1".to_string(),
                project_id: "project-1".to_string(),
                device_id: None,
                resource_id: "mcp-1".to_string(),
                exposed_tool_name: "demo_read".to_string(),
                original_tool_name: "demo_read".to_string(),
                mutation_may_have_started: false,
                cancel_supported: true,
                status: RuntimeInvocationStatus::Running,
                async_execution: false,
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                started_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
                completed_at_unix_ms: None,
                terminal_result: None,
                terminal_error_code: None,
                terminal_error_message: None,
                file_modification_outcome: None,
                result_reply_to: None,
                result_event_id: None,
                result_event_pending: false,
                expires_at: DateTime::from_millis(
                    (chrono::Utc::now().timestamp() + 60).saturating_mul(1_000),
                ),
                expires_at_unix: chrono::Utc::now().timestamp() + 60,
            })
            .await
            .unwrap();
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-secret",
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
        let response = build_internal_router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(
            body.get("status").and_then(serde_json::Value::as_str),
            Some("cancel_requested")
        );
    }

    #[tokio::test]
    async fn internal_get_invocation_endpoint_returns_terminal_result() {
        let state = state().await;
        state
            .runtime_invocations
            .register(RuntimeInvocationRecord {
                invocation_id: "invocation-read-test".to_string(),
                session_id: "session-read-test".to_string(),
                request_id_key: "\"request-read-test\"".to_string(),
                caller_service: "task-runner".to_string(),
                tenant_id: "tenant-1".to_string(),
                owner_user_id: "user-1".to_string(),
                project_id: "project-1".to_string(),
                device_id: None,
                resource_id: "mcp-1".to_string(),
                exposed_tool_name: "demo_read".to_string(),
                original_tool_name: "demo_read".to_string(),
                mutation_may_have_started: false,
                cancel_supported: true,
                status: RuntimeInvocationStatus::Running,
                async_execution: true,
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                started_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
                completed_at_unix_ms: None,
                terminal_result: None,
                terminal_error_code: None,
                terminal_error_message: None,
                file_modification_outcome: None,
                result_reply_to: None,
                result_event_id: None,
                result_event_pending: false,
                expires_at: DateTime::from_millis(
                    (chrono::Utc::now().timestamp() + 60).saturating_mul(1_000),
                ),
                expires_at_unix: chrono::Utc::now().timestamp() + 60,
            })
            .await
            .unwrap();
        state
            .runtime_invocations
            .complete("invocation-read-test", serde_json::json!({"ok": true}))
            .await
            .unwrap();
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-secret",
            "task-runner",
            "mcp-management-service",
            "runtime.invocations.read",
            60,
        )
        .unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("/api/internal/runtime/invocations/invocation-read-test")
            .header("x-mcp-management-caller-service", "task-runner")
            .header("x-mcp-management-internal-token", token)
            .body(Body::empty())
            .unwrap();
        let response = build_internal_router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(
            body.get("status").and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(
            body.get("terminal_result"),
            Some(&serde_json::json!({"ok": true}))
        );
        assert_eq!(
            body.get("original_tool_name"),
            Some(&serde_json::json!("demo_read"))
        );
        assert!(body.get("file_modification_outcome").is_none());
    }

    #[tokio::test]
    async fn internal_system_stats_endpoint_returns_runtime_observability_details() {
        let state = state().await;
        state
            .runtime_invocations
            .register(RuntimeInvocationRecord {
                invocation_id: "invocation-system-stats".to_string(),
                session_id: "session-system-stats".to_string(),
                request_id_key: "\"request-system-stats\"".to_string(),
                caller_service: "task-runner".to_string(),
                tenant_id: "tenant-1".to_string(),
                owner_user_id: "user-1".to_string(),
                project_id: "project-1".to_string(),
                device_id: None,
                resource_id: "mcp-1".to_string(),
                exposed_tool_name: "demo_read".to_string(),
                original_tool_name: "demo_read".to_string(),
                mutation_may_have_started: false,
                cancel_supported: true,
                status: RuntimeInvocationStatus::Queued,
                async_execution: true,
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                started_at_unix_ms: None,
                completed_at_unix_ms: None,
                terminal_result: None,
                terminal_error_code: None,
                terminal_error_message: None,
                file_modification_outcome: None,
                result_reply_to: None,
                result_event_id: None,
                result_event_pending: false,
                expires_at: DateTime::from_millis(
                    (chrono::Utc::now().timestamp() + 60).saturating_mul(1_000),
                ),
                expires_at_unix: chrono::Utc::now().timestamp() + 60,
            })
            .await
            .unwrap();
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-secret",
            "task-runner",
            "mcp-management-service",
            "system.stats.read",
            60,
        )
        .unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("/api/internal/system/stats")
            .header("x-mcp-management-caller-service", "task-runner")
            .header("x-mcp-management-internal-token", token)
            .body(Body::empty())
            .unwrap();
        let response = build_internal_router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(body.get("ok"), Some(&serde_json::json!(true)));
        assert_eq!(
            body.pointer("/async_tool_dispatch/mode"),
            Some(&serde_json::json!("local_queue"))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/cancellation_exchange"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/queue_max_length"),
            Some(&serde_json::json!(10_000))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/queue_max_bytes"),
            Some(&serde_json::json!(256_u64 * 1024 * 1024))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/rabbitmq_reconnect_ms"),
            Some(&serde_json::json!(3_000))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/result_outbox_reconcile_ms"),
            Some(&serde_json::json!(5_000))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/result_outbox_batch_size"),
            Some(&serde_json::json!(128))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/runtime/enqueue_accepted_total"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/runtime/enqueue_capacity_rejected_total"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/runtime/enqueue_unavailable_total"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/runtime/cancellation_consumer_connected"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/rabbitmq_queues/enabled"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(
            body.pointer("/async_tool_dispatch/rabbitmq_queues/queues"),
            Some(&serde_json::json!([]))
        );
        assert_eq!(
            body.pointer("/runtime_invocations/queued"),
            Some(&serde_json::json!(1))
        );
        assert_eq!(
            body.pointer("/runtime_invocations/pending_result_events"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            body.pointer("/runtime_invocations/file_modifications/total"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            body.pointer("/runtime_invocations/quota_limits/tenant"),
            Some(&serde_json::json!(100_000))
        );
        assert_eq!(
            body.pointer("/runtime_invocations/quota_limits/device"),
            Some(&serde_json::json!(100_000))
        );
        assert_eq!(
            body.pointer("/runtime_sessions/backend"),
            Some(&serde_json::json!("memory"))
        );
        assert_eq!(
            body.pointer("/runtime_sessions/cache_hits_total"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            body.pointer("/runtime_sessions/cache_misses_total"),
            Some(&serde_json::json!(0))
        );
    }

    #[tokio::test]
    async fn metrics_endpoint_exposes_stable_prometheus_queue_metrics_without_auth() {
        let request = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();
        let response = build_public_router(state().await)
            .oneshot(request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/plain; version=0.0.4; charset=utf-8")
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains(
            "chatos_rabbitmq_queue_observability_enabled{service=\"mcp-management-service\"} 0"
        ));
        assert!(body.contains(
            "chatos_mcp_runtime_session_metrics_available{service=\"mcp-management-service\"} 1"
        ));
        assert!(body.contains("chatos_mcp_runtime_session_cache_entries{"));
        assert!(body.contains("chatos_mcp_runtime_session_cache_hits_total{"));
        assert!(!body.contains("rabbitmq_url"));
        assert!(!body.contains("amqp://"));
    }

    #[tokio::test]
    async fn public_listener_does_not_expose_internal_routes() {
        let request = Request::builder()
            .uri("/api/internal/catalog")
            .body(Body::empty())
            .unwrap();
        let response = build_public_router(state().await)
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn internal_listener_does_not_expose_public_mcp_route() {
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .body(Body::empty())
            .unwrap();
        let response = build_internal_router(state().await)
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
