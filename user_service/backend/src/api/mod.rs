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
    bearer_token_from_headers, decode_user_service_token, unauthorized, CurrentPrincipal,
};
use crate::state::AppState;

mod agents;
mod auth;
mod models;
mod system;
mod token_exchange;
mod users;

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/logout", post(auth::logout))
        .route(
            "/api/users",
            get(users::list_users).post(users::create_user),
        )
        .route("/api/users/:id", patch(users::update_user))
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
            "/api/token/exchange/task-runner",
            post(token_exchange::exchange_task_runner_token),
        )
        .route("/api/system/config", get(system::get_system_config))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(system::health))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/register", post(auth::register))
        .merge(protected)
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

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<Value>)> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let token = bearer_token_from_headers(request.headers()).map_err(|err| unauthorized(&err))?;
    let claims = decode_user_service_token(token.as_str(), &state.config)
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
    if principal.is_human_user() {
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
    }

    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
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
