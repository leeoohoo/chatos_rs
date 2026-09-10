// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::State, http::HeaderMap, Json};
use chatos_mcp_management_sdk::{AuthorizeProjectContextRequest, ProjectContextAuthorization};

use crate::{auth::require_internal_request_identity, error::ApiError, state::AppState};

pub(super) async fn authorize_project_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AuthorizeProjectContextRequest>,
) -> Result<Json<ProjectContextAuthorization>, ApiError> {
    let identity =
        require_internal_request_identity(&state.config, &headers, "project-context.authorize")?;
    identity.require_signed_trace_id()?;
    identity.require_owner(&request.owner_user_id)?;
    if !matches!(identity.caller.as_str(), "chatos" | "task-runner") {
        return Err(ApiError::forbidden(
            "caller may not authorize client project snapshots",
        ));
    }
    request.snapshot.validate().map_err(ApiError::bad_request)?;
    crate::project_context::authorize_client_project_context(
        &state.config,
        &request.owner_user_id,
        &request.snapshot,
    )
    .await
    .map(Json)
    .map_err(ApiError::bad_gateway)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        response::IntoResponse,
        routing::post,
        Router,
    };
    use chatos_mcp_management_sdk::{ClientProjectContextSnapshot, ClientProjectExecutionTarget};
    use tower::ServiceExt;

    use super::*;
    use crate::{api::build_internal_router, config::AppConfig};

    const SCOPE: &str = "project-context.authorize";
    const PATH: &str = "/api/internal/project-context/authorize";
    const CONNECTOR_SECRET: &str = "a-long-local-connector-secret";

    fn snapshot() -> ClientProjectContextSnapshot {
        ClientProjectContextSnapshot {
            schema_version: 1,
            project_id: "client-only-project".into(),
            project_name: "Local project".into(),
            project_revision: 3,
            execution_target: ClientProjectExecutionTarget {
                device_id: "device".into(),
                workspace_id: "workspace".into(),
                relative_root: "repo".into(),
            },
        }
    }

    fn request(
        caller: &str,
        scope: &str,
        signed_owner: Option<&str>,
        requested_owner: &str,
        snapshot: ClientProjectContextSnapshot,
    ) -> Request<Body> {
        let config = AppConfig::test();
        let secret = config.internal_api_secrets.get(caller).unwrap();
        let token = match signed_owner {
            Some(owner) => chatos_service_runtime::issue_internal_service_token_for_owner(
                secret,
                caller,
                "mcp-management-service",
                scope,
                60,
                owner,
            ),
            None => chatos_service_runtime::issue_internal_service_token(
                secret,
                caller,
                "mcp-management-service",
                scope,
                60,
            ),
        }
        .unwrap();
        Request::builder()
            .method("POST")
            .uri(PATH)
            .header("content-type", "application/json")
            .header("x-mcp-management-caller-service", caller)
            .header("x-mcp-management-internal-token", token)
            .body(Body::from(
                serde_json::to_vec(&AuthorizeProjectContextRequest {
                    owner_user_id: requested_owner.into(),
                    snapshot,
                })
                .unwrap(),
            ))
            .unwrap()
    }

    struct Upstream {
        task: tokio::task::JoinHandle<()>,
        calls: Arc<AtomicUsize>,
    }

    impl Drop for Upstream {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn fixture(mode: &'static str) -> (Router, Upstream) {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let upstream = Router::new().route(
            "/api/local-connectors/project-context/authorize",
            post(
                move |headers: HeaderMap, Json(snapshot): Json<ClientProjectContextSnapshot>| {
                    let observed = observed.clone();
                    async move {
                        observed.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(
                            headers["x-local-connector-caller"],
                            "mcp-management-service"
                        );
                        let claims = chatos_service_runtime::verify_internal_service_token(
                            headers["x-local-connector-internal-token"]
                                .to_str()
                                .unwrap(),
                            CONNECTOR_SECRET,
                            "mcp-management-service",
                            "local-connector-service",
                            SCOPE,
                        )
                        .unwrap();
                        assert_eq!(claims.owner_user_id.as_deref(), Some("alice"));
                        assert_eq!(headers["x-local-connector-owner-user-id"], "alice");
                        let mut authorization = ProjectContextAuthorization {
                            snapshot,
                            owner_user_id: "alice".into(),
                            workspace_fingerprint: "workspace-binding-v1".into(),
                        };
                        match mode {
                            "owner" => authorization.owner_user_id = "bob".into(),
                            "snapshot" => {
                                authorization.snapshot.execution_target.device_id = "other".into()
                            }
                            "binding" => authorization.workspace_fingerprint.clear(),
                            "oversized" => return "x".repeat(20 * 1024).into_response(),
                            "denied" => {
                                return (StatusCode::FORBIDDEN, "upstream private diagnostic")
                                    .into_response()
                            }
                            _ => {}
                        }
                        Json(authorization).into_response()
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });
        let mut config = AppConfig::test();
        config.local_connector_service_base_url = format!("http://{address}");
        let router = build_internal_router(AppState::new(config).await.unwrap());
        (router, Upstream { task, calls })
    }

    #[tokio::test]
    async fn authorizes_client_snapshot_over_owner_bound_local_connector_hop() {
        let (router, upstream) = fixture("valid").await;
        for caller in ["task-runner", "chatos"] {
            let expected = snapshot();
            let response = router
                .clone()
                .oneshot(request(
                    caller,
                    SCOPE,
                    Some("alice"),
                    "alice",
                    expected.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
            let result: ProjectContextAuthorization = serde_json::from_slice(&body).unwrap();
            result.validate_expected("alice", &expected).unwrap();
            assert_eq!(result.workspace_fingerprint, "workspace-binding-v1");
        }
        assert_eq!(upstream.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn rejects_missing_or_mismatched_signed_owner_scope_and_caller_before_upstream() {
        let (router, upstream) = fixture("valid").await;
        for (caller, scope, owner, requested, status) in [
            (
                "task-runner",
                SCOPE,
                None,
                "alice",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "task-runner",
                SCOPE,
                Some("alice"),
                "bob",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "task-runner",
                "catalog.read",
                Some("alice"),
                "alice",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "configuration-center",
                SCOPE,
                Some("alice"),
                "alice",
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(request(caller, scope, owner, requested, snapshot()))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        assert_eq!(upstream.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejects_path_traversal_before_upstream() {
        let (router, upstream) = fixture("valid").await;
        let mut invalid = snapshot();
        invalid.execution_target.relative_root = "../private".into();
        let response = router
            .oneshot(request(
                "task-runner",
                SCOPE,
                Some("alice"),
                "alice",
                invalid,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(upstream.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn fails_closed_on_substitution_denial_and_oversized_response() {
        for mode in ["owner", "snapshot", "binding", "denied", "oversized"] {
            let (router, upstream) = fixture(mode).await;
            let response = router
                .oneshot(request(
                    "task-runner",
                    SCOPE,
                    Some("alice"),
                    "alice",
                    snapshot(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_GATEWAY, "{mode}");
            let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
            assert!(!String::from_utf8_lossy(&body).contains("upstream private diagnostic"));
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
        }
    }
}
