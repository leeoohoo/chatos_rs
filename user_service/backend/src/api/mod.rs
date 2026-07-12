// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::{
    bearer_token_from_headers, decode_any_user_service_token, unauthorized, CurrentPrincipal,
};
use crate::models::{PRINCIPAL_TYPE_AGENT_ACCOUNT, PRINCIPAL_TYPE_HUMAN_USER};
use crate::state::AppState;

mod agents;
mod auth;
mod harness;
mod internal_auth;
mod internal_models;
mod invite_codes;
mod models;
mod system;
mod token_exchange;
mod users;

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/verify", get(auth::verify))
        .route("/api/auth/logout", post(auth::logout))
        .route(
            "/api/auth/local-connector-ticket",
            post(auth::issue_local_connector_ticket),
        )
        .route(
            "/api/invite-codes",
            get(invite_codes::list_invite_codes).post(invite_codes::create_invite_code),
        )
        .route(
            "/api/invite-codes/:id/revoke",
            post(invite_codes::revoke_invite_code),
        )
        .route(
            "/api/users",
            get(users::list_users).post(users::create_user),
        )
        .route("/api/users/:id", patch(users::update_user))
        .route(
            "/api/users/:id/harness-provisioning",
            post(users::provision_harness_user),
        )
        .route(
            "/api/users/:id/harness-provisioning/retry",
            post(users::retry_harness_provisioning),
        )
        .route(
            "/api/agent-accounts",
            get(agents::list_agent_accounts).post(agents::create_agent_account),
        )
        .route(
            "/api/agent-accounts/:id",
            patch(agents::update_agent_account),
        )
        .route(
            "/api/agent-accounts/:id/reset-password",
            post(agents::reset_agent_password),
        )
        .route(
            "/api/model-configs",
            get(models::list_model_configs).post(models::create_model_config),
        )
        .route(
            "/api/model-providers",
            get(models::list_model_providers).post(models::create_model_provider),
        )
        .route(
            "/api/model-providers/:id",
            get(models::get_model_provider)
                .patch(models::update_model_provider)
                .delete(models::delete_model_provider),
        )
        .route(
            "/api/model-providers/:id/refresh",
            post(models::refresh_model_provider_models),
        )
        .route(
            "/api/model-configs/settings",
            get(models::get_model_settings).put(models::put_model_settings),
        )
        .route(
            "/api/model-configs/:id",
            get(models::get_model_config)
                .patch(models::update_model_config)
                .delete(models::delete_model_config),
        )
        .route(
            "/api/model-configs/:id/refresh",
            post(models::refresh_model_config_provider_models),
        )
        .route(
            "/api/token/exchange/task-runner",
            post(token_exchange::exchange_task_runner_token),
        )
        .route(
            "/api/token/exchange/agent",
            post(token_exchange::exchange_task_runner_token),
        )
        .route(
            "/api/internal/harness/repos",
            post(harness::create_project_repo),
        )
        .route("/api/system/config", get(system::get_system_config))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(system::health))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/register", post(auth::register))
        .route(
            "/api/auth/register/send-code",
            post(auth::send_register_email_code),
        )
        .route(
            "/api/auth/local-connector-ticket/exchange",
            post(auth::exchange_local_connector_ticket),
        )
        .route(
            "/api/internal/harness/users/:user_id/access",
            get(harness::get_user_harness_access),
        )
        .route(
            "/api/internal/users/:user_id/model-configs/:model_config_id/runtime",
            get(internal_models::get_user_model_runtime_config),
        )
        .route(
            "/api/internal/users/:user_id/model-settings",
            get(internal_models::get_user_model_settings),
        )
        .merge(protected)
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

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<Value>)> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let token = bearer_token_from_headers(request.headers()).map_err(|err| unauthorized(&err))?;
    let claims = decode_any_user_service_token(token.as_str(), &state.config)
        .map_err(|_| unauthorized("invalid or expired token"))?;
    if state
        .store
        .is_token_revoked(claims.jti.as_str())
        .await
        .map_err(internal_error)?
    {
        return Err(unauthorized("token has been revoked"));
    }

    let principal = CurrentPrincipal::from(claims);
    ensure_principal_active(&state, &principal).await?;

    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

async fn ensure_principal_active(
    state: &AppState,
    principal: &CurrentPrincipal,
) -> Result<(), (StatusCode, Json<Value>)> {
    match principal.principal_type.as_str() {
        PRINCIPAL_TYPE_HUMAN_USER => {
            let Some(user_id) = principal.user_id.as_deref() else {
                return Err(unauthorized("token missing user identity"));
            };
            let Some(user) = state
                .store
                .find_user_by_id(user_id)
                .await
                .map_err(internal_error)?
            else {
                return Err(unauthorized("user not found"));
            };
            if !user.enabled {
                return Err(unauthorized("user has been disabled"));
            }
            Ok(())
        }
        PRINCIPAL_TYPE_AGENT_ACCOUNT => {
            let Some(agent_account_id) = principal.agent_account_id.as_deref() else {
                return Err(unauthorized("token missing agent identity"));
            };
            let Some(agent) = state
                .store
                .find_agent_by_id(agent_account_id)
                .await
                .map_err(internal_error)?
            else {
                return Err(unauthorized("agent account not found"));
            };
            if !agent.enabled {
                return Err(unauthorized("agent account has been disabled"));
            }
            let Some(owner) = state
                .store
                .find_user_by_id(agent.owner_user_id.as_str())
                .await
                .map_err(internal_error)?
            else {
                return Err(unauthorized("agent owner not found"));
            };
            if !owner.enabled {
                return Err(unauthorized("agent owner has been disabled"));
            }
            Ok(())
        }
        _ => Err(unauthorized("unsupported principal type")),
    }
}

pub type ApiResult<T> = Result<Json<T>, (StatusCode, Json<Value>)>;
pub type ApiStatusResult = Result<StatusCode, (StatusCode, Json<Value>)>;

pub fn bad_request(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    error(StatusCode::BAD_REQUEST, message)
}

pub fn forbidden(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    error(StatusCode::FORBIDDEN, message)
}

pub fn not_found(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    error(StatusCode::NOT_FOUND, message)
}

pub fn internal_error(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, message)
}

pub fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}

pub fn require_super_admin(principal: &CurrentPrincipal) -> Result<(), (StatusCode, Json<Value>)> {
    if principal.is_super_admin() {
        Ok(())
    } else {
        Err(forbidden("super_admin permission required"))
    }
}
