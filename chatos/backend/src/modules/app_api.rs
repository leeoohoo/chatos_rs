// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Router;

use super::{conversation_runtime, memory, platform_admin, remote_execution, workspace};

pub fn public_routes() -> Router {
    Router::new()
        .merge(platform_admin::public_routes())
        .merge(conversation_runtime::public_routes())
}

pub fn internal_routes() -> Router {
    Router::new()
        .merge(crate::api::agent_chat::internal_router())
        .merge(crate::api::mcp_management::router())
}

pub fn protected_routes() -> Router {
    Router::new()
        .merge(conversation_runtime::routes())
        .merge(memory::routes())
        .merge(platform_admin::protected_routes())
        .merge(remote_execution::routes())
        .merge(workspace::routes())
}

#[cfg(test)]
mod tests {
    use super::{internal_routes, public_routes};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn public_router_does_not_register_internal_service_routes() {
        for path in [
            "/api/agent/chat/task-runner/callback",
            "/internal/mcp-management/mcp/ask_user",
            "/internal/mcp-management/mcp/browser_tools/sessions/session-1/close",
        ] {
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
    async fn internal_router_registers_internal_service_routes() {
        for path in [
            "/api/agent/chat/task-runner/callback",
            "/internal/mcp-management/mcp/ask_user",
            "/internal/mcp-management/mcp/browser_tools/sessions/session-1/close",
        ] {
            let response = internal_routes()
                .oneshot(
                    Request::post(path)
                        .body(Body::empty())
                        .expect("build request"),
                )
                .await
                .expect("route request");
            assert_ne!(response.status(), StatusCode::NOT_FOUND, "path={path}");
        }
    }
}
