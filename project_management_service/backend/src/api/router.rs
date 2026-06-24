use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::Value;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::ApiError;
use crate::auth::{
    bearer_token_from_headers, list_agent_accounts_via_user_service, login_via_user_service,
    verify_token_via_user_service, AccessToken, CurrentUser,
};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse, McpServerInfo};
use crate::models::*;
use crate::state::AppState;
use crate::task_runner_api_client;

pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/agent-accounts", get(list_agent_accounts))
        .route("/api/projects", get(list_projects).post(create_project))
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
            "/api/projects/:project_id/dependency-graph",
            get(get_project_dependency_graph),
        )
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
        .route("/api/mcp/server", get(get_mcp_server_info))
        .route("/api/mcp/tools", get(list_mcp_tools))
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
        .merge(protected_api)
        .route("/mcp", post(mcp_entrypoint))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
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

fn require_project_sync_secret(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state
        .config
        .sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(ApiError::forbidden("project sync secret is not configured"));
    };
    let provided = headers
        .get("x-project-service-sync-secret")
        .or_else(|| headers.get("x-chatos-callback-secret"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing project sync secret"))?;
    if provided != expected {
        return Err(ApiError::unauthorized("invalid project sync secret"));
    }
    Ok(())
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

async fn get_mcp_server_info() -> Json<McpServerInfo> {
    Json(mcp_server::server_info())
}

async fn list_mcp_tools() -> Json<Vec<Value>> {
    Json(mcp_server::tool_definitions())
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

async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let current_user = match task_runner_internal_mcp_user(&state.config, &headers) {
        Ok(Some(user)) => user,
        Ok(None) => {
            let token = match bearer_token_from_headers(&headers) {
                Ok(token) => token.to_string(),
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            let current_user = match verify_token_via_user_service(&state.config, &token).await {
                Ok(user) => user,
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            if !current_user.is_agent_account() {
                return Json(mcp_server::jsonrpc_error_response(
                    StatusCode::UNAUTHORIZED,
                    id,
                    "project management MCP requires an agent account token".to_string(),
                ));
            }
            let user_access_token = match require_user_access_token_from_headers(&headers) {
                Ok(value) => value,
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            let user = match verify_token_via_user_service(
                &state.config,
                user_access_token.as_str(),
            )
            .await
            {
                Ok(user) => user,
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            if !user.is_human_user() {
                return Json(mcp_server::jsonrpc_error_response(
                    StatusCode::UNAUTHORIZED,
                    id,
                    "project management MCP real user token must belong to a human user"
                        .to_string(),
                ));
            }
            if let Err(message) = ensure_same_owner_scope(&current_user, &user) {
                return Json(mcp_server::jsonrpc_error_response(
                    StatusCode::FORBIDDEN,
                    id,
                    message,
                ));
            }
            current_user.with_owner_identity_from(&user)
        }
        Err(err) => {
            return Json(mcp_server::jsonrpc_error_response(
                err.status,
                id,
                err.message,
            ));
        }
    };
    let project_id = project_id_from_headers(&headers);
    Json(mcp_server::handle_jsonrpc(state, current_user, project_id, request).await)
}

fn project_id_from_headers(headers: &HeaderMap) -> Option<String> {
    header_text(headers, "x-chatos-project-id")
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn require_user_access_token_from_headers(headers: &HeaderMap) -> Result<String, String> {
    user_access_token_from_headers(headers)?
        .ok_or_else(|| "project management MCP requires a real user token header".to_string())
}

fn user_access_token_from_headers(headers: &HeaderMap) -> Result<Option<String>, String> {
    for key in [
        "x-chatos-user-authorization",
        "x-user-service-authorization",
        "x-chatos-user-token",
    ] {
        let Some(value) = header_text(headers, key)? else {
            continue;
        };
        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim)
            .unwrap_or(value.as_str());
        if !token.is_empty() {
            return Ok(Some(token.to_string()));
        }
    }
    Ok(None)
}

fn ensure_same_owner_scope(agent_user: &CurrentUser, user: &CurrentUser) -> Result<(), String> {
    let agent_owner = agent_user
        .effective_owner_user_id()
        .ok_or_else(|| "agent token missing owner scope".to_string())?;
    let user_owner = user
        .effective_owner_user_id()
        .ok_or_else(|| "user token missing owner scope".to_string())?;
    if agent_owner == user_owner {
        Ok(())
    } else {
        Err("agent token and user token owner scope do not match".to_string())
    }
}

fn task_runner_internal_mcp_user(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
) -> Result<Option<CurrentUser>, ApiError> {
    let Some(provided_secret) =
        header_text(headers, "x-project-service-sync-secret").map_err(ApiError::bad_request)?
    else {
        return Ok(None);
    };
    let expected_secret = config
        .sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::forbidden("project sync secret is not configured"))?;
    if provided_secret != expected_secret {
        return Err(ApiError::unauthorized("invalid project sync secret"));
    }
    let task_profile = header_text(headers, "x-task-runner-task-profile")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::forbidden("task runner MCP sync branch requires task profile"))?;
    if !task_profile.eq_ignore_ascii_case("chatos_plan") {
        return Err(ApiError::forbidden(
            "task runner MCP sync branch only supports chatos_plan",
        ));
    }
    let owner_user_id = header_text(headers, "x-task-runner-owner-user-id")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::unauthorized("task runner MCP missing owner user id"))?;
    let owner_username = header_text(headers, "x-task-runner-owner-username")
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| owner_user_id.clone());
    let owner_display_name = header_text(headers, "x-task-runner-owner-display-name")
        .map_err(ApiError::bad_request)?
        .or_else(|| Some(owner_username.clone()))
        .unwrap_or_else(|| owner_user_id.clone());
    Ok(Some(CurrentUser {
        principal_type: "human_user".to_string(),
        id: owner_user_id.clone(),
        username: owner_username.clone(),
        display_name: owner_display_name.clone(),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id),
        owner_username: Some(owner_username),
        owner_display_name: Some(owner_display_name),
    }))
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Result<Option<String>, String> {
    headers
        .get(key)
        .map(|value| {
            value
                .to_str()
                .map(str::trim)
                .map(ToOwned::to_owned)
                .map_err(|_| format!("{key} header format is invalid"))
        })
        .transpose()
        .map(|value| value.filter(|value| !value.is_empty()))
}

async fn sync_list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<Vec<ProjectRecord>>, ApiError> {
    require_project_sync_secret(&state, &headers)?;
    state
        .store
        .list_all_projects(query.status)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn sync_import_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ImportProjectRequest>,
) -> Result<Json<ProjectRecord>, ApiError> {
    require_project_sync_secret(&state, &headers)?;
    state
        .store
        .import_project(input)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn sync_get_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectRecord>, ApiError> {
    require_project_sync_secret(&state, &headers)?;
    state
        .store
        .get_project(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))
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

#[derive(Debug, Default, Deserialize)]
struct ProjectListQuery {
    status: Option<ProjectStatus>,
}

async fn list_projects(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<Vec<ProjectRecord>>, ApiError> {
    state
        .store
        .list_projects(&user, query.status)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectRecord>), ApiError> {
    let project = state
        .store
        .create_project(input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn get_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    Ok(Json(project))
}

async fn update_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .update_project(&project_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    Ok(Json(project))
}

async fn delete_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .archive_project(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    Ok(Json(project))
}

async fn get_project_profile(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectProfileRecord>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let profile = state
        .store
        .get_project_profile(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| {
            let now = now_rfc3339();
            ProjectProfileRecord {
                project_id,
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                background: None,
                introduction: None,
                created_at: now.clone(),
                updated_at: now,
            }
        });
    Ok(Json(profile))
}

async fn upsert_project_profile(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpsertProjectProfileRequest>,
) -> Result<Json<ProjectProfileRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .upsert_project_profile(&project_id, input, &user)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Default, Deserialize)]
struct RequirementListQuery {
    status: Option<RequirementStatus>,
    keyword: Option<String>,
}

async fn list_project_requirements(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<RequirementListQuery>,
) -> Result<Json<Vec<RequirementRecord>>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    state
        .store
        .list_requirements(&project_id, query.status, query.keyword)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn create_requirement(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateRequirementRequest>,
) -> Result<(StatusCode, Json<RequirementRecord>), ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let requirement = state
        .store
        .create_requirement(&project_id, input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(requirement)))
}

async fn get_requirement(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<RequirementRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    Ok(Json(requirement))
}

async fn update_requirement(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateRequirementRequest>,
) -> Result<Json<RequirementRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .update_requirement(&requirement_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("需求不存在: {requirement_id}")))
}

async fn delete_requirement(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<RequirementRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .archive_requirement(&requirement_id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("需求不存在: {requirement_id}")))
}

async fn list_requirement_dependencies(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<RequirementDependencyRecord>>, ApiError> {
    require_requirement_access(&state, &requirement_id, &user).await?;
    state
        .store
        .list_requirement_dependencies(&requirement_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn set_requirement_dependencies(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<SetRequirementDependenciesRequest>,
) -> Result<Json<Vec<RequirementDependencyRecord>>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .set_requirement_dependencies(&requirement_id, input.prerequisite_requirement_ids)
        .await
        .map_err(ApiError::bad_request)?;
    state
        .store
        .list_requirement_dependencies(&requirement_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn get_requirement_technical_overview(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<RequirementDocumentRecord>, ApiError> {
    require_requirement_access(&state, &requirement_id, &user).await?;
    let doc = state
        .store
        .get_requirement_document(&requirement_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| {
            let now = now_rfc3339();
            RequirementDocumentRecord {
                id: String::new(),
                requirement_id,
                doc_type: "technical_overview".to_string(),
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                title: "实现技术总体文档".to_string(),
                format: "markdown".to_string(),
                content: String::new(),
                version: 0,
                created_at: now.clone(),
                updated_at: now,
            }
        });
    Ok(Json(doc))
}

async fn upsert_requirement_technical_overview(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpsertRequirementDocumentRequest>,
) -> Result<Json<RequirementDocumentRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .upsert_requirement_document(&requirement_id, input, &user)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Default, Deserialize)]
struct WorkItemListQuery {
    status: Option<ProjectWorkItemStatus>,
    keyword: Option<String>,
}

async fn list_project_work_items(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<WorkItemListQuery>,
) -> Result<Json<Vec<ProjectWorkItemRecord>>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    state
        .store
        .list_work_items_by_project(&project_id, query.status, query.keyword)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn list_requirement_work_items(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<ProjectWorkItemRecord>>, ApiError> {
    require_requirement_access(&state, &requirement_id, &user).await?;
    state
        .store
        .list_work_items_by_requirement(&requirement_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn create_work_item(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateProjectWorkItemRequest>,
) -> Result<(StatusCode, Json<ProjectWorkItemRecord>), ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let item = state
        .store
        .create_work_item(&requirement, input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn get_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectWorkItemRecord>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    Ok(Json(item))
}

async fn update_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectWorkItemRequest>,
) -> Result<Json<ProjectWorkItemRecord>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .update_work_item(&work_item_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("项目工作项不存在: {work_item_id}")))
}

async fn delete_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectWorkItemRecord>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .archive_work_item(&work_item_id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("项目工作项不存在: {work_item_id}")))
}

async fn list_work_item_dependencies(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<WorkItemDependencyRecord>>, ApiError> {
    require_work_item_access(&state, &work_item_id, &user).await?;
    state
        .store
        .list_work_item_dependencies(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn set_work_item_dependencies(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<SetWorkItemDependenciesRequest>,
) -> Result<Json<Vec<WorkItemDependencyRecord>>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .set_work_item_dependencies(&work_item_id, input.prerequisite_work_item_ids)
        .await
        .map_err(ApiError::bad_request)?;
    state
        .store
        .list_work_item_dependencies(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn list_task_runner_links(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<ProjectWorkItemTaskRunnerLinkRecord>>, ApiError> {
    require_work_item_access(&state, &work_item_id, &user).await?;
    state
        .store
        .list_task_runner_links(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn link_task_runner_task(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<LinkTaskRunnerTaskRequest>,
) -> Result<(StatusCode, Json<ProjectWorkItemTaskRunnerLinkRecord>), ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let link = state
        .store
        .upsert_task_runner_link(&work_item_id, input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(link)))
}

async fn delete_task_runner_link(
    Path((work_item_id, link_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let deleted = state
        .store
        .delete_task_runner_link(&work_item_id, &link_id)
        .await
        .map_err(ApiError::bad_request)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "TaskRunner 关联不存在: {link_id}"
        )))
    }
}

async fn create_task_runner_task_from_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
    Json(mut input): Json<CreateTaskRunnerTaskFromWorkItemRequest>,
) -> Result<(StatusCode, Json<CreateTaskRunnerTaskFromWorkItemResponse>), ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    if input.prerequisite_task_ids.is_none() {
        input.prerequisite_task_ids = Some(
            derive_task_runner_prerequisite_task_ids(&state, &work_item_id)
                .await
                .map_err(ApiError::bad_request)?,
        );
    }
    let task = task_runner_api_client::create_task_from_work_item(
        &state.config,
        access_token.0.as_str(),
        &item,
        input,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let link = state
        .store
        .upsert_task_runner_link(
            &work_item_id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: task.last_run_id.clone(),
                link_type: Some("execution".to_string()),
            },
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateTaskRunnerTaskFromWorkItemResponse { task, link }),
    ))
}

async fn derive_task_runner_prerequisite_task_ids(
    state: &AppState,
    work_item_id: &str,
) -> Result<Vec<String>, String> {
    let mut task_ids = Vec::new();
    for dependency in state
        .store
        .list_work_item_dependencies(work_item_id)
        .await?
    {
        for link in state
            .store
            .list_task_runner_links(&dependency.prerequisite_work_item_id)
            .await?
        {
            task_ids.push(link.task_runner_task_id);
        }
    }
    task_ids.sort();
    task_ids.dedup();
    Ok(task_ids)
}

async fn get_requirement_dependency_graph(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<DependencyGraphResponse>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let deps = state
        .store
        .list_requirement_dependencies(&requirement_id)
        .await
        .map_err(ApiError::bad_request)?;
    let mut nodes = vec![requirement_node(&requirement)];
    let mut edges = Vec::new();
    let mut blocked_by = Vec::new();
    for dep in deps {
        if let Some(prereq) = state
            .store
            .get_requirement(&dep.prerequisite_requirement_id)
            .await
            .map_err(ApiError::bad_request)?
        {
            if prereq.status != RequirementStatus::Done {
                blocked_by.push(requirement_node(&prereq));
            }
            edges.push(DependencyGraphEdge {
                from: format!("requirement:{}", prereq.id),
                to: format!("requirement:{}", requirement.id),
                edge_type: dep.relation_type,
            });
            nodes.push(requirement_node(&prereq));
        }
    }
    Ok(Json(DependencyGraphResponse {
        root_id: Some(format!("requirement:{requirement_id}")),
        ready: blocked_by.is_empty(),
        nodes,
        edges,
        blocked_by,
    }))
}

async fn get_work_item_dependency_graph(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<DependencyGraphResponse>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let deps = state
        .store
        .list_work_item_dependencies(&work_item_id)
        .await
        .map_err(ApiError::bad_request)?;
    let mut nodes = vec![work_item_node(&item)];
    let mut edges = Vec::new();
    let mut blocked_by = Vec::new();
    for dep in deps {
        if let Some(prereq) = state
            .store
            .get_work_item(&dep.prerequisite_work_item_id)
            .await
            .map_err(ApiError::bad_request)?
        {
            if prereq.status != ProjectWorkItemStatus::Done {
                blocked_by.push(work_item_node(&prereq));
            }
            edges.push(DependencyGraphEdge {
                from: format!("work_item:{}", prereq.id),
                to: format!("work_item:{}", item.id),
                edge_type: dep.relation_type,
            });
            nodes.push(work_item_node(&prereq));
        }
    }
    Ok(Json(DependencyGraphResponse {
        root_id: Some(format!("work_item:{work_item_id}")),
        ready: blocked_by.is_empty(),
        nodes,
        edges,
        blocked_by,
    }))
}

async fn get_project_dependency_graph(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<DependencyGraphResponse>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let requirements = state
        .store
        .list_requirements(&project_id, None, None)
        .await
        .map_err(ApiError::bad_request)?;
    let work_items = state
        .store
        .list_work_items_by_project(&project_id, None, None)
        .await
        .map_err(ApiError::bad_request)?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for requirement in &requirements {
        nodes.push(requirement_node(requirement));
        for dep in state
            .store
            .list_requirement_dependencies(&requirement.id)
            .await
            .map_err(ApiError::bad_request)?
        {
            edges.push(DependencyGraphEdge {
                from: format!("requirement:{}", dep.prerequisite_requirement_id),
                to: format!("requirement:{}", dep.requirement_id),
                edge_type: dep.relation_type,
            });
        }
    }
    for item in &work_items {
        nodes.push(work_item_node(item));
        edges.push(DependencyGraphEdge {
            from: format!("requirement:{}", item.requirement_id),
            to: format!("work_item:{}", item.id),
            edge_type: "contains".to_string(),
        });
        for dep in state
            .store
            .list_work_item_dependencies(&item.id)
            .await
            .map_err(ApiError::bad_request)?
        {
            edges.push(DependencyGraphEdge {
                from: format!("work_item:{}", dep.prerequisite_work_item_id),
                to: format!("work_item:{}", dep.work_item_id),
                edge_type: dep.relation_type,
            });
        }
    }

    Ok(Json(DependencyGraphResponse {
        root_id: Some(format!("project:{project_id}")),
        nodes,
        edges,
        blocked_by: Vec::new(),
        ready: true,
    }))
}

async fn require_project_access(
    state: &AppState,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectRecord, ApiError> {
    let project = state
        .store
        .get_project(project_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    if user.can_access_owned_resource(project.owner_user_id.as_deref()) {
        Ok(project)
    } else {
        Err(ApiError::forbidden("无权访问该项目"))
    }
}

async fn require_requirement_access(
    state: &AppState,
    requirement_id: &str,
    user: &CurrentUser,
) -> Result<RequirementRecord, ApiError> {
    let requirement = state
        .store
        .get_requirement(requirement_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("需求不存在: {requirement_id}")))?;
    require_project_access(state, &requirement.project_id, user).await?;
    Ok(requirement)
}

async fn require_work_item_access(
    state: &AppState,
    work_item_id: &str,
    user: &CurrentUser,
) -> Result<ProjectWorkItemRecord, ApiError> {
    let item = state
        .store
        .get_work_item(work_item_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目工作项不存在: {work_item_id}")))?;
    require_project_access(state, &item.project_id, user).await?;
    Ok(item)
}

fn ensure_project_writable(project: &ProjectRecord) -> Result<(), ApiError> {
    if project.status == ProjectStatus::Archived {
        Err(ApiError::bad_request("项目已归档，不能继续写入"))
    } else {
        Ok(())
    }
}

fn requirement_node(requirement: &RequirementRecord) -> DependencyGraphNode {
    DependencyGraphNode {
        id: format!("requirement:{}", requirement.id),
        raw_id: requirement.id.clone(),
        node_type: "requirement".to_string(),
        label: requirement.title.clone(),
        status: requirement.status.as_str().to_string(),
        parent_id: requirement.parent_requirement_id.clone(),
    }
}

fn work_item_node(item: &ProjectWorkItemRecord) -> DependencyGraphNode {
    DependencyGraphNode {
        id: format!("work_item:{}", item.id),
        raw_id: item.id.clone(),
        node_type: "work_item".to_string(),
        label: item.title.clone(),
        status: item.status.as_str().to_string(),
        parent_id: Some(item.requirement_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;
    use crate::config::AppConfig;

    fn test_principal(principal_type: &str, id: &str, owner_user_id: Option<&str>) -> CurrentUser {
        CurrentUser {
            principal_type: principal_type.to_string(),
            id: id.to_string(),
            username: format!("{id}-name"),
            display_name: format!("{id} display"),
            role: UserRole::Agent,
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: owner_user_id.map(|value| format!("{value}-name")),
            owner_display_name: owner_user_id.map(|value| format!("{value} display")),
        }
    }

    #[test]
    fn mcp_requires_real_user_token_header() {
        let headers = HeaderMap::new();
        let message = require_user_access_token_from_headers(&headers).unwrap_err();
        assert_eq!(
            message,
            "project management MCP requires a real user token header"
        );
    }

    #[test]
    fn mcp_real_user_token_header_is_read_from_bearer_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-chatos-user-authorization",
            HeaderValue::from_static("Bearer real-user-token"),
        );

        assert_eq!(
            require_user_access_token_from_headers(&headers).unwrap(),
            "real-user-token"
        );
    }

    #[test]
    fn mcp_agent_and_user_tokens_must_share_owner_scope() {
        let agent = test_principal("agent_account", "agent-1", Some("user-1"));
        let same_owner = test_principal("human_user", "user-1", Some("user-1"));
        let other_owner = test_principal("human_user", "user-2", Some("user-2"));
        let missing_owner = test_principal("agent_account", "agent-2", None);

        assert!(ensure_same_owner_scope(&agent, &same_owner).is_ok());
        assert_eq!(
            ensure_same_owner_scope(&agent, &other_owner).unwrap_err(),
            "agent token and user token owner scope do not match"
        );
        assert_eq!(
            ensure_same_owner_scope(&missing_owner, &same_owner).unwrap_err(),
            "agent token missing owner scope"
        );
    }

    #[test]
    fn task_runner_internal_mcp_user_accepts_valid_plan_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("sync-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );
        headers.insert(
            "x-task-runner-owner-username",
            HeaderValue::from_static("owner-name"),
        );
        headers.insert(
            "x-task-runner-owner-display-name",
            HeaderValue::from_static("Owner Name"),
        );

        let user = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect("internal user")
            .expect("present");

        assert_eq!(user.principal_type, "human_user");
        assert_eq!(user.id, "user-1");
        assert_eq!(user.username, "owner-name");
        assert_eq!(user.display_name, "Owner Name");
        assert_eq!(user.effective_owner_user_id(), Some("user-1"));
    }

    #[test]
    fn task_runner_internal_mcp_user_rejects_non_plan_profile() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("sync-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("default"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let err = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect_err("non-plan profile should fail");

        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(
            err.message,
            "task runner MCP sync branch only supports chatos_plan"
        );
    }

    #[test]
    fn task_runner_internal_mcp_user_rejects_invalid_sync_secret() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("wrong-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let err = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect_err("invalid secret should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "invalid project sync secret");
    }

    #[test]
    fn task_runner_internal_mcp_user_requires_owner_user_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("sync-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );

        let err = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect_err("missing owner user id should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "task runner MCP missing owner user id");
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "sqlite::memory:".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            task_runner_base_url: Some("http://127.0.0.1:39090".to_string()),
            task_runner_request_timeout: Duration::from_millis(10_000),
            sync_secret: Some("sync-secret".to_string()),
        }
    }
}
