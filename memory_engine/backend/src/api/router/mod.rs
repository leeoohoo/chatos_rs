// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{middleware, Router};

use crate::api::{memory_auth, operator_auth};
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
            .merge(admin::routes().route_layer(middleware::from_fn_with_state(
                protected_state.clone(),
                memory_auth::require_memory_auth,
            )))
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
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::{build_internal_router, build_public_router};
    use crate::config::AppConfig;
    use crate::pressure::{
        MemoryEnginePressurePolicy, MemoryEnginePressureState, PlatformPressureLevel,
    };
    use crate::state::{AppState, MemoryEngineRuntimeStats};

    const USER_SERVICE_SECRET: &str = "test-user-service-memory-engine-signing-secret";

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
        let response = build_public_router(test_state().await)
            .oneshot(
                Request::get("/api/memory-engine/v1/threads/thread-a")
                    .header("x-memory-caller", "task-runner")
                    .header("x-memory-internal-token", "retired-service-token")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_data_route_rejects_retired_task_runner_caller() {
        let response = build_internal_router(test_state().await)
            .oneshot(thread_upsert_request(
                "task-runner",
                "retired-service-token".to_string(),
            ))
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    fn thread_upsert_request(caller: &str, token: String) -> Request<Body> {
        Request::put("/api/memory-engine/v1/threads/thread-a")
            .header("x-memory-caller", caller)
            .header("x-memory-internal-token", token)
            .header("content-type", "application/json")
            .body(Body::empty())
            .expect("request")
    }

    async fn test_state() -> Arc<AppState> {
        let config = test_config();
        let pool = PgPoolOptions::new()
            .connect_lazy(config.database_url.as_str())
            .expect("PostgreSQL pool");
        Arc::new(AppState {
            pool,
            user_service_http: reqwest::Client::new(),
            runtime_stats: Arc::new(MemoryEngineRuntimeStats::default()),
            pressure: MemoryEnginePressureState::new(MemoryEnginePressurePolicy {
                level: PlatformPressureLevel::Normal,
                active_summary_concurrency: 1,
                reconcile_paused: false,
                refresh_interval: Duration::from_secs(1),
                queue_elevated_messages: 100,
                queue_critical_messages: 1_000,
            }),
            cloud_agent_store: chatos_cloud_agent_runtime::CloudAgentStateStore::memory(),
            config,
        })
    }

    fn test_config() -> AppConfig {
        let mut config = crate::config::test_app_config("postgresql://127.0.0.1:5432/test");
        config
            .internal_api_secrets
            .insert("user-service".to_string(), USER_SERVICE_SECRET.to_string());
        config
    }
}
