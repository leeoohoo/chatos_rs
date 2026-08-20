// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{Method, Request};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::dependencies::{
    list_requirement_dependencies, list_work_item_dependencies, set_requirement_dependencies,
    set_work_item_dependencies,
};
use super::dependency_graph::{
    get_project_dependency_graph, get_requirement_dependency_graph, get_work_item_dependency_graph,
};
use super::execution_context::resolve_project_execution_context;
use super::harness_git_access::{
    sync_get_project_harness_git_access, sync_get_project_harness_git_branches,
};
use super::harness_mcp::harness_project_mcp_entrypoint;
use super::plan::get_project_plan;
use super::projects::{
    create_project, delete_project, get_project, get_project_profile, list_projects,
    update_project, upsert_project_profile,
};
use super::requirements::{
    create_requirement, create_requirement_document, delete_requirement, get_requirement,
    get_requirement_document, get_requirement_technical_overview, list_project_requirements,
    list_requirement_documents, update_requirement, update_requirement_document,
    upsert_requirement_technical_overview,
};
use super::run_workspace::{
    finalize_run_workspace, get_run_workspace_changes, integrate_run_workspace,
    prepare_run_workspace, promote_execution_workspace,
};
use super::sync::{
    sync_delete_execution_links, sync_get_project, sync_import_project, sync_list_execution_links,
    sync_list_projects, sync_requirement_execution_state, sync_task_runner_task_status,
    sync_task_runner_work_item_status,
};
use super::task_runner_links::{
    delete_task_runner_link, link_task_runner_task, list_task_runner_links,
};
use super::work_items::{
    create_work_item, delete_work_item, get_work_item, list_project_requirement_work_items,
    list_project_work_items, list_requirement_work_items, update_work_item,
};
use super::ApiError;
use crate::auth::{
    bearer_token_from_headers, list_agent_accounts_via_user_service, login_via_user_service,
    verify_token_via_user_service, AccessToken, CurrentUser,
};
use crate::models::*;
use crate::state::AppState;

mod mcp;

fn protected_api(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/agent-accounts", get(list_agent_accounts))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{project_id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/api/projects/{project_id}/profile",
            get(get_project_profile).put(upsert_project_profile),
        )
        .route(
            "/api/projects/{project_id}/requirements",
            get(list_project_requirements).post(create_requirement),
        )
        .route(
            "/api/projects/{project_id}/work-items",
            get(list_project_work_items),
        )
        .route(
            "/api/projects/{project_id}/requirements/{requirement_id}/work-items",
            get(list_project_requirement_work_items),
        )
        .route(
            "/api/projects/{project_id}/dependency-graph",
            get(get_project_dependency_graph),
        )
        .route("/api/projects/{project_id}/plan", get(get_project_plan))
        .route(
            "/api/requirements/{requirement_id}",
            get(get_requirement)
                .patch(update_requirement)
                .delete(delete_requirement),
        )
        .route(
            "/api/requirements/{requirement_id}/dependencies",
            get(list_requirement_dependencies).put(set_requirement_dependencies),
        )
        .route(
            "/api/requirements/{requirement_id}/dependency-graph",
            get(get_requirement_dependency_graph),
        )
        .route(
            "/api/requirements/{requirement_id}/technical-overview",
            get(get_requirement_technical_overview).put(upsert_requirement_technical_overview),
        )
        .route(
            "/api/requirements/{requirement_id}/documents",
            get(list_requirement_documents).post(create_requirement_document),
        )
        .route(
            "/api/requirements/{requirement_id}/documents/{document_id}",
            get(get_requirement_document).put(update_requirement_document),
        )
        .route(
            "/api/requirements/{requirement_id}/work-items",
            get(list_requirement_work_items).post(create_work_item),
        )
        .route(
            "/api/work-items/{work_item_id}",
            get(get_work_item)
                .patch(update_work_item)
                .delete(delete_work_item),
        )
        .route(
            "/api/work-items/{work_item_id}/dependencies",
            get(list_work_item_dependencies).put(set_work_item_dependencies),
        )
        .route(
            "/api/work-items/{work_item_id}/dependency-graph",
            get(get_work_item_dependency_graph),
        )
        .route(
            "/api/work-items/{work_item_id}/task-runner-links",
            get(list_task_runner_links).post(link_task_runner_task),
        )
        .route(
            "/api/work-items/{work_item_id}/task-runner-links/{link_id}",
            axum::routing::delete(delete_task_runner_link),
        )
        .route("/api/mcp/server", get(mcp::get_mcp_server_info))
        .route("/api/mcp/tools", get(mcp::list_mcp_tools))
        .route_layer(middleware::from_fn_with_state(state, require_auth))
}

pub fn build_public_router(state: AppState) -> Router {
    let body_limit = cloud_project_body_limit(&state);
    apply_common_layers(
        Router::new()
            .route("/api/health", get(health_handler))
            .route("/api/auth/login", post(login_handler))
            .route("/api/auth/agent-token", post(agent_token_handler))
            .route(
                "/api/skills/project-management",
                get(project_management_skill_handler),
            )
            .merge(protected_api(state.clone()))
            .route("/mcp", post(mcp::mcp_entrypoint))
            .with_state(state),
        body_limit,
        "public",
    )
}

pub fn build_internal_router(state: AppState) -> Router {
    let body_limit = cloud_project_body_limit(&state);
    apply_common_layers(
        Router::new()
            .route(
                "/api/chatos-sync/projects",
                get(sync_list_projects).post(sync_import_project),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}",
                get(sync_get_project),
            )
            .route(
                "/api/internal/projects/{project_id}/execution-context",
                get(resolve_project_execution_context),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/harness/git-access",
                get(sync_get_project_harness_git_access),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/harness/git-branches",
                get(sync_get_project_harness_git_branches),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/harness/mcp",
                post(harness_project_mcp_entrypoint),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/run-workspaces/{run_id}/prepare",
                post(prepare_run_workspace),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/run-workspaces/{run_id}/finalize-result",
                post(finalize_run_workspace),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/run-workspaces/{run_id}/changes",
                post(get_run_workspace_changes),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/run-workspaces/{run_id}/integrate",
                post(integrate_run_workspace),
            )
            .route(
                "/api/chatos-sync/projects/{project_id}/execution-workspaces/{execution_group_id}/promote",
                post(promote_execution_workspace),
            )
            .route(
                "/api/chatos-sync/work-items/{work_item_id}/task-runner-status",
                post(sync_task_runner_work_item_status),
            )
            .route(
                "/api/chatos-sync/task-runner/tasks/{task_runner_task_id}/status",
                post(sync_task_runner_task_status),
            )
            .route(
                "/api/chatos-sync/requirements/{requirement_id}/execution-state",
                post(sync_requirement_execution_state),
            )
            .route(
                "/api/chatos-sync/execution-links/query",
                post(sync_list_execution_links),
            )
            .route(
                "/api/chatos-sync/execution-links/delete",
                post(sync_delete_execution_links),
            )
            .route("/mcp", post(mcp::mcp_entrypoint))
            .with_state(state),
        body_limit,
        "internal",
    )
}

fn cloud_project_body_limit(state: &AppState) -> usize {
    state
        .config
        .cloud_project_max_zip_bytes
        .saturating_add(1024 * 1024)
}

fn apply_common_layers(router: Router, body_limit: usize, surface: &'static str) -> Router {
    router
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(middleware::from_fn(
            crate::trace_context::accept_remote_parent,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(move |request: &Request<axum::body::Body>| {
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
                })
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    let token = bearer_token_from_request(&request).map_err(ApiError::unauthorized)?;
    let user = verify_token_via_user_service(&state.config, &token)
        .await
        .map_err(ApiError::unauthorized)?;
    request.extensions_mut().insert(AccessToken(token));
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn bearer_token_from_request(request: &Request<axum::body::Body>) -> Result<String, String> {
    if chatos_service_runtime::query_has_nonempty_parameter(
        request.uri().query(),
        &["access_token", "token"],
    ) {
        return Err(
            "URL query access tokens are not supported; use Authorization header".to_string(),
        );
    }
    bearer_token_from_headers(request.headers()).map(ToOwned::to_owned)
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "project_management_service".to_string(),
    })
}

async fn login_handler(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    login_via_user_service(&state.config, &input)
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

async fn agent_token_handler(
    Json(_input): Json<AgentTokenRequest>,
) -> Result<Json<AgentTokenResponse>, ApiError> {
    Err(ApiError::forbidden(
        "project management agent token must be exchanged through user_service",
    ))
}

#[derive(Debug, Default, Deserialize)]
struct ProjectManagementSkillQuery {
    lang: Option<String>,
}

async fn project_management_skill_handler(
    Query(query): Query<ProjectManagementSkillQuery>,
) -> Json<super::ProjectManagementSkillResponse> {
    Json(
        if requested_project_management_skill_is_english(query.lang.as_deref()) {
            super::ProjectManagementSkillResponse {
                name: "project-management-mcp-agent-en-us",
                locale: "en-US",
                content: super::PROJECT_MANAGEMENT_MCP_SKILL_EN_US,
            }
        } else {
            super::ProjectManagementSkillResponse {
                name: "project-management-mcp-agent-zh-cn",
                locale: "zh-CN",
                content: super::PROJECT_MANAGEMENT_MCP_SKILL_ZH_CN,
            }
        },
    )
}

fn requested_project_management_skill_is_english(lang: Option<&str>) -> bool {
    matches!(
        lang.map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "en" | "en-us" | "english"
    )
}

async fn current_user_handler(Extension(user): Extension<CurrentUser>) -> Json<AuthUser> {
    Json(user.public_user())
}

async fn list_agent_accounts(
    State(state): State<AppState>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<Vec<AgentAccountListItem>>, ApiError> {
    list_agent_accounts_via_user_service(&state.config, access_token.0.as_str())
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use reqwest::StatusCode;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use super::{build_internal_router, build_public_router};
    use crate::config::AppConfig;
    use crate::state::AppState;

    async fn test_state() -> AppState {
        let mut internal_api_secrets = HashMap::new();
        for caller in [
            "chatos-backend",
            "task-runner",
            "project-service",
            "mcp-management-service",
        ] {
            internal_api_secrets.insert(caller.to_string(), format!("test-{caller}-secret"));
        }
        AppState::new_without_external_dependencies(AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            database_url: "mongodb://127.0.0.1:1/project_router_tests".to_string(),
            user_service_base_url: "http://127.0.0.1:1".to_string(),
            user_service_internal_base_url: "https://127.0.0.1:1".to_string(),
            user_service_internal_http_client: reqwest::Client::new(),
            user_service_request_timeout: Duration::from_millis(300),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:1".to_string(),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_service_request_timeout: Duration::from_millis(300),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024 * 1024,
            cloud_project_max_unpacked_bytes: 1024 * 1024,
            cloud_project_max_files: 100,
            cloud_project_git_timeout: Duration::from_millis(300),
            task_runner_base_url: None,
            task_runner_request_timeout: Duration::from_millis(300),
            task_runner_internal_secret: None,
            sync_secret: None,
            internal_api_secrets,
            require_signed_internal_requests: true,
        })
        .await
        .expect("build router test state")
    }

    async fn spawn_router(router: axum::Router) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind router test listener");
        let address = listener.local_addr().expect("router test address");
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve router under test");
        });
        (format!("http://{address}"), server)
    }

    #[tokio::test]
    async fn public_router_does_not_expose_internal_routes() {
        let (base_url, server) = spawn_router(build_public_router(test_state().await)).await;
        let client = reqwest::Client::new();

        for path in [
            "/api/chatos-sync/projects",
            "/api/internal/projects/project-1/execution-context",
            "/api/chatos-sync/projects/project-1/harness/git-branches",
        ] {
            let status = client
                .get(format!("{base_url}{path}"))
                .send()
                .await
                .expect("request public router")
                .status();
            assert_eq!(status, StatusCode::NOT_FOUND, "unexpected route: {path}");
        }

        let health_status = client
            .get(format!("{base_url}/api/health"))
            .send()
            .await
            .expect("request public health")
            .status();
        assert_eq!(health_status, StatusCode::OK);
        server.abort();
    }

    #[tokio::test]
    async fn internal_router_only_exposes_internal_control_plane() {
        let (base_url, server) = spawn_router(build_internal_router(test_state().await)).await;
        let client = reqwest::Client::new();

        for (method, path) in [
            (reqwest::Method::GET, "/api/health"),
            (reqwest::Method::POST, "/api/auth/login"),
            (reqwest::Method::GET, "/api/projects"),
        ] {
            let status = client
                .request(method, format!("{base_url}{path}"))
                .send()
                .await
                .expect("request internal router")
                .status();
            assert_eq!(status, StatusCode::NOT_FOUND, "unexpected route: {path}");
        }

        for (path, expected_status) in [
            ("/api/chatos-sync/projects", StatusCode::UNAUTHORIZED),
            (
                "/api/chatos-sync/projects/project-1/harness/git-branches",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "/api/internal/projects/project-1/execution-context",
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let status = client
                .get(format!("{base_url}{path}"))
                .send()
                .await
                .expect("request internal control plane")
                .status();
            assert_eq!(status, expected_status, "missing route: {path}");
        }

        let mcp_status = client
            .get(format!("{base_url}/mcp"))
            .send()
            .await
            .expect("request internal MCP route")
            .status();
        assert_eq!(mcp_status, StatusCode::METHOD_NOT_ALLOWED);
        server.abort();
    }
}
