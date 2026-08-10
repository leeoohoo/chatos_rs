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
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
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
#[path = "api/runtime_capabilities.rs"]
mod capabilities;
mod internal_auth;
mod local_connector;
mod local_connector_skills;
#[path = "api/catalog/mcps.rs"]
mod mcps;
mod plugin_audit;
mod plugin_catalog_sync;
mod plugin_cloud_bundles;
#[path = "api/oauth/plugin_cloud_credentials.rs"]
mod plugin_cloud_credentials;
#[path = "api/oauth/plugin_cloud_oauth.rs"]
mod plugin_cloud_oauth;
mod plugin_install_sources;
#[path = "api/installations/plugin_installations.rs"]
mod plugin_installations;
mod plugin_marketplaces;
#[path = "api/oauth/plugin_oauth.rs"]
mod plugin_oauth;
mod plugin_publishers;
mod plugin_releases;
mod plugin_support;
mod plugins;
mod queue_operations;
mod resource_policy;
mod runtime_agent_prompts;
#[path = "api/catalog/skill_packages.rs"]
mod skill_packages;
#[path = "api/catalog/skills.rs"]
mod skills;
mod system;

use agent_provider_prompts::{
    agent_prompt_completeness, generate_agent_provider_prompt, get_agent_prompt_version,
    list_agent_prompt_versions, list_agent_provider_prompts, publish_agent_provider_prompt,
    update_agent_provider_prompt_draft,
};
use agents::{
    create_system_agent, get_agent_mcp_bindings, get_agent_plugin_bindings, list_system_agents,
    update_agent_mcp_bindings, update_agent_plugin_bindings, update_system_agent,
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
use plugin_audit::list_plugin_audit;
pub(crate) use plugin_catalog_sync::{
    is_syncable_network_marketplace, run_queued_plugin_catalog_sync,
};
use plugin_catalog_sync::{sync_admin_plugin_marketplace, sync_plugin_marketplace};
use plugin_cloud_bundles::{
    get_plugin_cloud_component_bundle_internal, get_plugin_mcp_cloud_runtime_bundle_internal,
    list_plugin_mcp_cloud_runtime_metadata,
};
use plugin_cloud_credentials::{
    delete_plugin_cloud_credential, delete_plugin_cloud_oauth_connection,
    list_plugin_cloud_credentials, list_plugin_cloud_oauth_connections,
    resolve_plugin_mcp_cloud_credentials_internal, upsert_plugin_cloud_credential,
    upsert_plugin_cloud_oauth_connection,
};
use plugin_cloud_oauth::{
    begin_plugin_cloud_oauth_authorization, complete_plugin_cloud_oauth_authorization,
};
use plugin_install_sources::{
    get_plugin_install_source_internal, list_plugin_install_sources_internal,
};
use plugin_installations::{list_installed_plugins, sync_plugin_installation_internal};
use plugin_marketplaces::{
    create_admin_plugin_marketplace, create_plugin_marketplace, list_admin_plugin_marketplaces,
    list_plugin_marketplaces, update_admin_plugin_marketplace,
};
use plugin_oauth::{list_plugin_oauth_connections, sync_plugin_oauth_status_internal};
use plugin_publishers::{
    list_admin_plugin_publishers, list_plugin_publishers, review_admin_plugin_publisher,
    submit_plugin_publisher,
};
use plugin_releases::{create_plugin_release, list_plugin_releases, revoke_plugin_release};
use plugin_support::*;
use plugins::{
    create_plugin_catalog_entry, get_plugin_catalog_entry, list_admin_plugins, list_plugin_catalog,
    update_user_plugin_preference, update_user_plugin_preference_internal,
};
use queue_operations::replay_catalog_sync_dead_letter;
use resource_policy::*;
use runtime_agent_prompts::{
    agent_prompt_bundle_internal, agent_prompt_bundle_manifest_internal,
    resolve_agent_prompt_internal,
};
use skill_packages::{get_skill_package, list_skill_packages};
use skills::{check_skill, get_skill, list_skills};
use system::{get_system_stats, prometheus_metrics};

const ALLOWED_INTERNAL_CALLER_SERVICES: &[&str] = &[
    "chatos-backend",
    "task-runner",
    "project-service",
    "local-connector-service",
    "memory-engine",
    "mcp-management-service",
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

pub fn build_public_router(state: AppState) -> Router {
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
        .route("/api/skills", get(list_skills))
        .route("/api/skills/{skill_id}", get(get_skill))
        .route("/api/skills/{skill_id}/check", post(check_skill))
        .route("/api/skill-packages", get(list_skill_packages))
        .route("/api/skill-packages/{package_id}", get(get_skill_package))
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
            "/api/system-agents/{agent_key}/prompt-versions",
            get(list_agent_prompt_versions),
        )
        .route(
            "/api/system-agents/{agent_key}/prompt-versions/{bundle_version}",
            get(get_agent_prompt_version),
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
            "/api/system-agents/{agent_key}/plugin-bindings",
            get(get_agent_plugin_bindings).put(update_agent_plugin_bindings),
        )
        .route(
            "/api/runtime/agent-capabilities",
            get(resolve_agent_capabilities),
        )
        .route("/api/plugins/catalog", get(list_plugin_catalog))
        .route(
            "/api/plugins/catalog/{plugin_id}",
            get(get_plugin_catalog_entry),
        )
        .route("/api/plugins/installed", get(list_installed_plugins))
        .route(
            "/api/plugins/{plugin_id}/releases",
            get(list_plugin_releases),
        )
        .route(
            "/api/plugins/{plugin_id}/releases/{release_id}/cloud-mcp-runtimes",
            get(list_plugin_mcp_cloud_runtime_metadata),
        )
        .route(
            "/api/plugins/{plugin_id}/preference",
            axum::routing::put(update_user_plugin_preference),
        )
        .route(
            "/api/plugins/{plugin_id}/oauth",
            get(list_plugin_oauth_connections),
        )
        .route(
            "/api/plugins/{plugin_id}/cloud-credentials",
            get(list_plugin_cloud_credentials),
        )
        .route(
            "/api/plugins/{plugin_id}/releases/{release_id}/cloud-credentials/{component_key}/{secret_name}",
            axum::routing::put(upsert_plugin_cloud_credential)
                .delete(delete_plugin_cloud_credential),
        )
        .route(
            "/api/plugins/{plugin_id}/cloud-oauth",
            get(list_plugin_cloud_oauth_connections),
        )
        .route(
            "/api/plugins/{plugin_id}/releases/{release_id}/cloud-oauth/{component_key}",
            axum::routing::put(upsert_plugin_cloud_oauth_connection),
        )
        .route(
            "/api/plugins/{plugin_id}/releases/{release_id}/cloud-oauth/{component_key}/authorize",
            post(begin_plugin_cloud_oauth_authorization),
        )
        .route(
            "/api/plugins/{plugin_id}/cloud-oauth/{connection_id}",
            axum::routing::delete(delete_plugin_cloud_oauth_connection),
        )
        .route(
            "/api/plugin-marketplaces",
            get(list_plugin_marketplaces).post(create_plugin_marketplace),
        )
        .route(
            "/api/plugin-marketplaces/{marketplace_id}/sync",
            post(sync_plugin_marketplace),
        )
        .route(
            "/api/plugin-publishers",
            get(list_plugin_publishers).post(submit_plugin_publisher),
        )
        .route(
            "/api/admin/plugin-marketplaces",
            get(list_admin_plugin_marketplaces).post(create_admin_plugin_marketplace),
        )
        .route(
            "/api/admin/plugin-marketplaces/{marketplace_id}",
            patch(update_admin_plugin_marketplace),
        )
        .route(
            "/api/admin/plugin-marketplaces/{marketplace_id}/sync",
            post(sync_admin_plugin_marketplace),
        )
        .route(
            "/api/admin/queue-operations/catalog-sync/replay",
            post(replay_catalog_sync_dead_letter),
        )
        .route(
            "/api/admin/plugin-publishers",
            get(list_admin_plugin_publishers),
        )
        .route(
            "/api/admin/plugin-publishers/{publisher_record_id}/review",
            patch(review_admin_plugin_publisher),
        )
        .route(
            "/api/admin/plugins",
            get(list_admin_plugins).post(create_plugin_catalog_entry),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/releases",
            get(list_plugin_releases).post(create_plugin_release),
        )
        .route(
            "/api/admin/plugin-releases/{release_id}/revoke",
            post(revoke_plugin_release),
        )
        .route("/api/admin/plugin-audit", get(list_plugin_audit))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    apply_common_layers(
        Router::new()
            .route("/api/health", get(health_handler))
            .route("/metrics", get(prometheus_metrics))
            .route("/api/auth/login", post(login_handler))
            .route(
                "/api/plugins/cloud-oauth/callback",
                get(complete_plugin_cloud_oauth_authorization),
            )
            .merge(protected_api)
            .with_state(state),
    )
    .layer(cors)
}

pub fn build_internal_router(state: AppState) -> Router {
    let internal_api = Router::new()
        .route("/api/internal/system/stats", get(get_system_stats))
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
            "/api/internal/plugins/{plugin_id}/releases/{release_id}/cloud-components/{component_key}",
            get(get_plugin_cloud_component_bundle_internal),
        )
        .route(
            "/api/internal/plugins/{plugin_id}/releases/{release_id}/cloud-mcp-components/{component_key}",
            get(get_plugin_mcp_cloud_runtime_bundle_internal),
        )
        .route(
            "/api/internal/plugins/{plugin_id}/releases/{release_id}/cloud-mcp-components/{component_key}/credentials",
            post(resolve_plugin_mcp_cloud_credentials_internal),
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
            "/api/internal/local-connector/plugins/installations",
            axum::routing::put(sync_plugin_installation_internal),
        )
        .route(
            "/api/internal/local-connector/plugins/install-sources",
            get(list_plugin_install_sources_internal),
        )
        .route(
            "/api/internal/local-connector/plugins/install-sources/{plugin_id}/{release_id}",
            get(get_plugin_install_source_internal),
        )
        .route(
            "/api/internal/local-connector/plugins/{plugin_id}/preference",
            axum::routing::put(update_user_plugin_preference_internal),
        )
        .route(
            "/api/internal/local-connector/plugins/oauth",
            axum::routing::put(sync_plugin_oauth_status_internal),
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

    apply_common_layers(internal_api.with_state(state))
}

fn apply_common_layers(router: Router) -> Router {
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<axum::body::Body>| {
                    tracing::debug_span!(
                        "http_request",
                        method = %request.method(),
                        path = %request.uri().path(),
                    )
                })
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
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
    let user =
        verify_token_via_user_service(&state.config, state.user_service_http(), token.as_str())
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
        service: "plugin_management_service".to_string(),
    })
}

async fn login_handler(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    login_via_user_service(&state.config, state.user_service_http(), &input)
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

async fn current_user_handler(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

#[cfg(test)]
mod tests;
