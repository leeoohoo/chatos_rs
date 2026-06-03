use axum::body::Body;
use axum::extract::{DefaultBodyLimit, OriginalUri};
use axum::http::{
    header::{HeaderName, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, UPGRADE},
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
    access_token_from_headers, resolve_auth_user_from_token, AuthHeaderError,
};
use crate::core::websocket_ticket::{consume_websocket_ticket, WebSocketTicketRecord};
use crate::modules;
use crate::services::access_token_scope;

static START_TIME: Lazy<Instant> = Lazy::new(Instant::now);
static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub mod agents;
pub mod agent_chat;
pub mod applications;
pub mod auth;
pub(crate) mod chat_stream_common;
pub mod code_nav;
pub mod configs;
pub mod contacts;
mod conversation_semantics;
pub mod fs;
pub mod git;
pub mod memory_compat;
pub mod memory_mappings;
pub mod messages;
pub mod notepad;
pub mod projects;
pub mod realtime;
pub mod remote_connections;
pub mod sessions;
pub mod system_contexts;
pub mod task_manager;
pub mod terminals;
pub mod ui_prompts;
pub mod user_settings;

pub fn router() -> Result<Router, String> {
    let cfg = Config::try_get()?;

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
        HeaderName::from_static("x-conversation-id"),
        HeaderName::from_static("x-request-id"),
        HeaderName::from_static("x-remote-verification-code"),
    ];

    let cors = if cfg.cors_origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_headers(allowed_headers)
            .expose_headers([REQUEST_ID_HEADER.clone()])
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
            .expose_headers([REQUEST_ID_HEADER.clone()])
            .allow_methods(Any)
            .allow_credentials(true)
    };

    let trace = TraceLayer::new_for_http()
        .make_span_with(|req: &Request<Body>| {
            let request_id = header_value(req, &REQUEST_ID_HEADER);
            let user_id = header_value(req, &HeaderName::from_static("x-user-id"));
            let project_id = header_value(req, &HeaderName::from_static("x-project-id"));
            let conversation_id = header_value(req, &HeaderName::from_static("x-conversation-id"));
            info_span!(
                "http.request",
                method = %req.method(),
                uri = %sanitize_request_uri(req.uri()),
                version = ?req.version(),
                request_id = %request_id,
                user_id = %user_id,
                project_id = %project_id,
                conversation_id = %conversation_id
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

    let protected_api =
        modules::app_api::protected_routes().route_layer(middleware::from_fn(require_auth));

    Ok(Router::new()
        .merge(modules::app_api::public_routes())
        .merge(protected_api)
        .route("/health", axum::routing::get(health))
        .route("/ready", axum::routing::get(ready))
        .route("/", axum::routing::get(root))
        .fallback(fallback_404)
        .layer(cors)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(trace)
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(SetRequestIdLayer::new(
            REQUEST_ID_HEADER.clone(),
            MakeRequestUuid,
        )))
}

fn build_health_payload() -> serde_json::Value {
    let snapshot = crate::core::runtime_health::snapshot_runtime_health();
    serde_json::json!({
        "status": snapshot.status,
        "ready": snapshot.ready,
        "timestamp": crate::core::time::now_rfc3339(),
        "uptime": START_TIME.elapsed().as_secs_f64(),
        "check_count": snapshot.check_count,
        "degraded_check_count": snapshot.degraded_check_count,
        "checks": snapshot.checks,
    })
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(build_health_payload())
}

async fn ready() -> (StatusCode, axum::Json<serde_json::Value>) {
    let payload = build_health_payload();
    let ready = payload
        .get("ready")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, axum::Json(payload))
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "Chatos RS Backend",
        "version": "1.0.0",
        "description": "Rust orchestration backend for Chatos RS engineering workflows",
        "endpoints": {
            "health": "/health",
            "ready": "/ready",
            "auth_login": "/api/auth/login",
            "sessions": "/api/sessions",
            "messages": "/api/messages",
            "chat_send": "/api/agent/chat/send",
            "realtime_ws": "/api/realtime/ws",
            "fs_list": "/api/fs/list",
            "git_status": "/api/git/status"
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

fn sanitize_request_uri(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    let Some(query) = uri.query() else {
        return path.to_string();
    };

    let sanitized_query = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| {
            let value = if matches!(
                key.as_ref(),
                "access_token"
                    | "token"
                    | "api_key"
                    | "authorization"
                    | "ws_ticket"
                    | "verification_code"
            ) {
                "[redacted]".to_string()
            } else {
                value.into_owned()
            };
            format!("{key}={value}")
        })
        .collect::<Vec<_>>()
        .join("&");

    if sanitized_query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{sanitized_query}")
    }
}

async fn require_auth(
    mut req: Request<Body>,
    next: middleware::Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // 在中间件只解析一次 token，并把登录用户注入 request extensions。
    let (access_token, auth_user) = match access_token_from_headers(req.headers()) {
        Ok(token) => {
            let auth_user = resolve_auth_user_from_token(token.as_str())
                .map_err(|err| err.into_response())?;
            (token, auth_user)
        }
        // Browser WebSocket cannot set Authorization headers directly.
        // Allow websocket auth via a short-lived `?ws_ticket=...` credential only.
        Err(AuthHeaderError::MissingAuthorization) => {
            match websocket_auth_from_query(&req).map_err(|err| err.into_response())? {
                WebSocketQueryAuth::Ticket(record) => (record.access_token, record.auth_user),
            }
        }
        Err(err) => return Err(err.into_response()),
    };

    req.extensions_mut().insert(auth_user);
    let response =
        access_token_scope::with_access_token_scope(Some(access_token), next.run(req)).await;
    Ok(response)
}

#[derive(Debug)]
enum WebSocketQueryAuth {
    Ticket(WebSocketTicketRecord),
}

fn websocket_auth_from_query(req: &Request<Body>) -> Result<WebSocketQueryAuth, AuthHeaderError> {
    if !is_websocket_upgrade(req) {
        return Err(AuthHeaderError::MissingAuthorization);
    }
    let query = req
        .uri()
        .query()
        .ok_or(AuthHeaderError::MissingAuthorization)?;
    let params = url::form_urlencoded::parse(query.as_bytes()).collect::<Vec<_>>();
    if let Some(ticket) = params
        .iter()
        .find_map(|(key, value)| (key == "ws_ticket").then(|| value.clone().into_owned()))
    {
        return consume_websocket_ticket(ticket.as_str()).map(WebSocketQueryAuth::Ticket);
    }
    Err(AuthHeaderError::MissingAuthorization)
}

fn is_websocket_upgrade(req: &Request<Body>) -> bool {
    req.headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        sanitize_request_uri, websocket_auth_from_query, WebSocketQueryAuth,
    };
    use crate::core::auth::{AuthHeaderError, AuthUser};
    use crate::core::websocket_ticket::issue_websocket_ticket;
    use axum::body::Body;
    use axum::http::{header::UPGRADE, Request, Uri};

    fn websocket_request(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header(UPGRADE, "websocket")
            .body(Body::empty())
            .expect("build websocket request")
    }

    fn auth_user() -> AuthUser {
        AuthUser {
            user_id: "user_1".to_string(),
            role: "user".to_string(),
        }
    }

    #[test]
    fn sanitize_request_uri_redacts_sensitive_query_values() {
        let uri: Uri = "/api/realtime/ws?ws_ticket=ticket_1&access_token=token_1&verification_code=123456&plain=value"
            .parse()
            .expect("parse uri");
        assert_eq!(
            sanitize_request_uri(&uri),
            "/api/realtime/ws?ws_ticket=[redacted]&access_token=[redacted]&verification_code=[redacted]&plain=value"
        );
    }

    #[test]
    fn websocket_auth_from_query_accepts_ws_ticket() {
        let ticket =
            issue_websocket_ticket("access_token_1", &auth_user()).expect("issue websocket ticket");
        let request = websocket_request(
            format!("/api/realtime/ws?ws_ticket={}", ticket.ticket).as_str(),
        );

        let result = websocket_auth_from_query(&request).expect("resolve websocket auth");
        match result {
            WebSocketQueryAuth::Ticket(record) => {
                assert_eq!(record.access_token, "access_token_1");
                assert_eq!(record.auth_user.user_id, "user_1");
            }
        }
    }

    #[test]
    fn websocket_auth_from_query_rejects_legacy_access_token_param() {
        let request = websocket_request("/api/realtime/ws?access_token=legacy_token");
        let error = websocket_auth_from_query(&request).expect_err("legacy query token rejected");
        assert_eq!(error, AuthHeaderError::MissingAuthorization);
    }
}
