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
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
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

fn protected_api(state: AppState) -> Router<AppState> {
    Router::new()
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
            "/api/invite-codes/{id}/revoke",
            post(invite_codes::revoke_invite_code),
        )
        .route(
            "/api/users",
            get(users::list_users).post(users::create_user),
        )
        .route("/api/users/{id}", patch(users::update_user))
        .route(
            "/api/users/{id}/harness-provisioning",
            post(users::provision_harness_user),
        )
        .route(
            "/api/users/{id}/harness-provisioning/retry",
            post(users::retry_harness_provisioning),
        )
        .route(
            "/api/agent-accounts",
            get(agents::list_agent_accounts).post(agents::create_agent_account),
        )
        .route(
            "/api/agent-accounts/{id}",
            patch(agents::update_agent_account),
        )
        .route(
            "/api/agent-accounts/{id}/reset-password",
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
            "/api/model-providers/{id}",
            get(models::get_model_provider)
                .patch(models::update_model_provider)
                .delete(models::delete_model_provider),
        )
        .route(
            "/api/model-providers/{id}/refresh",
            post(models::refresh_model_provider_models),
        )
        .route(
            "/api/model-configs/settings",
            get(models::get_model_settings).put(models::put_model_settings),
        )
        .route(
            "/api/model-configs/{id}",
            get(models::get_model_config)
                .patch(models::update_model_config)
                .delete(models::delete_model_config),
        )
        .route(
            "/api/model-configs/{id}/refresh",
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
        .route("/api/system/config", get(system::get_system_config))
        .route_layer(middleware::from_fn_with_state(state, require_auth))
}

pub fn build_public_router(state: AppState) -> Router {
    apply_common_layers(
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
            .merge(protected_api(state.clone()))
            .with_state(state),
        "public",
    )
}

pub fn build_internal_router(state: AppState) -> Router {
    let protected_internal = Router::new()
        .route(
            "/api/internal/harness/repos",
            post(harness::create_project_repo),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));
    let internal_harness_repo_write = Router::new()
        .route(
            "/api/internal/harness/users/{user_id}/repos",
            post(harness::create_project_repo_for_user),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_harness_repo_write_internal,
        ));
    apply_common_layers(
        Router::new()
            .route(
                "/api/internal/harness/users/{user_id}/access",
                get(harness::get_user_harness_access),
            )
            .route(
                "/api/internal/users/{user_id}/model-configs/{model_config_id}/runtime",
                get(internal_models::get_user_model_runtime_config),
            )
            .route(
                "/api/internal/users/{user_id}/model-settings",
                get(internal_models::get_user_model_settings),
            )
            .route(
                "/api/internal/task-runner/model-configs",
                get(internal_models::list_task_model_configs),
            )
            .route(
                "/api/internal/task-runner/model-configs/{model_config_id}",
                get(internal_models::get_task_model_config),
            )
            .merge(internal_harness_repo_write)
            .merge(protected_internal)
            .with_state(state),
        "internal",
    )
}

fn apply_common_layers(router: Router, surface: &'static str) -> Router {
    router
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(middleware::from_fn(
            crate::trace_context::accept_remote_parent,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(move |request: &axum::http::Request<axum::body::Body>| {
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

async fn require_harness_repo_write_internal(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<Value>)> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    internal_auth::require_project_service_internal_request(
        &state.config,
        request.headers(),
        internal_auth::HARNESS_REPO_WRITE_SCOPE,
    )?;
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

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use reqwest::StatusCode;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use super::{build_internal_router, build_public_router};
    use crate::config::AppConfig;
    use crate::state::AppState;

    fn test_config() -> AppConfig {
        AppConfig {
            host: "127.0.0.1".parse().expect("test host"),
            port: 39190,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 1.0,
            otlp_export_timeout: std::time::Duration::from_secs(1),
            database_url: "mongodb://127.0.0.1:1/user_router_tests".to_string(),
            mongodb_database: "user_router_tests".to_string(),
            jwt_secret: "test-secret".to_string(),
            jwt_issuer: "user_service".to_string(),
            user_service_audience: "user_service".to_string(),
            task_runner_audience: "task_runner".to_string(),
            user_access_ttl_seconds: 3600,
            task_runner_access_ttl_seconds: 3600,
            super_admin_username: "admin".to_string(),
            super_admin_password: "password".to_string(),
            super_admin_display_name: "Admin".to_string(),
            memory_engine_base_url: None,
            memory_engine_operator_token: None,
            memory_engine_mtls_ca_cert_path: None,
            memory_engine_mtls_client_identity_path: None,
            task_runner_internal_api_secret: None,
            downstream_request_timeout_ms: 5000,
            harness_provisioning_enabled: false,
            harness_base_url: None,
            harness_synthetic_email_domain: "chatos.local".to_string(),
            harness_space_prefix: "u-".to_string(),
            harness_request_timeout_ms: 5000,
            harness_project_pat_prefix: "chatos-project".to_string(),
            user_service_internal_api_secret: Some("test-project-service-secret".to_string()),
            chatos_internal_api_secret: Some("test-chatos-service-secret".to_string()),
            smtp_host: None,
            smtp_port: 587,
            smtp_username: None,
            smtp_password: None,
            email_from: None,
            email_from_name: "Chat OS".to_string(),
            registration_code_ttl_seconds: 600,
            registration_code_resend_seconds: 60,
            registration_code_hourly_limit: 5,
            registration_code_max_attempts: 5,
            login_max_failed_attempts: 3,
            login_failure_window_seconds: 300,
            login_lockout_seconds: 120,
        }
    }

    async fn test_state() -> AppState {
        AppState::new_without_external_dependencies(test_config())
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
        for (method, path) in [
            (reqwest::Method::POST, "/api/internal/harness/repos"),
            (
                reqwest::Method::POST,
                "/api/internal/harness/users/user-1/repos",
            ),
            (
                reqwest::Method::GET,
                "/api/internal/harness/users/user-1/access",
            ),
            (
                reqwest::Method::GET,
                "/api/internal/users/user-1/model-settings",
            ),
        ] {
            let status = client
                .request(method, format!("{base_url}{path}"))
                .send()
                .await
                .expect("request public router")
                .status();
            assert_eq!(status, StatusCode::NOT_FOUND, "unexpected route: {path}");
        }
        assert_eq!(
            client
                .get(format!("{base_url}/api/health"))
                .send()
                .await
                .expect("request public health")
                .status(),
            StatusCode::OK
        );
        server.abort();
    }

    #[tokio::test]
    async fn internal_router_does_not_expose_public_user_routes() {
        let (base_url, server) = spawn_router(build_internal_router(test_state().await)).await;
        let client = reqwest::Client::new();
        for (method, path) in [
            (reqwest::Method::GET, "/api/health"),
            (reqwest::Method::POST, "/api/auth/login"),
            (reqwest::Method::GET, "/api/users"),
        ] {
            let status = client
                .request(method, format!("{base_url}{path}"))
                .send()
                .await
                .expect("request internal router")
                .status();
            assert_eq!(status, StatusCode::NOT_FOUND, "unexpected route: {path}");
        }
        for (method, path) in [
            (reqwest::Method::POST, "/api/internal/harness/repos"),
            (
                reqwest::Method::POST,
                "/api/internal/harness/users/user-1/repos",
            ),
            (
                reqwest::Method::GET,
                "/api/internal/harness/users/user-1/access",
            ),
            (
                reqwest::Method::GET,
                "/api/internal/users/user-1/model-settings",
            ),
        ] {
            let status = client
                .request(method, format!("{base_url}{path}"))
                .send()
                .await
                .expect("request internal control plane")
                .status();
            assert_eq!(status, StatusCode::UNAUTHORIZED, "missing route: {path}");
        }
        server.abort();
    }
}
