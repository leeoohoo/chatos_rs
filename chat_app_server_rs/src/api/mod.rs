use axum::body::Body;
use axum::extract::{DefaultBodyLimit, OriginalUri};
use axum::http::{
    header::{HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, UPGRADE},
    Request, StatusCode,
};
use axum::middleware;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde_json::json;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, info_span};

use crate::config::Config;
use crate::core::auth::{
    auth_user_from_access_token, auth_user_from_headers, sign_access_token, AuthHeaderError,
    AuthUser,
};

static START_TIME: Lazy<Instant> = Lazy::new(Instant::now);
static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
static ACCESS_TOKEN_HEADER: HeaderName = HeaderName::from_static("x-access-token");
static ACCESS_TOKEN_EXPIRES_IN_HEADER: HeaderName =
    HeaderName::from_static("x-access-token-expires-in");

pub mod agents;
pub mod agents_v3;
pub mod applications;
pub mod auth;
pub mod chat_agent_v2;
pub mod chat_v2;
pub mod chat_v3;
pub mod configs;
pub mod fs;
pub mod messages;
pub mod notepad;
pub mod projects;
pub mod session_summary_job_config;
pub mod sessions;
pub mod system_contexts;
pub mod task_manager;
pub mod terminals;
pub mod user_settings;

pub fn router() -> Router {
    let cfg = Config::get();

    let allowed_headers = [
        ACCEPT,
        AUTHORIZATION,
        CONTENT_TYPE,
        ORIGIN,
        HeaderName::from_static("x-requested-with"),
        HeaderName::from_static("x-api-key"),
        HeaderName::from_static("x-openai-key"),
        HeaderName::from_static("x-user-id"),
        HeaderName::from_static("x-project-id"),
        HeaderName::from_static("x-session-id"),
        HeaderName::from_static("x-request-id"),
    ];

    let cors = if cfg.cors_origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_headers(allowed_headers)
            .expose_headers([
                REQUEST_ID_HEADER.clone(),
                ACCESS_TOKEN_HEADER.clone(),
                ACCESS_TOKEN_EXPIRES_IN_HEADER.clone(),
            ])
            .allow_methods(Any)
            .allow_credentials(false)
    } else {
        let origins = cfg
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect::<Vec<_>>();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_headers(allowed_headers)
            .expose_headers([
                REQUEST_ID_HEADER.clone(),
                ACCESS_TOKEN_HEADER.clone(),
                ACCESS_TOKEN_EXPIRES_IN_HEADER.clone(),
            ])
            .allow_methods(Any)
            .allow_credentials(true)
    };

    let trace = TraceLayer::new_for_http()
        .make_span_with(|req: &Request<Body>| {
            let request_id = header_value(req, &REQUEST_ID_HEADER);
            let user_id = header_value(req, &HeaderName::from_static("x-user-id"));
            let project_id = header_value(req, &HeaderName::from_static("x-project-id"));
            let session_id = header_value(req, &HeaderName::from_static("x-session-id"));
            info_span!(
                "http.request",
                method = %req.method(),
                uri = %req.uri(),
                version = ?req.version(),
                request_id = %request_id,
                user_id = %user_id,
                project_id = %project_id,
                session_id = %session_id
            )
        })
        .on_request(|_req: &Request<Body>, _span: &tracing::Span| {
            info!("request.start");
        })
        .on_response(
            |res: &Response, latency: std::time::Duration, _span: &tracing::Span| {
                info!(status = %res.status(), latency_ms = %latency.as_millis(), "request.end");
            },
        )
        .on_failure(|err, latency: std::time::Duration, _span: &tracing::Span| {
            tracing::error!(error = %err, latency_ms = %latency.as_millis(), "request.failure");
        });

    let protected_api = Router::new()
        .merge(sessions::router())
        .merge(messages::router())
        .merge(chat_v2::router())
        .merge(chat_v3::router())
        .merge(agents_v3::router())
        .nest("/api/agents", agents::router())
        .nest("/api/applications", applications::router())
        .merge(projects::router())
        .merge(session_summary_job_config::router())
        .merge(task_manager::router())
        .merge(terminals::router())
        .merge(configs::router())
        .merge(system_contexts::router())
        .merge(fs::router())
        .merge(notepad::router())
        .nest("/api/v2", chat_agent_v2::router())
        .merge(user_settings::router())
        .route_layer(middleware::from_fn(require_auth));

    Router::new()
        .merge(auth::router())
        .merge(protected_api)
        .route("/health", axum::routing::get(health))
        .route("/", axum::routing::get(root))
        .fallback(fallback_404)
        .layer(cors)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(trace)
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(SetRequestIdLayer::new(
            REQUEST_ID_HEADER.clone(),
            MakeRequestUuid,
        ))
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "timestamp": crate::core::time::now_rfc3339(),
        "uptime": START_TIME.elapsed().as_secs_f64()
    }))
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "Chat App Node Server",
        "version": "1.0.0",
        "description": "Node.js 聊天应用服务器 - 完全复刻自 Python FastAPI 版本",
        "endpoints": {
            "health": "/health",
            "sessions": "/api/sessions",
            "messages": "/api/messages"
        }
    }))
}

async fn fallback_404(uri: OriginalUri) -> impl IntoResponse {
    let path = uri.0.path().to_string();
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "message": "请求的资源不存在",
                "path": path
            }
        })),
    )
}

fn header_value(req: &Request<Body>, name: &HeaderName) -> String {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

async fn require_auth(
    mut req: Request<Body>,
    next: middleware::Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let is_ws_upgrade = is_websocket_upgrade(&req);
    // 在中间件只解析一次 token，并把登录用户注入 request extensions。
    let auth_user = match auth_user_from_headers(req.headers()) {
        Ok(auth_user) => auth_user,
        // Browser WebSocket cannot set Authorization headers directly.
        // Allow terminal websocket auth via `?access_token=...` fallback.
        Err(AuthHeaderError::MissingAuthorization) => {
            auth_user_from_ws_query(&req).map_err(|err| err.into_response())?
        }
        Err(err) => return Err(err.into_response()),
    };
    req.extensions_mut().insert(auth_user.clone());
    let mut response = next.run(req).await;

    if !is_ws_upgrade {
        match sign_access_token(&auth_user.user_id, &auth_user.email) {
            Ok(token) => {
                if let Ok(header_value) = HeaderValue::from_str(&token) {
                    response
                        .headers_mut()
                        .insert(ACCESS_TOKEN_HEADER.clone(), header_value);
                }
                let ttl = crate::config::Config::get()
                    .auth_access_token_ttl_seconds
                    .max(60)
                    .to_string();
                if let Ok(header_value) = HeaderValue::from_str(&ttl) {
                    response
                        .headers_mut()
                        .insert(ACCESS_TOKEN_EXPIRES_IN_HEADER.clone(), header_value);
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to refresh access token");
            }
        }
    }

    Ok(response)
}

fn auth_user_from_ws_query(req: &Request<Body>) -> Result<AuthUser, AuthHeaderError> {
    if !is_websocket_upgrade(req) {
        return Err(AuthHeaderError::MissingAuthorization);
    }
    let query = req
        .uri()
        .query()
        .ok_or(AuthHeaderError::MissingAuthorization)?;
    let token = url::form_urlencoded::parse(query.as_bytes())
        .find_map(|(key, value)| {
            if key == "access_token" {
                Some(value.into_owned())
            } else {
                None
            }
        })
        .ok_or(AuthHeaderError::MissingAuthorization)?;
    auth_user_from_access_token(token.as_str())
}

fn is_websocket_upgrade(req: &Request<Body>) -> bool {
    req.headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}
