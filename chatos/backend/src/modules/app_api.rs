// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Router;

use super::{conversation_runtime, memory, platform_admin, remote_execution, workspace};

pub fn public_routes() -> Router {
    Router::new()
        .merge(platform_admin::public_routes())
        .merge(conversation_runtime::public_routes())
}

pub fn protected_routes() -> Router {
    Router::new()
        .merge(crate::api::model_gateway::router())
        .merge(conversation_runtime::routes())
        .merge(memory::routes())
        .merge(platform_admin::protected_routes())
        .merge(remote_execution::routes())
        .merge(workspace::routes())
}

#[cfg(test)]
mod tests {
    use super::{protected_routes, public_routes};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn public_router_does_not_register_internal_service_routes() {
        for path in ["/internal/retired-service"] {
            let response = public_routes()
                .oneshot(
                    Request::post(path)
                        .body(Body::empty())
                        .expect("build request"),
                )
                .await
                .expect("route request");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "path={path}");
        }
    }

    #[tokio::test]
    async fn model_gateway_routes_are_protected_only() {
        for (method, path) in [
            ("GET", "/api/model-gateway/descriptors/model-1"),
            ("POST", "/api/model-gateway/stream"),
        ] {
            let public_response = public_routes()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .expect("build request"),
                )
                .await
                .expect("route request");
            assert_eq!(public_response.status(), StatusCode::NOT_FOUND);

            let protected_response = protected_routes()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .expect("build request"),
                )
                .await
                .expect("route request");
            assert_eq!(protected_response.status(), StatusCode::UNAUTHORIZED);
        }
    }
}
