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
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::dependencies::{
    list_requirement_dependencies, list_work_item_dependencies, set_requirement_dependencies,
    set_work_item_dependencies,
};
use super::dependency_graph::{
    get_project_dependency_graph, get_requirement_dependency_graph, get_work_item_dependency_graph,
};
use super::plan::get_project_plan;
use super::projects::{
    create_cloud_project, create_project, delete_project, get_project, get_project_profile,
    list_projects, update_project, upsert_project_profile,
};
use super::requirements::{
    create_requirement, create_requirement_document, delete_requirement, get_requirement,
    get_requirement_document, get_requirement_technical_overview, list_project_requirements,
    list_requirement_documents, update_requirement, update_requirement_document,
    upsert_requirement_technical_overview,
};
use super::sync::{
    sync_get_project, sync_import_project, sync_list_projects, sync_requirement_execution_state,
    sync_task_runner_work_item_status,
};
use super::task_runner_links::{
    create_task_runner_task_from_work_item, delete_task_runner_link,
    get_task_runner_execution_options, link_task_runner_task, list_task_runner_links,
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

pub fn build_router(state: AppState) -> Router {
    let cloud_project_body_limit = state
        .config
        .cloud_project_max_zip_bytes
        .saturating_add(1024 * 1024);
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/agent-accounts", get(list_agent_accounts))
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/projects/cloud", post(create_cloud_project))
        .route(
            "/api/projects/:project_id",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/api/projects/:project_id/profile",
            get(get_project_profile).put(upsert_project_profile),
        )
        .route(
            "/api/projects/:project_id/requirements",
            get(list_project_requirements).post(create_requirement),
        )
        .route(
            "/api/projects/:project_id/work-items",
            get(list_project_work_items),
        )
        .route(
            "/api/projects/:project_id/requirements/:requirement_id/work-items",
            get(list_project_requirement_work_items),
        )
        .route(
            "/api/projects/:project_id/dependency-graph",
            get(get_project_dependency_graph),
        )
        .route("/api/projects/:project_id/plan", get(get_project_plan))
        .route(
            "/api/requirements/:requirement_id",
            get(get_requirement)
                .patch(update_requirement)
                .delete(delete_requirement),
        )
        .route(
            "/api/requirements/:requirement_id/dependencies",
            get(list_requirement_dependencies).put(set_requirement_dependencies),
        )
        .route(
            "/api/requirements/:requirement_id/dependency-graph",
            get(get_requirement_dependency_graph),
        )
        .route(
            "/api/requirements/:requirement_id/technical-overview",
            get(get_requirement_technical_overview).put(upsert_requirement_technical_overview),
        )
        .route(
            "/api/requirements/:requirement_id/documents",
            get(list_requirement_documents).post(create_requirement_document),
        )
        .route(
            "/api/requirements/:requirement_id/documents/:document_id",
            get(get_requirement_document).put(update_requirement_document),
        )
        .route(
            "/api/requirements/:requirement_id/work-items",
            get(list_requirement_work_items).post(create_work_item),
        )
        .route(
            "/api/work-items/:work_item_id",
            get(get_work_item)
                .patch(update_work_item)
                .delete(delete_work_item),
        )
        .route(
            "/api/work-items/:work_item_id/dependencies",
            get(list_work_item_dependencies).put(set_work_item_dependencies),
        )
        .route(
            "/api/work-items/:work_item_id/dependency-graph",
            get(get_work_item_dependency_graph),
        )
        .route(
            "/api/work-items/:work_item_id/task-runner-links",
            get(list_task_runner_links).post(link_task_runner_task),
        )
        .route(
            "/api/work-items/:work_item_id/task-runner-links/:link_id",
            axum::routing::delete(delete_task_runner_link),
        )
        .route(
            "/api/work-items/:work_item_id/task-runner-task",
            post(create_task_runner_task_from_work_item),
        )
        .route(
            "/api/task-runner/execution-options",
            get(get_task_runner_execution_options),
        )
        .route("/api/mcp/server", get(mcp::get_mcp_server_info))
        .route("/api/mcp/tools", get(mcp::list_mcp_tools))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/agent-token", post(agent_token_handler))
        .route(
            "/api/skills/project-management",
            get(project_management_skill_handler),
        )
        .route(
            "/api/chatos-sync/projects",
            get(sync_list_projects).post(sync_import_project),
        )
        .route(
            "/api/chatos-sync/projects/:project_id",
            get(sync_get_project),
        )
        .route(
            "/api/chatos-sync/work-items/:work_item_id/task-runner-status",
            post(sync_task_runner_work_item_status),
        )
        .route(
            "/api/chatos-sync/requirements/:requirement_id/execution-state",
            post(sync_requirement_execution_state),
        )
        .merge(protected_api)
        .route("/mcp", post(mcp::mcp_entrypoint))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::DEBUG))
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(DefaultBodyLimit::max(cloud_project_body_limit))
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
    bearer_token_from_headers(request.headers())
        .map(ToOwned::to_owned)
        .or_else(|_| {
            token_from_query(request.uri().query()).ok_or_else(|| "缺少登录令牌".to_string())
        })
}

fn token_from_query(query: Option<&str>) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        ((key == "access_token" || key == "token") && !value.is_empty()).then(|| value.to_string())
    })
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
