// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, OriginalUri};
use axum::http::{
    header::{HeaderName, HOST, UPGRADE},
    Method, Request, StatusCode,
};
use axum::middleware;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde_json::json;
use std::time::Instant;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{debug, debug_span};

use crate::config::Config;
use crate::core::auth::{
    access_token_from_headers, resolve_auth_user_via_user_service, AuthHeaderError,
};
use crate::core::websocket_ticket::{consume_websocket_ticket, WebSocketTicketRecord};
use crate::modules;
use crate::services::access_token_scope;

static START_TIME: Lazy<Instant> = Lazy::new(Instant::now);
static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub mod agent_chat;
pub mod agents;
pub mod applications;
pub mod ask_user_prompts;
pub mod attachments;
pub mod auth;
pub(crate) mod chat_stream_common;
pub mod code_nav;
pub mod configs;
pub mod contacts;
mod conversation_semantics;
mod cors;
pub mod fs;
pub mod git;
pub mod local_connectors;
pub mod mcp_management;
pub mod memory_compat;
pub mod memory_mappings;
pub mod message_task_runner;
pub mod messages;
pub mod notepad;
pub mod projects;
pub mod realtime;
pub mod remote_connections;
pub mod sessions;
pub mod system_contexts;
pub mod task_manager;
pub mod task_runner_plugins;
pub mod terminals;
pub mod user_settings;

pub fn public_router() -> Result<Router, String> {
    let cfg = Config::try_get()?;
    let request_body_limit = default_request_body_limit_bytes();

    let cors = cors::layer(&cfg.cors_origins, REQUEST_ID_HEADER.clone());

    let trace = TraceLayer::new_for_http()
        .make_span_with(|req: &Request<Body>| {
            let request_id = header_value(req, &REQUEST_ID_HEADER);
            let user_id = header_value(req, &HeaderName::from_static("x-user-id"));
            let project_id = header_value(req, &HeaderName::from_static("x-project-id"));
            let conversation_id = header_value(req, &HeaderName::from_static("x-conversation-id"));
            debug_span!(
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
            debug!("request.start");
        })
        .on_response(
            |res: &Response, latency: std::time::Duration, _span: &tracing::Span| {
                debug!(status = %res.status(), latency_ms = %latency.as_millis(), "request.end");
            },
        )
        .on_failure(|err, latency: std::time::Duration, span: &tracing::Span| {
            span.in_scope(|| {
                debug!(
                    error = %err,
                    latency_ms = %latency.as_millis(),
                    "request.failure"
                );
            });
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
        .layer(DefaultBodyLimit::max(request_body_limit))
        .layer(middleware::from_fn(
            enforce_plugin_ui_resource_origin_namespace,
        ))
        .layer(middleware::from_fn(log_server_error_requests))
        .layer(trace)
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(SetRequestIdLayer::new(
            REQUEST_ID_HEADER.clone(),
            MakeRequestUuid,
        )))
}

pub fn internal_router() -> Router {
    Router::new()
        .merge(modules::app_api::internal_routes())
        .fallback(fallback_404)
        .layer(DefaultBodyLimit::max(default_request_body_limit_bytes()))
        .layer(middleware::from_fn(log_server_error_requests))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(SetRequestIdLayer::new(
            REQUEST_ID_HEADER.clone(),
            MakeRequestUuid,
        ))
}

async fn log_server_error_requests(request: Request<Body>, next: middleware::Next) -> Response {
    let method = request.method().clone();
    let uri = sanitize_request_uri(request.uri());
    let started_at = Instant::now();
    let response = next.run(request).await;
    if response.status().is_server_error() {
        tracing::error!(
            method = %method,
            uri = %uri,
            status = %response.status(),
            latency_ms = %started_at.elapsed().as_millis(),
            "request.server_error"
        );
    }
    response
}

async fn enforce_plugin_ui_resource_origin_namespace(
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    let Ok(config) = Config::try_get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let request_host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok());
    let resource_request = config
        .plugin_ui_resource_origin
        .as_deref()
        .is_some_and(|origin| {
            request_host.is_some_and(|host| request_host_matches_origin(host, origin))
                && request
                    .uri()
                    .path()
                    .starts_with("/api/plugin-ui/workbench/")
        });
    let allowed = plugin_ui_resource_namespace_allowed(
        config.plugin_ui_resource_origin.as_deref(),
        request.method(),
        request.uri().path(),
        request_host,
    );
    if !allowed {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut response = next.run(request).await;
    if resource_request {
        remove_plugin_ui_resource_cors_headers(response.headers_mut());
    }
    response
}

fn remove_plugin_ui_resource_cors_headers(headers: &mut axum::http::HeaderMap) {
    for header in [
        "access-control-allow-origin",
        "access-control-allow-credentials",
        "access-control-expose-headers",
        "access-control-allow-headers",
        "access-control-allow-methods",
        "access-control-max-age",
    ] {
        headers.remove(header);
    }
}

fn plugin_ui_resource_namespace_allowed(
    resource_origin: Option<&str>,
    method: &Method,
    path: &str,
    request_host: Option<&str>,
) -> bool {
    let Some(resource_origin) = resource_origin else {
        return true;
    };
    let resource_host =
        request_host.is_some_and(|host| request_host_matches_origin(host, resource_origin));
    let resource_path = path.starts_with("/api/plugin-ui/workbench/");
    if resource_host {
        resource_path && (method == Method::GET || method == Method::HEAD)
    } else {
        !resource_path
    }
}

fn request_host_matches_origin(request_host: &str, origin: &str) -> bool {
    let Ok(origin) = url::Url::parse(origin) else {
        return false;
    };
    let Ok(authority) = request_host.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    if !authority
        .host()
        .eq_ignore_ascii_case(origin.host_str().unwrap_or_default())
    {
        return false;
    }
    match origin.port() {
        Some(expected) => authority.port_u16() == Some(expected),
        None => {
            authority.port_u16().is_none() || authority.port_u16() == origin.port_or_known_default()
        }
    }
}

fn default_request_body_limit_bytes() -> usize {
    const BASE_LIMIT: usize = 50 * 1024 * 1024;
    let cloud_zip_limit = std::env::var("PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(100 * 1024 * 1024)
        .saturating_add(1024 * 1024);
    BASE_LIMIT.max(cloud_zip_limit)
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
        "name": "Chat OS Backend",
        "version": "1.0.0",
        "description": "Rust orchestration backend for Chat OS engineering workflows",
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
    let path = sanitize_sensitive_path(uri.path());
    let Some(query) = uri.query() else {
        return path;
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

fn sanitize_sensitive_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.len() == 68
                && segment.starts_with("pui_")
                && segment[4..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            {
                "[redacted]"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

async fn require_auth(
    mut req: Request<Body>,
    next: middleware::Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // 在中间件只解析一次 token，并把登录用户注入 request extensions。
    let (access_token, auth_user) = match access_token_from_headers(req.headers()) {
        Ok(token) => {
            let auth_user = resolve_auth_user_via_user_service(token.as_str())
                .await
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
        plugin_ui_resource_namespace_allowed, remove_plugin_ui_resource_cors_headers,
        sanitize_request_uri, websocket_auth_from_query, WebSocketQueryAuth,
    };
    use crate::core::auth::{AuthHeaderError, AuthUser};
    use crate::core::websocket_ticket::issue_websocket_ticket;
    use axum::body::Body;
    use axum::http::{header::UPGRADE, HeaderMap, HeaderValue, Method, Request, Uri};

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
    fn sanitize_request_uri_redacts_plugin_ui_workbench_session_paths() {
        let session_id = format!("pui_{}", "a".repeat(64));
        let uri: Uri = format!("/api/plugin-ui/workbench/{session_id}/ui/index.html?plain=value")
            .parse()
            .expect("parse uri");
        assert_eq!(
            sanitize_request_uri(&uri),
            "/api/plugin-ui/workbench/[redacted]/ui/index.html?plain=value"
        );
    }

    #[test]
    fn plugin_ui_resource_origin_is_an_exact_get_only_namespace() {
        let origin = Some("https://plugin-ui.example.com");
        assert!(plugin_ui_resource_namespace_allowed(
            origin,
            &Method::GET,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("plugin-ui.example.com"),
        ));
        assert!(plugin_ui_resource_namespace_allowed(
            origin,
            &Method::HEAD,
            "/api/plugin-ui/workbench/pui_session/ui/app.js",
            Some("plugin-ui.example.com:443"),
        ));
        assert!(!plugin_ui_resource_namespace_allowed(
            origin,
            &Method::POST,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("plugin-ui.example.com"),
        ));
        assert!(!plugin_ui_resource_namespace_allowed(
            origin,
            &Method::GET,
            "/api/sessions",
            Some("plugin-ui.example.com"),
        ));
        assert!(!plugin_ui_resource_namespace_allowed(
            origin,
            &Method::GET,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("app.example.com"),
        ));
        assert!(plugin_ui_resource_namespace_allowed(
            None,
            &Method::GET,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("app.example.com"),
        ));

        let mut headers = HeaderMap::new();
        headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        headers.insert(
            "access-control-allow-credentials",
            HeaderValue::from_static("true"),
        );
        headers.insert("content-type", HeaderValue::from_static("text/html"));
        remove_plugin_ui_resource_cors_headers(&mut headers);
        assert!(!headers.contains_key("access-control-allow-origin"));
        assert!(!headers.contains_key("access-control-allow-credentials"));
        assert_eq!(headers["content-type"], "text/html");
    }

    #[test]
    fn websocket_auth_from_query_accepts_ws_ticket() {
        let ticket =
            issue_websocket_ticket("access_token_1", &auth_user()).expect("issue websocket ticket");
        let request =
            websocket_request(format!("/api/realtime/ws?ws_ticket={}", ticket.ticket).as_str());

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
