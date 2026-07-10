// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;
use uuid::Uuid;

use crate::auth::{
    bearer_token_from_headers, login_via_user_service, verify_token_via_user_service, AccessToken,
};
use crate::models::*;
use crate::state::AppState;
use crate::store::{normalized, now_rfc3339};

const ALLOWED_INTERNAL_CALLER_SERVICES: &[&str] =
    &["task-runner", "project-service", "local-connector-service"];

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}

pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/mcps", get(list_mcps).post(create_mcp))
        .route(
            "/api/mcps/:mcp_id",
            get(get_mcp).patch(update_mcp).delete(delete_mcp),
        )
        .route("/api/mcps/:mcp_id/check", post(check_mcp))
        .route("/api/skills", get(list_skills).post(create_skill))
        .route(
            "/api/skills/:skill_id",
            get(get_skill).patch(update_skill).delete(delete_skill),
        )
        .route("/api/skills/:skill_id/check", post(check_skill))
        .route(
            "/api/skill-packages",
            get(list_skill_packages).post(create_skill_package),
        )
        .route(
            "/api/skill-packages/:package_id",
            get(get_skill_package)
                .patch(update_skill_package)
                .delete(delete_skill_package),
        )
        .route(
            "/api/system-agents",
            get(list_system_agents).post(create_system_agent),
        )
        .route("/api/system-agents/:agent_key", patch(update_system_agent))
        .route(
            "/api/system-agents/:agent_key/mcp-bindings",
            get(get_agent_mcp_bindings).put(update_agent_mcp_bindings),
        )
        .route(
            "/api/runtime/agent-capabilities",
            get(resolve_agent_capabilities),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let internal_api = Router::new()
        .route(
            "/api/internal/runtime/agent-capabilities/resolve",
            post(resolve_agent_capabilities_internal),
        )
        .route(
            "/api/internal/local-connector/mcps",
            get(list_local_connector_mcps_internal).post(sync_local_connector_mcp_internal),
        )
        .route(
            "/api/internal/local-connector/mcps/:mcp_id",
            patch(update_local_connector_mcp_internal).delete(delete_local_connector_mcp_internal),
        )
        .route(
            "/api/internal/local-connector/mcps/:mcp_id/status",
            axum::routing::put(update_local_connector_mcp_status_internal),
        )
        .route(
            "/api/internal/local-connector/mcps/status/batch",
            axum::routing::put(update_local_connector_mcp_status_batch_internal),
        );

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/auth/login", post(login_handler))
        .merge(internal_api)
        .merge(protected_api)
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
    let user = verify_token_via_user_service(&state.config, token.as_str())
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
        service: "plugin_management_service".to_string(),
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

async fn current_user_handler(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

async fn list_mcps(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<McpRecord>>, ApiError> {
    state
        .store
        .list_mcps(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<McpPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    validate_client_managed_mcp_payload(&payload)?;
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let display_name = payload
        .display_name
        .as_deref()
        .and_then(|value| normalized(Some(value)))
        .unwrap_or_else(|| name.clone());
    let runtime = payload
        .runtime
        .ok_or_else(|| ApiError::bad_request("runtime is required"))?;
    validate_mcp_runtime(&runtime)?;
    validate_mcp_visibility_for_runtime(visibility.as_str(), &runtime)?;
    let now = now_rfc3339();
    let record = McpRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.clone(),
        owner_kind: owner_kind_for(&visibility, &user),
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        display_name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        runtime,
        security: payload.security.unwrap_or_default(),
        metadata: payload.metadata.unwrap_or_default(),
        created_by: user.user_id.clone(),
        updated_by: user.user_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn get_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<McpRecord>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

async fn update_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
    Json(payload): Json<McpPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    let mut record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if record.source_kind == SOURCE_KIND_SYSTEM_SEED {
        validate_system_seed_mcp_update(&payload)?;
        if let Some(enabled) = payload.enabled {
            record.enabled = enabled;
        }
        record.updated_by = user.user_id.clone();
        record.updated_at = now_rfc3339();
        state
            .store
            .replace_mcp(&record)
            .await
            .map_err(ApiError::internal)?;
        return Ok(Json(record));
    }
    validate_client_managed_mcp_payload(&payload)?;
    if let Some(owner_user_id) = payload.owner_user_id.as_deref() {
        record.owner_user_id = requested_owner_user_id(Some(owner_user_id), &user)?;
    }
    if let Some(visibility) = payload.visibility.as_deref() {
        record.visibility = normalize_visibility(Some(visibility), &user)?;
        record.owner_kind = owner_kind_for(record.visibility.as_str(), &user);
    }
    if let Some(source_kind) = payload.source_kind {
        if user.is_super_admin() {
            record.source_kind = source_kind;
        }
    }
    if let Some(name) = payload.name.as_deref() {
        record.name = required_text(Some(name), "name")?;
    }
    if let Some(display_name) = payload.display_name {
        record.display_name =
            normalized(Some(&display_name)).unwrap_or_else(|| record.name.clone());
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(enabled) = payload.enabled {
        record.enabled = enabled;
    }
    if let Some(runtime) = payload.runtime {
        validate_mcp_runtime(&runtime)?;
        record.runtime = runtime;
    }
    if let Some(security) = payload.security {
        record.security = security;
    }
    if let Some(metadata) = payload.metadata {
        record.metadata = metadata;
    }
    validate_mcp_visibility_for_runtime(record.visibility.as_str(), &record.runtime)?;
    record.updated_by = user.user_id.clone();
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn delete_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if record.source_kind == SOURCE_KIND_SYSTEM_SEED {
        record.enabled = false;
        record.updated_at = now_rfc3339();
        record.updated_by = user.user_id;
        state
            .store
            .replace_mcp(&record)
            .await
            .map_err(ApiError::internal)?;
    } else {
        state
            .store
            .delete_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn check_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if matches!(
        record.runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    ) {
        if let Some(check) = state
            .store
            .get_check(RESOURCE_KIND_MCP, record.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            return Ok(Json(check));
        }
    }
    let check = check_record_for_mcp(&record);
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(check))
}

async fn list_skills(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<SkillRecord>>, ApiError> {
    state
        .store
        .list_skills(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<SkillPayload>,
) -> Result<Json<SkillRecord>, ApiError> {
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let display_name = payload
        .display_name
        .as_deref()
        .and_then(|value| normalized(Some(value)))
        .unwrap_or_else(|| name.clone());
    let content = payload
        .content
        .ok_or_else(|| ApiError::bad_request("content is required"))?;
    validate_skill_content(&content)?;
    let now = now_rfc3339();
    let record = SkillRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id,
        owner_kind: owner_kind_for(&visibility, &user),
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        display_name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        content,
        metadata: payload.metadata.unwrap_or_default(),
        created_by: user.user_id.clone(),
        updated_by: user.user_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_skill(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn get_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
) -> Result<Json<SkillRecord>, ApiError> {
    let record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

async fn update_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
    Json(payload): Json<SkillPayload>,
) -> Result<Json<SkillRecord>, ApiError> {
    let mut record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if let Some(owner_user_id) = payload.owner_user_id.as_deref() {
        record.owner_user_id = requested_owner_user_id(Some(owner_user_id), &user)?;
    }
    if let Some(visibility) = payload.visibility.as_deref() {
        record.visibility = normalize_visibility(Some(visibility), &user)?;
        record.owner_kind = owner_kind_for(record.visibility.as_str(), &user);
    }
    if let Some(source_kind) = payload.source_kind {
        if user.is_super_admin() {
            record.source_kind = source_kind;
        }
    }
    if let Some(name) = payload.name.as_deref() {
        record.name = required_text(Some(name), "name")?;
    }
    if let Some(display_name) = payload.display_name {
        record.display_name =
            normalized(Some(&display_name)).unwrap_or_else(|| record.name.clone());
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(enabled) = payload.enabled {
        record.enabled = enabled;
    }
    if let Some(content) = payload.content {
        validate_skill_content(&content)?;
        record.content = content;
    }
    if let Some(metadata) = payload.metadata {
        record.metadata = metadata;
    }
    record.updated_by = user.user_id.clone();
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_skill(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn delete_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    state
        .store
        .delete_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn check_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    let record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    let check = check_record_for_skill(&record);
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(check))
}

async fn list_skill_packages(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<SkillPackageRecord>>, ApiError> {
    state
        .store
        .list_skill_packages(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<SkillPackagePayload>,
) -> Result<Json<SkillPackageRecord>, ApiError> {
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let now = now_rfc3339();
    let record = SkillPackageRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id,
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        repository: payload
            .repository
            .and_then(|value| normalized(Some(&value))),
        branch: payload.branch.and_then(|value| normalized(Some(&value))),
        cache_ref: payload.cache_ref.and_then(|value| normalized(Some(&value))),
        local_connector: payload.local_connector,
        skill_ids: payload.skill_ids.unwrap_or_default(),
        installed: payload.installed.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_skill_package(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn get_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(package_id): Path<String>,
) -> Result<Json<SkillPackageRecord>, ApiError> {
    let record = state
        .store
        .get_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill package not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

async fn update_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(package_id): Path<String>,
    Json(payload): Json<SkillPackagePayload>,
) -> Result<Json<SkillPackageRecord>, ApiError> {
    let mut record = state
        .store
        .get_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill package not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if let Some(owner_user_id) = payload.owner_user_id.as_deref() {
        record.owner_user_id = requested_owner_user_id(Some(owner_user_id), &user)?;
    }
    if let Some(visibility) = payload.visibility.as_deref() {
        record.visibility = normalize_visibility(Some(visibility), &user)?;
    }
    if let Some(source_kind) = payload.source_kind {
        if user.is_super_admin() {
            record.source_kind = source_kind;
        }
    }
    if let Some(name) = payload.name.as_deref() {
        record.name = required_text(Some(name), "name")?;
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(repository) = payload.repository {
        record.repository = normalized(Some(&repository));
    }
    if let Some(branch) = payload.branch {
        record.branch = normalized(Some(&branch));
    }
    if let Some(cache_ref) = payload.cache_ref {
        record.cache_ref = normalized(Some(&cache_ref));
    }
    if payload.local_connector.is_some() {
        record.local_connector = payload.local_connector;
    }
    if let Some(skill_ids) = payload.skill_ids {
        record.skill_ids = skill_ids;
    }
    if let Some(installed) = payload.installed {
        record.installed = installed;
    }
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_skill_package(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn delete_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(package_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let record = state
        .store
        .get_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill package not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    state
        .store
        .delete_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_system_agents(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<SystemAgentRecord>>, ApiError> {
    ensure_super_admin(&user)?;
    state
        .store
        .list_agents()
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_system_agent(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<SystemAgentPayload>,
) -> Result<Json<SystemAgentRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let agent_key = required_text(payload.agent_key.as_deref(), "agent_key")?;
    if state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .is_some()
    {
        return Err(ApiError::conflict("System agent already exists"));
    }
    let display_name = required_text(payload.display_name.as_deref(), "display_name")?;
    let service_name = required_text(payload.service_name.as_deref(), "service_name")?;
    let now = now_rfc3339();
    let record = SystemAgentRecord {
        id: format!("system_agent_{agent_key}"),
        agent_key,
        display_name,
        service_name,
        scope: "system_internal".to_string(),
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        managed_by: payload.managed_by.unwrap_or_else(|| "admin".to_string()),
        include_user_resources: false,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_agent(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn update_system_agent(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
    Json(payload): Json<SystemAgentPayload>,
) -> Result<Json<SystemAgentRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let mut record = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    if let Some(display_name) = payload.display_name {
        record.display_name = required_text(Some(&display_name), "display_name")?;
    }
    if let Some(service_name) = payload.service_name {
        record.service_name = required_text(Some(&service_name), "service_name")?;
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(enabled) = payload.enabled {
        record.enabled = enabled;
    }
    if let Some(managed_by) = payload.managed_by {
        record.managed_by = managed_by;
    }
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_agent(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

async fn get_agent_mcp_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
) -> Result<Json<AgentMcpBindingsResponse>, ApiError> {
    ensure_super_admin(&user)?;
    build_agent_mcp_bindings_response(&state, agent_key.as_str())
        .await
        .map(Json)
}

async fn update_agent_mcp_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
    Json(payload): Json<UpdateAgentMcpBindingsRequest>,
) -> Result<Json<AgentMcpBindingsResponse>, ApiError> {
    ensure_super_admin(&user)?;
    state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;

    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for selection in payload.bindings {
        let mcp_id = required_text(Some(selection.mcp_id.as_str()), "mcp_id")?;
        if !seen.insert(mcp_id.clone()) {
            return Err(ApiError::bad_request("duplicate mcp_id in bindings"));
        }
        validate_mcp_binding_mode(selection.mode.as_str())?;
        let mcp = state
            .store
            .get_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("MCP not found: {mcp_id}")))?;
        if mcp.visibility != VISIBILITY_SYSTEM_PRIVATE {
            return Err(ApiError::bad_request(
                "system agent bindings only accept system-private MCPs",
            ));
        }
        selected.push((mcp_id, selection.mode));
    }

    state
        .store
        .delete_mcp_bindings_for_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?;

    for (index, (mcp_id, mode)) in selected.into_iter().enumerate() {
        let (enabled, required, binding_scope) = mcp_binding_state(mode.as_str())?;
        let now = now_rfc3339();
        let record = AgentBindingRecord {
            id: format!("{agent_key}__mcp__{mcp_id}"),
            agent_key: agent_key.clone(),
            binding_scope: binding_scope.to_string(),
            owner_user_id: None,
            resource_kind: RESOURCE_KIND_MCP.to_string(),
            resource_id: mcp_id,
            enabled,
            required,
            priority: 100 + index as i64,
            conditions: BindingConditions::default(),
            created_by: user.user_id.clone(),
            updated_by: user.user_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        state
            .store
            .replace_binding(&record)
            .await
            .map_err(ApiError::internal)?;
    }

    build_agent_mcp_bindings_response(&state, agent_key.as_str())
        .await
        .map(Json)
}

async fn build_agent_mcp_bindings_response(
    state: &AppState,
    agent_key: &str,
) -> Result<AgentMcpBindingsResponse, ApiError> {
    let agent = state
        .store
        .get_agent(agent_key)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    let mcps = state
        .store
        .list_system_mcps()
        .await
        .map_err(ApiError::internal)?;
    let bindings = state
        .store
        .list_bindings(agent_key, &ListBindingsQuery::default())
        .await
        .map_err(ApiError::internal)?;
    let mut modes = HashMap::new();
    for binding in bindings
        .into_iter()
        .filter(|binding| binding.enabled && binding.resource_kind == RESOURCE_KIND_MCP)
    {
        let mode = if binding.required {
            MCP_BINDING_MODE_REQUIRED
        } else {
            MCP_BINDING_MODE_OPTIONAL
        };
        modes
            .entry(binding.resource_id)
            .and_modify(|current: &mut &str| {
                if mode == MCP_BINDING_MODE_REQUIRED {
                    *current = mode;
                }
            })
            .or_insert(mode);
    }
    let items = mcps
        .into_iter()
        .map(|mcp| AgentMcpBindingView {
            mode: modes
                .get(mcp.id.as_str())
                .copied()
                .unwrap_or(MCP_BINDING_MODE_DISABLED)
                .to_string(),
            mcp,
        })
        .collect();
    Ok(AgentMcpBindingsResponse { agent, items })
}

async fn resolve_agent_capabilities(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<RuntimeCapabilitiesQuery>,
) -> Result<Json<RuntimeCapabilitiesResponse>, ApiError> {
    let requested_owner = query
        .owner_user_id
        .as_deref()
        .and_then(|value| normalized(Some(value)));
    if !user.is_super_admin()
        && requested_owner
            .as_deref()
            .is_some_and(|owner| owner != user.effective_owner_user_id())
    {
        return Err(ApiError::forbidden(
            "ordinary users cannot resolve capabilities for another owner",
        ));
    }
    let owner_user_id = if user.is_super_admin() {
        requested_owner.unwrap_or_else(|| user.effective_owner_user_id().to_string())
    } else {
        user.effective_owner_user_id().to_string()
    };
    resolve_agent_capabilities_for_owner(
        &state,
        query.agent_key,
        owner_user_id,
        query.include_unavailable.unwrap_or(true),
    )
    .await
    .map(Json)
}

async fn resolve_agent_capabilities_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RuntimeCapabilitiesRequest>,
) -> Result<Json<RuntimeCapabilitiesResponse>, ApiError> {
    require_internal_api_secret(&state, &headers)?;
    let owner_user_id = normalized(Some(input.owner_user_id.as_str()))
        .ok_or_else(|| ApiError::bad_request("owner_user_id is required"))?;
    let caller_service = require_internal_caller_service(&headers)?;
    tracing::debug!(
        caller_service,
        agent_key = input.agent_key,
        "resolving agent capabilities through internal API"
    );
    resolve_agent_capabilities_for_owner(
        &state,
        input.agent_key,
        owner_user_id,
        input.include_unavailable,
    )
    .await
    .map(Json)
}

async fn list_local_connector_mcps_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LocalConnectorMcpInternalQuery>,
) -> Result<Json<ListResponse<McpRecord>>, ApiError> {
    require_local_connector_internal_request(&state, &headers)?;
    let owner_user_id = required_text(query.owner_user_id.as_deref(), "owner_user_id")?;
    let device_id = required_text(query.device_id.as_deref(), "device_id")?;
    let items = state
        .store
        .list_local_connector_mcps(owner_user_id.as_str(), device_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

async fn sync_local_connector_mcp_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LocalConnectorMcpSyncPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers)?;
    let owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    let device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    let manifest_id = required_text(Some(payload.manifest_id.as_str()), "manifest_id")?;
    let existing = state
        .store
        .find_local_connector_mcp(
            owner_user_id.as_str(),
            device_id.as_str(),
            manifest_id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    sync_local_connector_mcp_record(&state, existing, payload)
        .await
        .map(Json)
}

async fn update_local_connector_mcp_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(mcp_id): Path<String>,
    Json(payload): Json<LocalConnectorMcpSyncPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers)?;
    let record = load_local_connector_mcp_for_sync(&state, mcp_id.as_str(), &payload).await?;
    sync_local_connector_mcp_record(&state, Some(record), payload)
        .await
        .map(Json)
}

async fn delete_local_connector_mcp_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(mcp_id): Path<String>,
    Query(query): Query<LocalConnectorMcpInternalQuery>,
) -> Result<StatusCode, ApiError> {
    require_local_connector_internal_request(&state, &headers)?;
    let owner_user_id = required_text(query.owner_user_id.as_deref(), "owner_user_id")?;
    let device_id = required_text(query.device_id.as_deref(), "device_id")?;
    let manifest_id = required_text(query.manifest_id.as_deref(), "manifest_id")?;
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_local_connector_record_scope(
        &record,
        owner_user_id.as_str(),
        device_id.as_str(),
        manifest_id.as_str(),
    )?;
    state
        .store
        .delete_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_local_connector_mcp_status_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(mcp_id): Path<String>,
    Json(payload): Json<LocalConnectorMcpStatusPayload>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers)?;
    update_local_connector_mcp_status_record(&state, mcp_id.as_str(), payload)
        .await
        .map(Json)
}

async fn update_local_connector_mcp_status_batch_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LocalConnectorMcpStatusBatchPayload>,
) -> Result<Json<Vec<ResourceCheckRecord>>, ApiError> {
    require_local_connector_internal_request(&state, &headers)?;
    if payload.items.len() > 200 {
        return Err(ApiError::bad_request(
            "local connector MCP status batch exceeds 200 items",
        ));
    }
    let mut checks = Vec::with_capacity(payload.items.len());
    for item in payload.items {
        checks.push(
            update_local_connector_mcp_status_record(
                &state,
                item.mcp_id.as_str(),
                LocalConnectorMcpStatusPayload {
                    owner_user_id: item.owner_user_id,
                    device_id: item.device_id,
                    workspace_id: item.workspace_id,
                    manifest_id: item.manifest_id,
                    status: item.status,
                    last_error: item.last_error,
                    tool_snapshot: item.tool_snapshot,
                    manifest_hash: item.manifest_hash,
                },
            )
            .await?,
        );
    }
    Ok(Json(checks))
}

async fn sync_local_connector_mcp_record(
    state: &AppState,
    existing: Option<McpRecord>,
    payload: LocalConnectorMcpSyncPayload,
) -> Result<McpRecord, ApiError> {
    validate_local_connector_sync_payload(&payload)?;
    let owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    let device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    let manifest_id = required_text(Some(payload.manifest_id.as_str()), "manifest_id")?;
    let internal_name = required_text(Some(payload.internal_name.as_str()), "internal_name")?;
    let display_name = required_text(Some(payload.display_name.as_str()), "display_name")?;
    validate_internal_mcp_name(internal_name.as_str())?;
    let now = now_rfc3339();
    let mut metadata = existing
        .as_ref()
        .map(|record| record.metadata.clone())
        .unwrap_or_default();
    metadata.category = Some("user_local_mcp".to_string());
    metadata
        .extra
        .insert("managed_by".to_string(), json!("local_connector_client"));
    let runtime = McpRuntime {
        kind: payload.runtime_kind.clone(),
        server_name: Some(internal_name.clone()),
        local_connector: Some(LocalConnectorRef {
            device_id: Some(device_id.clone()),
            workspace_id: None,
            manifest_id: Some(manifest_id.clone()),
            relative_path: None,
            requires_online: true,
        }),
        ..McpRuntime::default()
    };
    validate_mcp_runtime(&runtime)?;
    let record = McpRecord {
        id: existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_user_id: owner_user_id.clone(),
        owner_kind: OWNER_KIND_USER.to_string(),
        visibility: VISIBILITY_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED.to_string(),
        name: existing
            .as_ref()
            .map(|record| record.name.clone())
            .unwrap_or_else(|| internal_name.clone()),
        display_name,
        description: payload
            .description
            .as_deref()
            .and_then(|value| normalized(Some(value))),
        enabled: payload.enabled,
        runtime,
        security: existing
            .as_ref()
            .map(|record| record.security.clone())
            .unwrap_or_default(),
        metadata,
        created_by: existing
            .as_ref()
            .map(|record| record.created_by.clone())
            .unwrap_or_else(|| "local-connector-service".to_string()),
        updated_by: "local-connector-service".to_string(),
        created_at: existing
            .as_ref()
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    if let Some(existing) = existing.as_ref() {
        ensure_local_connector_record_scope(
            existing,
            owner_user_id.as_str(),
            device_id.as_str(),
            manifest_id.as_str(),
        )?;
    }
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    reconcile_local_connector_check_after_sync(state, &record, payload.manifest_hash.as_deref())
        .await?;
    Ok(record)
}

async fn load_local_connector_mcp_for_sync(
    state: &AppState,
    mcp_id: &str,
    payload: &LocalConnectorMcpSyncPayload,
) -> Result<McpRecord, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_local_connector_record_scope(
        &record,
        payload.owner_user_id.as_str(),
        payload.device_id.as_str(),
        payload.manifest_id.as_str(),
    )?;
    Ok(record)
}

fn validate_local_connector_sync_payload(
    payload: &LocalConnectorMcpSyncPayload,
) -> Result<(), ApiError> {
    for (value, field) in [
        (payload.owner_user_id.as_str(), "owner_user_id"),
        (payload.device_id.as_str(), "device_id"),
        (payload.manifest_id.as_str(), "manifest_id"),
        (payload.internal_name.as_str(), "internal_name"),
        (payload.display_name.as_str(), "display_name"),
    ] {
        required_text(Some(value), field)?;
    }
    if !matches!(
        payload.runtime_kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
    ) {
        return Err(ApiError::bad_request(
            "local connector user MCP runtime must be local_connector_stdio or local_connector_http",
        ));
    }
    Ok(())
}

fn validate_internal_mcp_name(value: &str) -> Result<(), ApiError> {
    if value.len() > 96
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(ApiError::bad_request(
            "internal_name must contain only ASCII letters, numbers, underscore, or hyphen",
        ));
    }
    Ok(())
}

fn ensure_local_connector_record_scope(
    record: &McpRecord,
    owner_user_id: &str,
    device_id: &str,
    manifest_id: &str,
) -> Result<(), ApiError> {
    let local = record.runtime.local_connector.as_ref();
    let matches_scope = record.owner_user_id == owner_user_id
        && record.visibility == VISIBILITY_PRIVATE
        && record.source_kind == SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED
        && matches!(
            record.runtime.kind.as_str(),
            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
        )
        && local.and_then(|value| value.device_id.as_deref()) == Some(device_id)
        && local.and_then(|value| value.manifest_id.as_deref()) == Some(manifest_id);
    if matches_scope {
        Ok(())
    } else {
        Err(ApiError::not_found("MCP not found"))
    }
}

async fn reconcile_local_connector_check_after_sync(
    state: &AppState,
    record: &McpRecord,
    manifest_hash: Option<&str>,
) -> Result<(), ApiError> {
    let current = state
        .store
        .get_check(RESOURCE_KIND_MCP, record.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let normalized_hash = normalized(manifest_hash);
    let preserve_available = record.enabled
        && normalized_hash.is_some()
        && current.as_ref().is_some_and(|check| {
            check.status == "available" && check.manifest_hash == normalized_hash
        });
    if preserve_available {
        return Ok(());
    }
    let check = ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status: if record.enabled {
            "unknown".to_string()
        } else {
            "unavailable".to_string()
        },
        last_checked_at: now_rfc3339(),
        last_error: Some(if record.enabled {
            "Local Connector MCP is waiting for a successful local check".to_string()
        } else {
            "resource is disabled".to_string()
        }),
        tool_snapshot: Vec::new(),
        manifest_hash: normalized_hash,
    };
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)
}

async fn update_local_connector_mcp_status_record(
    state: &AppState,
    mcp_id: &str,
    payload: LocalConnectorMcpStatusPayload,
) -> Result<ResourceCheckRecord, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_local_connector_record_scope(
        &record,
        payload.owner_user_id.as_str(),
        payload.device_id.as_str(),
        payload.manifest_id.as_str(),
    )?;
    let status = normalize_local_connector_status(payload.status.as_str())?;
    let manifest_hash = normalized(payload.manifest_hash.as_deref());
    if status == "available" {
        if !record.enabled {
            return Err(ApiError::bad_request(
                "disabled Local Connector MCP cannot be marked available",
            ));
        }
        if manifest_hash.is_none() {
            return Err(ApiError::bad_request(
                "available Local Connector MCP requires manifest_hash",
            ));
        }
        if payload.tool_snapshot.is_empty() {
            return Err(ApiError::bad_request(
                "available Local Connector MCP requires a non-empty tool snapshot",
            ));
        }
    }
    let current = state
        .store
        .get_check(RESOURCE_KIND_MCP, record.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    ensure_local_connector_manifest_hash_matches(current.as_ref(), manifest_hash.as_deref())?;
    let tool_snapshot = sanitize_tool_snapshot(
        payload.tool_snapshot,
        state.config.local_connector_max_tool_snapshot_bytes,
    )?;
    let check = ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status: if record.enabled {
            status.to_string()
        } else {
            "unavailable".to_string()
        },
        last_checked_at: now_rfc3339(),
        last_error: normalized(payload.last_error.as_deref())
            .map(|value| truncate_text(value.as_str(), 1000)),
        tool_snapshot,
        manifest_hash,
    };
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)?;
    Ok(check)
}

fn ensure_local_connector_manifest_hash_matches(
    current: Option<&ResourceCheckRecord>,
    manifest_hash: Option<&str>,
) -> Result<(), ApiError> {
    if current
        .and_then(|check| check.manifest_hash.as_deref())
        .is_some()
        && manifest_hash.is_some()
        && current.and_then(|check| check.manifest_hash.as_deref()) != manifest_hash
    {
        return Err(ApiError::conflict(
            "Local Connector MCP manifest hash does not match the synced descriptor",
        ));
    }
    Ok(())
}

fn normalize_local_connector_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "available" => Ok("available"),
        "unavailable" => Ok("unavailable"),
        "offline" => Ok("offline"),
        "invalid" => Ok("invalid"),
        "unknown" => Ok("unknown"),
        _ => Err(ApiError::bad_request(
            "status must be available, unavailable, offline, invalid, or unknown",
        )),
    }
}

fn sanitize_tool_snapshot(
    mut tools: Vec<serde_json::Value>,
    max_bytes: usize,
) -> Result<Vec<serde_json::Value>, ApiError> {
    if tools.len() > 200 {
        tools.truncate(200);
    }
    let encoded = serde_json::to_vec(&tools)
        .map_err(|err| ApiError::bad_request(format!("invalid tool snapshot: {err}")))?;
    if encoded.len() > max_bytes {
        return Err(ApiError::bad_request(format!(
            "tool snapshot exceeds {max_bytes} bytes"
        )));
    }
    Ok(tools)
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn require_local_connector_internal_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    require_internal_api_secret(state, headers)?;
    let caller = require_internal_caller_service(headers)?;
    if caller != "local-connector-service" {
        return Err(ApiError::forbidden(
            "local connector MCP sync requires local-connector-service caller",
        ));
    }
    Ok(())
}

fn require_internal_caller_service(headers: &HeaderMap) -> Result<&str, ApiError> {
    let caller_service = headers
        .get("x-plugin-management-caller-service")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("caller service is required"))?;
    if !ALLOWED_INTERNAL_CALLER_SERVICES.contains(&caller_service) {
        return Err(ApiError::forbidden("caller service is not allowed"));
    }
    Ok(caller_service)
}

fn require_internal_api_secret(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = state
        .config
        .internal_api_secret
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized("plugin management internal API is disabled"))?;
    let actual = headers
        .get("x-plugin-management-internal-secret")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing plugin management internal API secret"))?;
    if !constant_time_eq(expected.as_bytes(), actual.as_bytes()) {
        return Err(ApiError::unauthorized(
            "invalid plugin management internal API secret",
        ));
    }
    Ok(())
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    for (left, right) in expected.iter().zip(actual.iter()) {
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

async fn resolve_agent_capabilities_for_owner(
    state: &AppState,
    agent_key: String,
    owner_user_id: String,
    include_unavailable: bool,
) -> Result<RuntimeCapabilitiesResponse, ApiError> {
    let agent = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    if !agent.enabled {
        return Err(ApiError::bad_request("System agent is disabled"));
    }
    let bindings = state
        .store
        .list_bindings_for_runtime(agent_key.as_str(), owner_user_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let mut mcps = Vec::new();
    let mut skills = Vec::new();
    let mut local_connector_requirements = Vec::new();

    for binding in bindings {
        match binding.resource_kind.as_str() {
            RESOURCE_KIND_MCP => {
                let Some(resource) = state
                    .store
                    .get_mcp(binding.resource_id.as_str())
                    .await
                    .map_err(ApiError::internal)?
                else {
                    continue;
                };
                if !resource_visible_in_runtime(
                    &resource.owner_user_id,
                    &resource.visibility,
                    owner_user_id.as_str(),
                    &binding,
                ) {
                    continue;
                }
                let (available, status, reason) = availability_for_mcp(&state, &resource).await?;
                collect_local_connector_requirement_for_mcp(
                    &mut local_connector_requirements,
                    &resource,
                    &binding,
                    available,
                    reason.clone(),
                );
                if available || include_unavailable {
                    mcps.push(ResolvedMcp {
                        resource,
                        binding,
                        available,
                        status,
                        reason,
                    });
                }
            }
            RESOURCE_KIND_SKILL => {
                let Some(resource) = state
                    .store
                    .get_skill(binding.resource_id.as_str())
                    .await
                    .map_err(ApiError::internal)?
                else {
                    continue;
                };
                if !resource_visible_in_runtime(
                    &resource.owner_user_id,
                    &resource.visibility,
                    owner_user_id.as_str(),
                    &binding,
                ) {
                    continue;
                }
                let (available, status, reason) = availability_for_skill(&state, &resource).await?;
                collect_local_connector_requirement_for_skill(
                    &mut local_connector_requirements,
                    &resource,
                    &binding,
                    available,
                    reason.clone(),
                );
                if available || include_unavailable {
                    skills.push(ResolvedSkill {
                        resource,
                        binding,
                        available,
                        status,
                        reason,
                    });
                }
            }
            RESOURCE_KIND_SKILL_PACKAGE => {
                let Some(package) = state
                    .store
                    .get_skill_package(binding.resource_id.as_str())
                    .await
                    .map_err(ApiError::internal)?
                else {
                    continue;
                };
                if !package.installed
                    || !resource_visible_in_runtime(
                        &package.owner_user_id,
                        &package.visibility,
                        owner_user_id.as_str(),
                        &binding,
                    )
                {
                    continue;
                }
                for skill_id in &package.skill_ids {
                    let Some(resource) = state
                        .store
                        .get_skill(skill_id.as_str())
                        .await
                        .map_err(ApiError::internal)?
                    else {
                        continue;
                    };
                    if !resource_visible_in_runtime(
                        &resource.owner_user_id,
                        &resource.visibility,
                        owner_user_id.as_str(),
                        &binding,
                    ) {
                        continue;
                    }
                    let (available, status, reason) =
                        availability_for_skill(&state, &resource).await?;
                    collect_local_connector_requirement_for_skill(
                        &mut local_connector_requirements,
                        &resource,
                        &binding,
                        available,
                        reason.clone(),
                    );
                    if available || include_unavailable {
                        skills.push(ResolvedSkill {
                            resource,
                            binding: binding.clone(),
                            available,
                            status,
                            reason,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if agent.include_user_resources {
        let mut resolved_mcp_ids = mcps
            .iter()
            .map(|item| item.resource.id.clone())
            .collect::<HashSet<_>>();
        for resource in state
            .store
            .list_enabled_user_mcps(owner_user_id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if !resolved_mcp_ids.insert(resource.id.clone()) {
                continue;
            }
            let binding = automatic_user_binding(
                agent_key.as_str(),
                owner_user_id.as_str(),
                RESOURCE_KIND_MCP,
                resource.id.as_str(),
            );
            let (available, status, reason) = availability_for_mcp(&state, &resource).await?;
            collect_local_connector_requirement_for_mcp(
                &mut local_connector_requirements,
                &resource,
                &binding,
                available,
                reason.clone(),
            );
            if available || include_unavailable {
                mcps.push(ResolvedMcp {
                    resource,
                    binding,
                    available,
                    status,
                    reason,
                });
            }
        }

        let mut resolved_skill_ids = skills
            .iter()
            .map(|item| item.resource.id.clone())
            .collect::<HashSet<_>>();
        for resource in state
            .store
            .list_enabled_user_skills(owner_user_id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if !resolved_skill_ids.insert(resource.id.clone()) {
                continue;
            }
            let binding = automatic_user_binding(
                agent_key.as_str(),
                owner_user_id.as_str(),
                RESOURCE_KIND_SKILL,
                resource.id.as_str(),
            );
            let (available, status, reason) = availability_for_skill(&state, &resource).await?;
            collect_local_connector_requirement_for_skill(
                &mut local_connector_requirements,
                &resource,
                &binding,
                available,
                reason.clone(),
            );
            if available || include_unavailable {
                skills.push(ResolvedSkill {
                    resource,
                    binding,
                    available,
                    status,
                    reason,
                });
            }
        }
    }

    let generated_at = now_rfc3339();
    let policy_revision = capability_policy_revision(&agent, &mcps, &skills);
    Ok(RuntimeCapabilitiesResponse {
        agent_key,
        owner_user_id,
        policy_revision,
        generated_at,
        agent_enabled: agent.enabled,
        mcps,
        skills,
        local_connector_requirements,
    })
}

fn capability_policy_revision(
    agent: &SystemAgentRecord,
    mcps: &[ResolvedMcp],
    skills: &[ResolvedSkill],
) -> String {
    let mut revision_parts = vec![format!(
        "agent:{}:{}:{}",
        agent.agent_key, agent.enabled, agent.updated_at
    )];
    revision_parts.extend(mcps.iter().map(|item| {
        format!(
            "mcp:{}:{}:{}:{}:{}:{}",
            item.resource.id,
            item.resource.enabled,
            item.resource.updated_at,
            item.binding.required,
            item.binding.enabled,
            item.binding.updated_at
        )
    }));
    revision_parts.extend(skills.iter().map(|item| {
        format!(
            "skill:{}:{}:{}:{}:{}:{}",
            item.resource.id,
            item.resource.enabled,
            item.resource.updated_at,
            item.binding.required,
            item.binding.enabled,
            item.binding.updated_at
        )
    }));
    revision_parts.sort();
    let mut hasher = DefaultHasher::new();
    revision_parts.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn automatic_user_binding(
    agent_key: &str,
    owner_user_id: &str,
    resource_kind: &str,
    resource_id: &str,
) -> AgentBindingRecord {
    let now = now_rfc3339();
    AgentBindingRecord {
        id: format!("{agent_key}__automatic_user__{resource_kind}__{resource_id}"),
        agent_key: agent_key.to_string(),
        binding_scope: BINDING_SCOPE_USER_OVERRIDE.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        resource_kind: resource_kind.to_string(),
        resource_id: resource_id.to_string(),
        enabled: true,
        required: false,
        priority: 1_000,
        conditions: BindingConditions::default(),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: now.clone(),
        updated_at: now,
    }
}

fn required_text(value: Option<&str>, field: &str) -> Result<String, ApiError> {
    normalized(value).ok_or_else(|| ApiError::bad_request(format!("{field} is required")))
}

fn normalize_visibility(value: Option<&str>, user: &CurrentUser) -> Result<String, ApiError> {
    let visibility = normalized(value).unwrap_or_else(|| VISIBILITY_PRIVATE.to_string());
    match visibility.as_str() {
        VISIBILITY_PRIVATE => Ok(visibility),
        VISIBILITY_PUBLIC | VISIBILITY_SYSTEM_PRIVATE if user.is_super_admin() => Ok(visibility),
        VISIBILITY_PUBLIC | VISIBILITY_SYSTEM_PRIVATE => Err(ApiError::forbidden(
            "only super_admin can create public or system-private resources",
        )),
        _ => Err(ApiError::bad_request(
            "visibility must be private, public, or system_private",
        )),
    }
}

fn requested_owner_user_id(value: Option<&str>, user: &CurrentUser) -> Result<String, ApiError> {
    let requested = normalized(value).unwrap_or_else(|| user.effective_owner_user_id().to_string());
    if user.is_super_admin() || requested == user.effective_owner_user_id() {
        Ok(requested)
    } else {
        Err(ApiError::forbidden(
            "cannot write resources for another user",
        ))
    }
}

fn owner_kind_for(visibility: &str, user: &CurrentUser) -> String {
    if visibility == VISIBILITY_SYSTEM_PRIVATE {
        OWNER_KIND_SYSTEM.to_string()
    } else if user.is_super_admin() {
        OWNER_KIND_ADMIN.to_string()
    } else {
        OWNER_KIND_USER.to_string()
    }
}

fn default_source_kind(value: Option<String>, user: &CurrentUser) -> String {
    if user.is_super_admin() {
        value.unwrap_or_else(|| SOURCE_KIND_ADMIN_CREATED.to_string())
    } else {
        SOURCE_KIND_USER_CREATED.to_string()
    }
}

fn ensure_super_admin(user: &CurrentUser) -> Result<(), ApiError> {
    if user.is_super_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("super_admin permission required"))
    }
}

fn ensure_can_read_resource(
    user: &CurrentUser,
    owner_user_id: &str,
    visibility: &str,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || visibility == VISIBILITY_PUBLIC
        || (visibility == VISIBILITY_PRIVATE && owner_user_id == user.effective_owner_user_id())
    {
        Ok(())
    } else {
        Err(ApiError::not_found("resource not found"))
    }
}

fn ensure_can_update_resource(
    user: &CurrentUser,
    owner_user_id: &str,
    visibility: &str,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || (visibility == VISIBILITY_PRIVATE && owner_user_id == user.effective_owner_user_id())
    {
        Ok(())
    } else {
        Err(ApiError::forbidden("resource is not writable"))
    }
}

fn validate_client_managed_mcp_payload(payload: &McpPayload) -> Result<(), ApiError> {
    if matches!(
        normalized(payload.source_kind.as_deref()).as_deref(),
        Some(SOURCE_KIND_SYSTEM_SEED)
    ) {
        return Err(ApiError::bad_request(
            "system seed MCPs are managed by the service",
        ));
    }
    if matches!(
        payload
            .runtime
            .as_ref()
            .map(|runtime| runtime.kind.as_str()),
        Some(RUNTIME_KIND_BUILTIN | RUNTIME_KIND_SYSTEM_ROUTED)
    ) {
        return Err(ApiError::bad_request(
            "builtin and system-routed MCPs are managed by the service",
        ));
    }
    Ok(())
}

fn validate_system_seed_mcp_update(payload: &McpPayload) -> Result<(), ApiError> {
    let modifies_managed_fields = payload.owner_user_id.is_some()
        || payload.visibility.is_some()
        || payload.source_kind.is_some()
        || payload.name.is_some()
        || payload.display_name.is_some()
        || payload.description.is_some()
        || payload.runtime.is_some()
        || payload.security.is_some()
        || payload.metadata.is_some();
    if modifies_managed_fields {
        Err(ApiError::bad_request(
            "system seed MCPs only allow updating enabled",
        ))
    } else {
        Ok(())
    }
}

fn validate_mcp_runtime(runtime: &McpRuntime) -> Result<(), ApiError> {
    match runtime.kind.as_str() {
        RUNTIME_KIND_BUILTIN => {
            if runtime
                .builtin_kind
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request("builtin MCP requires builtin_kind"));
            }
        }
        RUNTIME_KIND_SYSTEM_ROUTED => {
            if runtime
                .server_name
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request(
                    "system-routed MCP requires server_name",
                ));
            }
        }
        RUNTIME_KIND_HTTP => {
            if runtime
                .url
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request("HTTP MCP requires url"));
            }
        }
        RUNTIME_KIND_STDIO_CLOUD => {
            if runtime
                .command
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request("stdio MCP requires command"));
            }
        }
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
        | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
        | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY => validate_local_connector_ref(runtime)?,
        _ => {
            return Err(ApiError::bad_request(
                "runtime.kind must be builtin, system_routed, http, stdio_cloud, local_connector_stdio, local_connector_http, or local_connector_builtin_proxy",
            ));
        }
    }
    Ok(())
}

fn validate_local_connector_ref(runtime: &McpRuntime) -> Result<(), ApiError> {
    let local = runtime
        .local_connector
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("local connector runtime requires local_connector"))?;
    for (value, field) in [
        (local.device_id.as_deref(), "device_id"),
        (local.manifest_id.as_deref(), "manifest_id"),
    ] {
        if value.and_then(|value| normalized(Some(value))).is_none() {
            return Err(ApiError::bad_request(format!(
                "local connector runtime requires {field}"
            )));
        }
    }
    if runtime.kind == RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
        && local
            .workspace_id
            .as_deref()
            .and_then(|value| normalized(Some(value)))
            .is_none()
    {
        return Err(ApiError::bad_request(
            "local connector builtin proxy requires workspace_id",
        ));
    }
    if !local.requires_online {
        return Err(ApiError::bad_request(
            "local connector runtime requires requires_online=true",
        ));
    }
    if runtime.command.is_some()
        || !runtime.args.is_empty()
        || !runtime.env.is_empty()
        || runtime.cwd.is_some()
        || runtime.url.is_some()
        || !runtime.headers.is_empty()
    {
        return Err(ApiError::bad_request(
            "local connector runtime secrets and execution config must remain on the client",
        ));
    }
    Ok(())
}

fn validate_mcp_visibility_for_runtime(
    visibility: &str,
    runtime: &McpRuntime,
) -> Result<(), ApiError> {
    if matches!(
        runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    ) && visibility != VISIBILITY_PRIVATE
    {
        return Err(ApiError::bad_request(
            "local connector MCPs must use private visibility",
        ));
    }
    Ok(())
}

fn validate_skill_content(content: &SkillContent) -> Result<(), ApiError> {
    match content.kind.as_str() {
        "inline_content" => {
            if content
                .inline
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request(
                    "inline skill requires inline content",
                ));
            }
        }
        "cloud_package" | "git_package" => {}
        "local_connector_file" | "local_connector_package" => {
            if content.local_connector.is_none() {
                return Err(ApiError::bad_request(
                    "local connector skill requires local_connector",
                ));
            }
        }
        _ => {
            return Err(ApiError::bad_request(
                "content.kind must be inline_content, cloud_package, git_package, local_connector_file, or local_connector_package",
            ));
        }
    }
    Ok(())
}

fn validate_mcp_binding_mode(value: &str) -> Result<(), ApiError> {
    match value {
        MCP_BINDING_MODE_DISABLED | MCP_BINDING_MODE_OPTIONAL | MCP_BINDING_MODE_REQUIRED => Ok(()),
        _ => Err(ApiError::bad_request(
            "binding mode must be disabled, optional, or required",
        )),
    }
}

fn mcp_binding_state(value: &str) -> Result<(bool, bool, &'static str), ApiError> {
    validate_mcp_binding_mode(value)?;
    Ok(match value {
        MCP_BINDING_MODE_DISABLED => (false, false, BINDING_SCOPE_GLOBAL_DEFAULT),
        MCP_BINDING_MODE_OPTIONAL => (true, false, BINDING_SCOPE_GLOBAL_DEFAULT),
        MCP_BINDING_MODE_REQUIRED => (true, true, BINDING_SCOPE_SYSTEM_REQUIRED),
        _ => unreachable!("validated MCP binding mode"),
    })
}

fn check_record_for_mcp(record: &McpRecord) -> ResourceCheckRecord {
    let (status, error) = if record.enabled {
        match record.runtime.kind.as_str() {
            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY => (
                "unknown".to_string(),
                Some(
                    "Local Connector runtime check is not wired in this service phase".to_string(),
                ),
            ),
            _ => ("available".to_string(), None),
        }
    } else {
        (
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        )
    };
    ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status,
        last_checked_at: now_rfc3339(),
        last_error: error,
        tool_snapshot: Vec::new(),
        manifest_hash: None,
    }
}

fn check_record_for_skill(record: &SkillRecord) -> ResourceCheckRecord {
    let is_local = matches!(
        record.content.kind.as_str(),
        "local_connector_file" | "local_connector_package"
    );
    let (status, error) = if !record.enabled {
        (
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        )
    } else if is_local {
        (
            "unknown".to_string(),
            Some("Local Connector skill check is not wired in this service phase".to_string()),
        )
    } else {
        ("available".to_string(), None)
    };
    ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_SKILL, record.id),
        resource_kind: RESOURCE_KIND_SKILL.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status,
        last_checked_at: now_rfc3339(),
        last_error: error,
        tool_snapshot: Vec::new(),
        manifest_hash: None,
    }
}

fn resource_visible_in_runtime(
    owner_user_id: &str,
    visibility: &str,
    runtime_owner_user_id: &str,
    binding: &AgentBindingRecord,
) -> bool {
    visibility == VISIBILITY_PUBLIC
        || owner_user_id == runtime_owner_user_id
        || (visibility == VISIBILITY_SYSTEM_PRIVATE
            && matches!(
                binding.binding_scope.as_str(),
                BINDING_SCOPE_SYSTEM_REQUIRED | BINDING_SCOPE_GLOBAL_DEFAULT
            ))
}

async fn availability_for_mcp(
    state: &AppState,
    record: &McpRecord,
) -> Result<(bool, String, Option<String>), ApiError> {
    if !record.enabled {
        return Ok((
            false,
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        ));
    }
    let local = matches!(
        record.runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    );
    if local {
        let check = state
            .store
            .get_check(RESOURCE_KIND_MCP, record.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        return Ok(match check {
            Some(check)
                if check.status == "available"
                    && check.manifest_hash.is_some()
                    && !check.tool_snapshot.is_empty()
                    && local_connector_check_is_fresh(
                        check.last_checked_at.as_str(),
                        state.config.local_connector_check_ttl,
                    ) =>
            {
                (true, check.status, check.last_error)
            }
            Some(check) if check.status == "available" => (
                false,
                "offline".to_string(),
                Some("Local Connector availability check is stale or incomplete".to_string()),
            ),
            Some(check) => (false, check.status, check.last_error),
            None => (
                false,
                "unknown".to_string(),
                Some("Local Connector status has not been checked".to_string()),
            ),
        });
    }
    Ok((true, "available".to_string(), None))
}

fn local_connector_check_is_fresh(last_checked_at: &str, ttl: std::time::Duration) -> bool {
    let Ok(last_checked_at) = chrono::DateTime::parse_from_rfc3339(last_checked_at) else {
        return false;
    };
    let age = chrono::Utc::now().signed_duration_since(last_checked_at.with_timezone(&chrono::Utc));
    age.num_milliseconds() >= 0
        && u128::try_from(age.num_milliseconds())
            .ok()
            .is_some_and(|age_ms| age_ms <= ttl.as_millis())
}

async fn availability_for_skill(
    state: &AppState,
    record: &SkillRecord,
) -> Result<(bool, String, Option<String>), ApiError> {
    if !record.enabled {
        return Ok((
            false,
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        ));
    }
    let local = matches!(
        record.content.kind.as_str(),
        "local_connector_file" | "local_connector_package"
    );
    if local {
        let check = state
            .store
            .get_check(RESOURCE_KIND_SKILL, record.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        return Ok(match check {
            Some(check) if check.status == "available" => (true, check.status, check.last_error),
            Some(check) => (false, check.status, check.last_error),
            None => (
                false,
                "unknown".to_string(),
                Some("Local Connector status has not been checked".to_string()),
            ),
        });
    }
    Ok((true, "available".to_string(), None))
}

fn collect_local_connector_requirement_for_mcp(
    out: &mut Vec<LocalConnectorRequirement>,
    resource: &McpRecord,
    binding: &AgentBindingRecord,
    available: bool,
    reason: Option<String>,
) {
    let Some(local) = resource.runtime.local_connector.as_ref() else {
        return;
    };
    out.push(LocalConnectorRequirement {
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: resource.id.clone(),
        device_id: local.device_id.clone(),
        workspace_id: local.workspace_id.clone(),
        required: binding.required,
        available,
        reason,
    });
}

fn collect_local_connector_requirement_for_skill(
    out: &mut Vec<LocalConnectorRequirement>,
    resource: &SkillRecord,
    binding: &AgentBindingRecord,
    available: bool,
    reason: Option<String>,
) {
    let Some(local) = resource.content.local_connector.as_ref() else {
        return;
    };
    out.push(LocalConnectorRequirement {
        resource_kind: RESOURCE_KIND_SKILL.to_string(),
        resource_id: resource.id.clone(),
        device_id: local.device_id.clone(),
        workspace_id: local.workspace_id.clone(),
        required: binding.required,
        available,
        reason,
    });
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;
    use mongodb::Client;

    use super::*;
    use crate::config::AppConfig;
    use crate::store::AppStore;

    fn user(role: &str) -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            user_id: "user-1".to_string(),
            username: "user".to_string(),
            display_name: "User".to_string(),
            role: role.to_string(),
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
        }
    }

    fn binding(scope: &str) -> AgentBindingRecord {
        AgentBindingRecord {
            id: "binding-1".to_string(),
            agent_key: "agent".to_string(),
            binding_scope: scope.to_string(),
            owner_user_id: None,
            resource_kind: RESOURCE_KIND_MCP.to_string(),
            resource_id: "resource-1".to_string(),
            enabled: true,
            required: false,
            priority: 100,
            conditions: BindingConditions::default(),
            created_by: "user-1".to_string(),
            updated_by: "user-1".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn local_connector_record() -> McpRecord {
        McpRecord {
            id: "local-mcp-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_kind: OWNER_KIND_USER.to_string(),
            visibility: VISIBILITY_PRIVATE.to_string(),
            source_kind: SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED.to_string(),
            name: "user_mcp_manifest1".to_string(),
            display_name: "Local MCP".to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: RUNTIME_KIND_LOCAL_CONNECTOR_STDIO.to_string(),
                server_name: Some("user_mcp_manifest1".to_string()),
                local_connector: Some(LocalConnectorRef {
                    device_id: Some("device-1".to_string()),
                    workspace_id: None,
                    manifest_id: Some("manifest-1".to_string()),
                    relative_path: None,
                    requires_online: true,
                }),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            created_by: "local-connector-service".to_string(),
            updated_by: "local-connector-service".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn ordinary_users_can_only_choose_private_visibility() {
        let ordinary = user(USER_ROLE_USER);
        assert_eq!(
            normalize_visibility(Some(VISIBILITY_PRIVATE), &ordinary).unwrap(),
            VISIBILITY_PRIVATE
        );
        assert_eq!(
            normalize_visibility(Some(VISIBILITY_PUBLIC), &ordinary)
                .unwrap_err()
                .status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            normalize_visibility(Some(VISIBILITY_SYSTEM_PRIVATE), &ordinary)
                .unwrap_err()
                .status,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn super_admin_can_choose_public_and_system_private_visibility() {
        let admin = user(USER_ROLE_SUPER_ADMIN);
        assert_eq!(
            normalize_visibility(Some(VISIBILITY_PUBLIC), &admin).unwrap(),
            VISIBILITY_PUBLIC
        );
        assert_eq!(
            normalize_visibility(Some(VISIBILITY_SYSTEM_PRIVATE), &admin).unwrap(),
            VISIBILITY_SYSTEM_PRIVATE
        );
    }

    #[test]
    fn ordinary_users_cannot_write_for_another_owner() {
        let ordinary = user(USER_ROLE_USER);
        assert_eq!(
            requested_owner_user_id(Some("user-2"), &ordinary)
                .unwrap_err()
                .status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            requested_owner_user_id(Some("user-1"), &ordinary).unwrap(),
            "user-1"
        );
    }

    #[test]
    fn system_private_resources_require_system_or_global_binding() {
        assert!(resource_visible_in_runtime(
            "admin-id",
            VISIBILITY_SYSTEM_PRIVATE,
            "user-id",
            &binding(BINDING_SCOPE_SYSTEM_REQUIRED)
        ));
        assert!(resource_visible_in_runtime(
            "admin-id",
            VISIBILITY_SYSTEM_PRIVATE,
            "user-id",
            &binding(BINDING_SCOPE_GLOBAL_DEFAULT)
        ));
        assert!(!resource_visible_in_runtime(
            "admin-id",
            VISIBILITY_SYSTEM_PRIVATE,
            "user-id",
            &binding(BINDING_SCOPE_USER_OVERRIDE)
        ));
    }

    #[test]
    fn local_connector_mcp_requires_connector_reference() {
        let runtime = McpRuntime {
            kind: RUNTIME_KIND_LOCAL_CONNECTOR_STDIO.to_string(),
            command: Some("tool".to_string()),
            ..McpRuntime::default()
        };
        assert_eq!(
            validate_mcp_runtime(&runtime).unwrap_err().status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn local_connector_user_mcp_does_not_require_workspace() {
        let runtime = McpRuntime {
            kind: RUNTIME_KIND_LOCAL_CONNECTOR_STDIO.to_string(),
            server_name: Some("user_mcp_manifest1".to_string()),
            local_connector: Some(LocalConnectorRef {
                device_id: Some("device-1".to_string()),
                workspace_id: None,
                manifest_id: Some("manifest-1".to_string()),
                relative_path: None,
                requires_online: true,
            }),
            ..McpRuntime::default()
        };

        assert!(validate_mcp_runtime(&runtime).is_ok());
    }

    #[test]
    fn local_connector_user_mcp_scope_is_private_and_owner_isolated() {
        let record = local_connector_record();
        assert!(
            ensure_local_connector_record_scope(&record, "user-1", "device-1", "manifest-1",)
                .is_ok()
        );

        let mut public = record.clone();
        public.visibility = VISIBILITY_PUBLIC.to_string();
        assert_eq!(
            ensure_local_connector_record_scope(&public, "user-1", "device-1", "manifest-1",)
                .unwrap_err()
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ensure_local_connector_record_scope(&record, "user-2", "device-1", "manifest-1",)
                .unwrap_err()
                .status,
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn local_connector_status_rejects_manifest_hash_mismatch() {
        let check = ResourceCheckRecord {
            id: "mcp:local-mcp-1".to_string(),
            resource_kind: RESOURCE_KIND_MCP.to_string(),
            resource_id: "local-mcp-1".to_string(),
            owner_user_id: "user-1".to_string(),
            status: "available".to_string(),
            last_checked_at: now_rfc3339(),
            last_error: None,
            tool_snapshot: vec![json!({"name": "demo"})],
            manifest_hash: Some("hash-1".to_string()),
        };

        assert!(ensure_local_connector_manifest_hash_matches(Some(&check), Some("hash-1")).is_ok());
        assert_eq!(
            ensure_local_connector_manifest_hash_matches(Some(&check), Some("hash-2"))
                .unwrap_err()
                .status,
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn local_connector_availability_check_expires_after_ttl() {
        let now = chrono::Utc::now().to_rfc3339();
        let stale = (chrono::Utc::now() - chrono::Duration::seconds(61)).to_rfc3339();

        assert!(local_connector_check_is_fresh(
            now.as_str(),
            Duration::from_secs(60)
        ));
        assert!(!local_connector_check_is_fresh(
            stale.as_str(),
            Duration::from_secs(60)
        ));
        assert!(!local_connector_check_is_fresh(
            "invalid",
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn builtin_mcps_cannot_be_created_through_the_api() {
        let payload = McpPayload {
            runtime: Some(McpRuntime {
                kind: RUNTIME_KIND_BUILTIN.to_string(),
                builtin_kind: Some("Notepad".to_string()),
                ..McpRuntime::default()
            }),
            ..McpPayload::default()
        };
        assert_eq!(
            validate_client_managed_mcp_payload(&payload)
                .unwrap_err()
                .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn system_routed_mcps_cannot_be_created_through_the_api() {
        let payload = McpPayload {
            runtime: Some(McpRuntime {
                kind: RUNTIME_KIND_SYSTEM_ROUTED.to_string(),
                server_name: Some("sandbox_images".to_string()),
                ..McpRuntime::default()
            }),
            ..McpPayload::default()
        };
        assert_eq!(
            validate_client_managed_mcp_payload(&payload)
                .unwrap_err()
                .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn client_managed_mcps_cannot_claim_the_system_seed_source() {
        let payload = McpPayload {
            source_kind: Some(SOURCE_KIND_SYSTEM_SEED.to_string()),
            ..McpPayload::default()
        };
        assert_eq!(
            validate_client_managed_mcp_payload(&payload)
                .unwrap_err()
                .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn system_seed_mcps_only_allow_enabled_updates() {
        assert!(validate_system_seed_mcp_update(&McpPayload {
            enabled: Some(false),
            ..McpPayload::default()
        })
        .is_ok());

        assert_eq!(
            validate_system_seed_mcp_update(&McpPayload {
                name: Some("renamed".to_string()),
                ..McpPayload::default()
            })
            .unwrap_err()
            .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn mcp_binding_modes_are_limited_to_three_states() {
        assert!(validate_mcp_binding_mode(MCP_BINDING_MODE_DISABLED).is_ok());
        assert!(validate_mcp_binding_mode(MCP_BINDING_MODE_OPTIONAL).is_ok());
        assert!(validate_mcp_binding_mode(MCP_BINDING_MODE_REQUIRED).is_ok());
        assert_eq!(
            validate_mcp_binding_mode("conditional").unwrap_err().status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn disabled_mcp_bindings_are_persisted_but_excluded_from_runtime() {
        assert_eq!(
            mcp_binding_state(MCP_BINDING_MODE_DISABLED).unwrap(),
            (false, false, BINDING_SCOPE_GLOBAL_DEFAULT)
        );
        assert_eq!(
            mcp_binding_state(MCP_BINDING_MODE_OPTIONAL).unwrap(),
            (true, false, BINDING_SCOPE_GLOBAL_DEFAULT)
        );
        assert_eq!(
            mcp_binding_state(MCP_BINDING_MODE_REQUIRED).unwrap(),
            (true, true, BINDING_SCOPE_SYSTEM_REQUIRED)
        );
    }

    #[test]
    fn automatic_user_resources_are_optional_and_owner_scoped() {
        let binding = automatic_user_binding("task_runner_run_phase", "user-1", "mcp", "mcp-1");
        assert!(!binding.required);
        assert_eq!(binding.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(binding.resource_kind, RESOURCE_KIND_MCP);
    }

    #[tokio::test]
    async fn internal_capability_resolver_requires_secret() {
        let state = test_state_with_secret(Some("internal-secret")).await;

        let err = resolve_agent_capabilities_internal(
            State(state),
            HeaderMap::new(),
            Json(runtime_request("owner-1")),
        )
        .await
        .expect_err("missing secret should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "missing plugin management internal API secret");
    }

    #[tokio::test]
    async fn internal_capability_resolver_rejects_wrong_secret() {
        let state = test_state_with_secret(Some("internal-secret")).await;
        let mut headers = internal_headers();
        headers.insert(
            "x-plugin-management-internal-secret",
            HeaderValue::from_static("wrong-secret"),
        );

        let err = resolve_agent_capabilities_internal(
            State(state),
            headers,
            Json(runtime_request("owner-1")),
        )
        .await
        .expect_err("wrong secret should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "invalid plugin management internal API secret");
    }

    #[tokio::test]
    async fn internal_capability_resolver_requires_owner() {
        let state = test_state_with_secret(Some("internal-secret")).await;

        let err = resolve_agent_capabilities_internal(
            State(state),
            internal_headers(),
            Json(runtime_request("  ")),
        )
        .await
        .expect_err("missing owner should fail");

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "owner_user_id is required");
    }

    #[tokio::test]
    async fn internal_capability_resolver_requires_caller_service() {
        let state = test_state_with_secret(Some("internal-secret")).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-plugin-management-internal-secret",
            HeaderValue::from_static("internal-secret"),
        );

        let err = resolve_agent_capabilities_internal(
            State(state),
            headers,
            Json(runtime_request("owner-1")),
        )
        .await
        .expect_err("missing caller should fail");

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "caller service is required");
    }

    #[tokio::test]
    async fn internal_capability_resolver_rejects_unknown_caller_service() {
        let state = test_state_with_secret(Some("internal-secret")).await;
        let mut headers = internal_headers();
        headers.insert(
            "x-plugin-management-caller-service",
            HeaderValue::from_static("unknown-service"),
        );

        let err = resolve_agent_capabilities_internal(
            State(state),
            headers,
            Json(runtime_request("owner-1")),
        )
        .await
        .expect_err("unknown caller should fail");

        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.message, "caller service is not allowed");
    }

    fn runtime_request(owner_user_id: &str) -> RuntimeCapabilitiesRequest {
        RuntimeCapabilitiesRequest {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: owner_user_id.to_string(),
            include_unavailable: true,
        }
    }

    fn internal_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-plugin-management-internal-secret",
            HeaderValue::from_static("internal-secret"),
        );
        headers.insert(
            "x-plugin-management-caller-service",
            HeaderValue::from_static("task-runner"),
        );
        headers
    }

    async fn test_state_with_secret(internal_api_secret: Option<&str>) -> AppState {
        let client = Client::with_uri_str("mongodb://127.0.0.1:27017")
            .await
            .expect("create MongoDB client");
        let store = AppStore::new(client.database("plugin_management_api_unit_test"));
        AppState {
            config: AppConfig {
                host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 0,
                database_url: "mongodb://127.0.0.1:27017".to_string(),
                mongodb_database: "plugin_management_api_unit_test".to_string(),
                user_service_base_url: "http://127.0.0.1:39190".to_string(),
                user_service_request_timeout: Duration::from_secs(1),
                internal_api_secret: internal_api_secret.map(ToOwned::to_owned),
                local_connector_check_ttl: Duration::from_secs(60),
                local_connector_max_tool_snapshot_bytes: 512 * 1024,
                super_admin_username: "admin".to_string(),
                super_admin_password: "admin".to_string(),
                seed_system_resources: false,
            },
            store,
        }
    }
}
