use axum::body::Body;
use axum::extract::{DefaultBodyLimit, OriginalUri};
use axum::http::{
    header::{HeaderName, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN},
    Request, StatusCode,
};
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

static START_TIME: Lazy<Instant> = Lazy::new(Instant::now);
static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub mod agents;
pub mod agents_v3;
pub mod applications;
pub mod chat_agent_v2;
pub mod chat_v2;
pub mod chat_v3;
pub mod configs;
pub mod fs;
pub mod messages;
pub mod projects;
pub mod sessions;
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

    Router::new()
        .merge(sessions::router())
        .merge(messages::router())
        .merge(chat_v2::router())
        .merge(chat_v3::router())
        .merge(agents_v3::router())
        .nest("/api/agents", agents::router())
        .nest("/api/applications", applications::router())
        .merge(projects::router())
        .merge(terminals::router())
        .merge(configs::router())
        .merge(fs::router())
        .nest("/api/v2", chat_agent_v2::router())
        .merge(user_settings::router())
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
        "timestamp": chrono::Utc::now().to_rfc3339(),
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
