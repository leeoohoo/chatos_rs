// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{middleware, Router};

use crate::api::{memory_auth, model_profile_auth, operator_auth};
use crate::state::AppState;

mod admin;
mod core;
mod sdk;

fn common_layers(router: Router) -> Router {
    router.layer(middleware::from_fn(
        chatos_service_runtime::request_id_middleware,
    ))
}

pub fn build_public_router(state: Arc<AppState>) -> Router {
    let protected_state = state.clone();

    common_layers(
        Router::new()
            .merge(
                admin::model_profile_routes().route_layer(middleware::from_fn_with_state(
                    protected_state.clone(),
                    model_profile_auth::require_user_memory_auth,
                )),
            )
            .merge(admin::routes().route_layer(middleware::from_fn_with_state(
                protected_state.clone(),
                memory_auth::require_user_memory_auth,
            )))
            .merge(sdk::routes())
            .merge(core::public_routes())
            .merge(
                core::data_routes().route_layer(middleware::from_fn_with_state(
                    protected_state,
                    memory_auth::require_user_memory_auth,
                )),
            )
            .with_state(state),
    )
}

pub fn build_internal_router(state: Arc<AppState>) -> Router {
    let protected_state = state.clone();
    common_layers(
        Router::new()
            .merge(
                admin::model_profile_routes().route_layer(middleware::from_fn_with_state(
                    protected_state.clone(),
                    model_profile_auth::require_model_profile_internal_auth,
                )),
            )
            .merge(
                admin::routes().route_layer(middleware::from_fn_with_state(
                    protected_state.clone(),
                    memory_auth::require_memory_auth,
                )),
            )
            .merge(
                core::data_routes().route_layer(middleware::from_fn_with_state(
                    protected_state.clone(),
                    memory_auth::require_memory_auth,
                )),
            )
            .merge(
                core::operator_routes().route_layer(middleware::from_fn_with_state(
                    protected_state,
                    operator_auth::require_operator_auth,
                )),
            )
            .with_state(state),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use mongodb::Client;
    use tower::ServiceExt;

    use super::{build_internal_router, build_public_router};
    use crate::api::internal_auth::{DATA_SCOPE, MODEL_PROFILE_SYNC_SCOPE, TOKEN_AUDIENCE};
    use crate::config::AppConfig;
    use crate::pressure::{
        MemoryEnginePressurePolicy, MemoryEnginePressureState, PlatformPressureLevel,
    };
    use crate::state::{AppState, MemoryEngineRuntimeStats};

    const USER_SERVICE_SECRET: &str = "test-user-service-memory-engine-signing-secret";
    const TASK_RUNNER_SECRET: &str = "test-task-runner-memory-engine-signing-secret";

    #[tokio::test]
    async fn public_router_does_not_expose_operator_routes() {
        let router = build_public_router(test_state().await);
        for path in [
            "/api/internal/system/stats",
            "/api/memory-engine/v1/jobs/summaries/run-once",
            "/api/memory-engine/v1/queue-operations/replay",
            "/api/memory-engine/v1/sources/source-a",
        ] {
            let response = router
                .clone()
                .oneshot(Request::post(path).body(Body::empty()).expect("request"))
                .await
                .expect("router response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "path={path}");
        }
    }

    #[tokio::test]
    async fn internal_router_does_not_expose_public_or_sdk_routes() {
        let router = build_internal_router(test_state().await);
        for path in [
            "/health",
            "/metrics",
            "/api/memory-engine/v1/sdk/auth/status",
        ] {
            let response = router
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).expect("request"))
                .await
                .expect("router response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "path={path}");
        }
    }

    #[tokio::test]
    async fn public_data_route_rejects_internal_service_headers() {
        let token = service_token(TASK_RUNNER_SECRET, "task-runner", DATA_SCOPE);
        let response = build_public_router(test_state().await)
            .oneshot(
                Request::get("/api/memory-engine/v1/threads/thread-a")
                    .header("x-memory-caller", "task-runner")
                    .header("x-memory-internal-token", token)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_data_route_requires_allowed_caller_and_data_scope() {
        let router = build_internal_router(test_state().await);
        let wrong_scope = service_token(
            TASK_RUNNER_SECRET,
            "task-runner",
            MODEL_PROFILE_SYNC_SCOPE,
        );
        let wrong_scope_response = router
            .clone()
            .oneshot(thread_upsert_request("task-runner", wrong_scope))
            .await
            .expect("wrong scope response");
        assert_eq!(wrong_scope_response.status(), StatusCode::UNAUTHORIZED);

        let valid = service_token(TASK_RUNNER_SECRET, "task-runner", DATA_SCOPE);
        let valid_response = router
            .oneshot(thread_upsert_request("task-runner", valid))
            .await
            .expect("valid identity response");
        assert_eq!(valid_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn public_model_profile_route_rejects_internal_service_headers() {
        let token = service_token(
            USER_SERVICE_SECRET,
            "user-service",
            MODEL_PROFILE_SYNC_SCOPE,
        );
        let response = build_public_router(test_state().await)
            .oneshot(
                Request::get("/api/memory-engine/v1/admin/model-profiles")
                    .header("x-memory-caller", "user-service")
                    .header("x-memory-internal-token", token)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_model_profile_route_requires_user_service_sync_scope() {
        let state = test_state().await;
        let router = build_internal_router(state);

        let wrong_scope = service_token(USER_SERVICE_SECRET, "user-service", DATA_SCOPE);
        let wrong_scope_response = router
            .clone()
            .oneshot(model_profile_create_request("user-service", wrong_scope))
            .await
            .expect("wrong scope response");
        assert_eq!(wrong_scope_response.status(), StatusCode::UNAUTHORIZED);

        let wrong_caller =
            service_token(TASK_RUNNER_SECRET, "task-runner", MODEL_PROFILE_SYNC_SCOPE);
        let wrong_caller_response = router
            .clone()
            .oneshot(model_profile_create_request("task-runner", wrong_caller))
            .await
            .expect("wrong caller response");
        assert_eq!(wrong_caller_response.status(), StatusCode::FORBIDDEN);

        let valid = service_token(
            USER_SERVICE_SECRET,
            "user-service",
            MODEL_PROFILE_SYNC_SCOPE,
        );
        let valid_response = router
            .oneshot(model_profile_create_request("user-service", valid))
            .await
            .expect("valid identity response");
        assert_eq!(valid_response.status(), StatusCode::BAD_REQUEST);
    }

    fn model_profile_create_request(caller: &str, token: String) -> Request<Body> {
        Request::post("/api/memory-engine/v1/admin/model-profiles")
            .header("x-memory-caller", caller)
            .header("x-memory-internal-token", token)
            .header("content-type", "application/json")
            .body(Body::empty())
            .expect("request")
    }

    fn thread_upsert_request(caller: &str, token: String) -> Request<Body> {
        Request::put("/api/memory-engine/v1/threads/thread-a")
            .header("x-memory-caller", caller)
            .header("x-memory-internal-token", token)
            .header("content-type", "application/json")
            .body(Body::empty())
            .expect("request")
    }

    fn service_token(secret: &str, caller: &str, scope: &str) -> String {
        chatos_service_runtime::issue_internal_service_token(
            secret,
            caller,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .expect("issue service token")
    }

    async fn test_state() -> Arc<AppState> {
        let config = test_config();
        let client = Client::with_uri_str(config.mongodb_uri.as_str())
            .await
            .expect("MongoDB client");
        Arc::new(AppState {
            pool: client.database(config.mongodb_database.as_str()),
            user_service_http: reqwest::Client::new(),
            runtime_stats: Arc::new(MemoryEngineRuntimeStats::default()),
            rabbitmq_queue_inspector: chatos_queue_observability::RabbitMqQueueInspector::new(
                config.rabbitmq_url.clone(),
            )
            .expect("queue inspector"),
            pressure: MemoryEnginePressureState::new(MemoryEnginePressurePolicy {
                level: PlatformPressureLevel::Normal,
                active_summary_concurrency: 1,
                reconcile_paused: false,
                refresh_interval: Duration::from_secs(1),
                queue_elevated_messages: 100,
                queue_critical_messages: 1_000,
            }),
            config,
        })
    }

    fn test_config() -> AppConfig {
        let mut internal_api_secrets = HashMap::new();
        internal_api_secrets.insert("user-service".to_string(), USER_SERVICE_SECRET.to_string());
        internal_api_secrets.insert("task-runner".to_string(), TASK_RUNNER_SECRET.to_string());
        AppConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            mongodb_uri: "mongodb://127.0.0.1:27017/test".to_string(),
            mongodb_database: "test".to_string(),
            ai_request_timeout_secs: 5,
            openai_api_key: None,
            openai_base_url: "https://api.openai.com/v1".to_string(),
            openai_model: "test".to_string(),
            openai_temperature: 0.0,
            api_enabled: true,
            worker_enabled: false,
            worker_interval_secs: 30,
            worker_max_threads_per_tick: 1,
            worker_summary_concurrency: 1,
            worker_rollup_concurrency: 1,
            worker_subject_memory_concurrency: 1,
            worker_reconcile_concurrency: 1,
            rabbitmq_url: "amqp://127.0.0.1/%2f".to_string(),
            rabbitmq_exchange: "memory_engine_test".to_string(),
            rabbitmq_reconnect_delay: Duration::from_millis(100),
            summary_queue: "memory_engine_test.summary".to_string(),
            summary_retry_queue: "memory_engine_test.summary.retry".to_string(),
            summary_dead_letter_queue: "memory_engine_test.summary.dead".to_string(),
            summary_max_delivery_attempts: 3,
            summary_retry_delay: Duration::from_millis(100),
            summary_outbox_reconcile_interval: Duration::from_secs(1),
            summary_outbox_batch_size: 10,
            rollup_queue: "memory_engine_test.rollup".to_string(),
            rollup_retry_queue: "memory_engine_test.rollup.retry".to_string(),
            rollup_dead_letter_queue: "memory_engine_test.rollup.dead".to_string(),
            rollup_max_delivery_attempts: 3,
            rollup_retry_delay: Duration::from_millis(100),
            rollup_outbox_reconcile_interval: Duration::from_secs(1),
            rollup_outbox_batch_size: 10,
            subject_memory_queue: "memory_engine_test.subject_memory".to_string(),
            subject_memory_retry_queue: "memory_engine_test.subject_memory.retry".to_string(),
            subject_memory_dead_letter_queue: "memory_engine_test.subject_memory.dead".to_string(),
            subject_memory_max_delivery_attempts: 3,
            subject_memory_retry_delay: Duration::from_millis(100),
            subject_memory_outbox_reconcile_interval: Duration::from_secs(1),
            subject_memory_outbox_batch_size: 10,
            subject_memory_lock_timeout_secs: 300,
            record_sync_lease_timeout_secs: 300,
            rollup_lock_timeout_secs: 300,
            internal_api_secrets,
            require_signed_internal_requests: true,
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout_ms: 300,
        }
    }
}
