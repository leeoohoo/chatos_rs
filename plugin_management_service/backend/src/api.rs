// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;
use uuid::Uuid;

use crate::auth::{
    bearer_token_from_headers, login_via_user_service, verify_token_via_user_service, AccessToken,
};
use crate::models::*;
use crate::state::AppState;
use crate::store::{normalized, now_rfc3339};

mod agent_provider_prompts;
mod agents;
mod availability;
mod capabilities;
mod internal_auth;
mod local_connector;
mod local_connector_skills;
mod mcps;
mod resource_policy;
mod runtime_agent_prompts;
mod skill_packages;
mod skills;

use agent_provider_prompts::{
    agent_prompt_completeness, generate_agent_provider_prompt, list_agent_provider_prompts,
    publish_agent_provider_prompt, update_agent_provider_prompt_draft,
};
use agents::{
    create_system_agent, get_agent_mcp_bindings, list_system_agents, update_agent_mcp_bindings,
    update_system_agent,
};
use availability::*;
#[cfg(test)]
use capabilities::automatic_user_binding;
use capabilities::{resolve_agent_capabilities, resolve_agent_capabilities_internal};
use internal_auth::*;
use local_connector::{
    delete_local_connector_mcp_internal, list_local_connector_mcps_internal,
    sync_local_connector_mcp_internal, truncate_text, update_local_connector_mcp_internal,
    update_local_connector_mcp_status_batch_internal, update_local_connector_mcp_status_internal,
};
#[cfg(test)]
use local_connector::{
    ensure_local_connector_manifest_hash_matches, ensure_local_connector_record_scope,
};
use local_connector_skills::{
    list_user_skill_catalog_internal, sync_skill_inventory_internal,
    update_user_skill_preference_internal,
};
use mcps::{
    check_mcp, create_mcp, delete_mcp, get_mcp, get_mcp_descriptor, list_admin_ai_models,
    list_mcps, optimize_mcp_provider_skill, optimize_mcp_provider_skill_stream, update_mcp,
    update_mcp_provider_skill,
};
use resource_policy::*;
use runtime_agent_prompts::{
    agent_prompt_bundle_internal, agent_prompt_bundle_manifest_internal,
    resolve_agent_prompt_internal,
};
use skill_packages::{
    create_skill_package, delete_skill_package, get_skill_package, list_skill_packages,
    update_skill_package,
};
use skills::{check_skill, create_skill, delete_skill, get_skill, list_skills, update_skill};

const ALLOWED_INTERNAL_CALLER_SERVICES: &[&str] = &[
    "chatos-backend",
    "task-runner",
    "project-service",
    "local-connector-service",
];

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
    let cors = plugin_management_cors(&state.config.cors_origins);
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/mcps", get(list_mcps).post(create_mcp))
        .route(
            "/api/mcps/{mcp_id}",
            get(get_mcp).patch(update_mcp).delete(delete_mcp),
        )
        .route("/api/mcps/{mcp_id}/check", post(check_mcp))
        .route("/api/mcps/{mcp_id}/descriptor", get(get_mcp_descriptor))
        .route("/api/admin/ai-models", get(list_admin_ai_models))
        .route(
            "/api/mcps/{mcp_id}/provider-skills/optimize",
            post(optimize_mcp_provider_skill),
        )
        .route(
            "/api/mcps/{mcp_id}/provider-skills/optimize/stream",
            post(optimize_mcp_provider_skill_stream),
        )
        .route(
            "/api/mcps/{mcp_id}/provider-skills/{skill_id}",
            axum::routing::put(update_mcp_provider_skill),
        )
        .route("/api/skills", get(list_skills).post(create_skill))
        .route(
            "/api/skills/{skill_id}",
            get(get_skill).patch(update_skill).delete(delete_skill),
        )
        .route("/api/skills/{skill_id}/check", post(check_skill))
        .route(
            "/api/skill-packages",
            get(list_skill_packages).post(create_skill_package),
        )
        .route(
            "/api/skill-packages/{package_id}",
            get(get_skill_package)
                .patch(update_skill_package)
                .delete(delete_skill_package),
        )
        .route(
            "/api/system-agents",
            get(list_system_agents).post(create_system_agent),
        )
        .route("/api/system-agents/{agent_key}", patch(update_system_agent))
        .route(
            "/api/system-agents/{agent_key}/provider-prompts",
            get(list_agent_provider_prompts),
        )
        .route(
            "/api/system-agents/{agent_key}/provider-prompts/{vendor}/draft",
            axum::routing::put(update_agent_provider_prompt_draft),
        )
        .route(
            "/api/system-agents/{agent_key}/provider-prompts/{vendor}/publish",
            post(publish_agent_provider_prompt),
        )
        .route(
            "/api/system-agents/{agent_key}/provider-prompts/{vendor}/generate",
            post(generate_agent_provider_prompt),
        )
        .route(
            "/api/system-agents/prompt-completeness",
            get(agent_prompt_completeness),
        )
        .route(
            "/api/system-agents/{agent_key}/mcp-bindings",
            get(get_agent_mcp_bindings).put(update_agent_mcp_bindings),
        )
        .route(
            "/api/runtime/agent-capabilities",
            get(resolve_agent_capabilities),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let internal_api = Router::new()
        .route(
            "/api/internal/runtime/agent-prompts/resolve",
            post(resolve_agent_prompt_internal),
        )
        .route(
            "/api/internal/runtime/agent-prompts/manifest",
            get(agent_prompt_bundle_manifest_internal),
        )
        .route(
            "/api/internal/runtime/agent-prompts/bundle",
            get(agent_prompt_bundle_internal),
        )
        .route(
            "/api/internal/runtime/agent-capabilities/resolve",
            post(resolve_agent_capabilities_internal),
        )
        .route(
            "/api/internal/local-connector/mcps",
            get(list_local_connector_mcps_internal).post(sync_local_connector_mcp_internal),
        )
        .route(
            "/api/internal/local-connector/skills/catalog",
            get(list_user_skill_catalog_internal),
        )
        .route(
            "/api/internal/local-connector/skills/inventory",
            axum::routing::put(sync_skill_inventory_internal),
        )
        .route(
            "/api/internal/local-connector/skills/{skill_id}/preference",
            axum::routing::put(update_user_skill_preference_internal),
        )
        .route(
            "/api/internal/local-connector/mcps/{mcp_id}",
            patch(update_local_connector_mcp_internal).delete(delete_local_connector_mcp_internal),
        )
        .route(
            "/api/internal/local-connector/mcps/{mcp_id}/status",
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
        .layer(cors)
}

fn plugin_management_cors(configured_origins: &[String]) -> CorsLayer {
    let origins = configured_origins
        .iter()
        .filter_map(|value| HeaderValue::from_str(value).ok())
        .collect::<Vec<_>>();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
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
    if has_legacy_query_token(request.uri().query()) {
        return Err(
            "URL query access tokens are not supported; use Authorization header".to_string(),
        );
    }
    bearer_token_from_headers(request.headers()).map(ToOwned::to_owned)
}

fn has_legacy_query_token(query: Option<&str>) -> bool {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .any(|pair| {
            let mut parts = pair.splitn(2, '=');
            matches!(parts.next(), Some("access_token" | "token"))
                && parts.next().is_some_and(|value| !value.trim().is_empty())
        })
}

#[cfg(test)]
mod auth_query_tests {
    use super::has_legacy_query_token;

    #[test]
    fn detects_legacy_query_tokens() {
        assert!(has_legacy_query_token(Some("access_token=token-1")));
        assert!(has_legacy_query_token(Some("token=token-1")));
        assert!(!has_legacy_query_token(Some("plain=value")));
    }
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

#[cfg(test)]
mod tests;
