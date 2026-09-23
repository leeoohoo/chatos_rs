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
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use once_cell::sync::Lazy;
use serde_json::json;
use sha2::{Digest, Sha512};
use std::time::Instant;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{debug, info_span};

use crate::config::Config;
use crate::core::auth::{
    access_token_from_headers, resolve_device_bound_auth_user_and_scopes_via_user_service,
    AuthHeaderError,
};
use crate::core::websocket_ticket::{consume_websocket_ticket, WebSocketTicketRecord};
use crate::modules;
use crate::services::access_token_scope;

static START_TIME: Lazy<Instant> = Lazy::new(Instant::now);
static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Default)]
pub(crate) struct RequestClientScopes(Vec<String>);

impl RequestClientScopes {
    pub(crate) fn is_wechat_companion(&self) -> bool {
        self.0.iter().any(|scope| scope == "wechat_companion")
    }

    pub(crate) fn as_slice(&self) -> &[String] {
        self.0.as_slice()
    }
}

#[derive(Clone)]
pub(crate) struct RequestAccessToken(String);

impl RequestAccessToken {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

pub mod agent_artifacts;
pub mod agent_chat;
pub mod agents;
pub mod applications;
pub mod ask_user_prompts;
pub mod attachments;
pub mod auth;
pub(crate) mod chat_stream_common;
pub mod code_nav;
pub mod companion;
pub mod configs;
pub mod contacts;
mod conversation_semantics;
mod cors;
pub mod fs;
// Filesystem policy remains an internal runtime primitive. Git and filesystem
// project operations are owned by the desktop client and Local Connector.
mod internal_audit;
pub mod local_connectors;
pub mod mcp_management;
pub mod memory_compat;
pub mod memory_mappings;
pub mod message_task_runner;
pub mod messages;
pub(crate) mod metrics;
pub mod notepad;
pub mod pet_activities;
pub mod realtime;
pub mod remote_connections;
pub mod sessions;
pub mod system_contexts;
pub mod task_manager;
pub mod task_runner_plugins;
pub mod terminals;
mod trace_context;
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
            info_span!(
                "http.request",
                otel.kind = "server",
                otel.name = %format!("{} {}", req.method(), matched_route(req)),
                http.request.method = %req.method(),
                http.route = %matched_route(req),
                server.address = %header_value(req, &HOST),
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
        .route("/metrics", axum::routing::get(metrics::prometheus_metrics))
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
        .layer(middleware::from_fn(metrics::observe_public_http))
        .layer(middleware::from_fn(trace_context::accept_remote_parent))
        .layer(trace)
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(SetRequestIdLayer::new(
            REQUEST_ID_HEADER.clone(),
            MakeRequestUuid,
        )))
}

pub fn internal_router() -> Router {
    let trace = TraceLayer::new_for_http().make_span_with(|req: &Request<Body>| {
        info_span!(
            "http.request",
            otel.kind = "server",
            otel.name = %format!("{} {}", req.method(), matched_route(req)),
            http.request.method = %req.method(),
            http.route = %matched_route(req),
            server.address = %header_value(req, &HOST),
            surface = "internal"
        )
    });
    Router::new()
        .merge(modules::app_api::internal_routes())
        .fallback(fallback_404)
        .layer(DefaultBodyLimit::max(default_request_body_limit_bytes()))
        .layer(middleware::from_fn(log_server_error_requests))
        .layer(middleware::from_fn(metrics::observe_internal_http))
        .layer(middleware::from_fn(trace_context::accept_remote_parent))
        .layer(trace)
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
    50 * 1024 * 1024
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

fn matched_route(req: &Request<Body>) -> &str {
    req.extensions()
        .get::<axum::extract::MatchedPath>()
        .map(axum::extract::MatchedPath::as_str)
        .unwrap_or("/unmatched")
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
    let (access_token, auth_user, scopes) = match access_token_from_headers(req.headers()) {
        Ok(token) => {
            let proof = device_proof_request(&req);
            if !proof.body_sha512.is_empty() {
                verify_and_restore_device_bound_body(&mut req, proof.body_sha512.as_str()).await?;
            }
            let (auth_user, scopes) =
                resolve_device_bound_auth_user_and_scopes_via_user_service(token.as_str(), &proof)
                    .await
                    .map_err(|err| err.into_response())?;
            (token, auth_user, scopes)
        }
        // Browser WebSocket cannot set Authorization headers directly.
        // Allow websocket auth via a short-lived `?ws_ticket=...` credential only.
        Err(AuthHeaderError::MissingAuthorization) => {
            match websocket_auth_from_query(&req).map_err(|err| err.into_response())? {
                WebSocketQueryAuth::Ticket(record) => {
                    (record.access_token, record.auth_user, record.scopes)
                }
            }
        }
        Err(err) => return Err(err.into_response()),
    };

    enforce_client_scope(req.method(), req.uri().path(), scopes.as_slice())?;

    // The Companion device proof is verified above against the original
    // ChatOS method, target, and body. It must not be replayed as if it were a
    // proof for Memory Engine. After the route has passed scope enforcement,
    // use ChatOS' signed internal identity for its Memory Engine data access;
    // individual handlers still enforce conversation ownership with auth_user.
    let use_internal_memory_service_auth = scopes.iter().any(|scope| scope == "wechat_companion");

    req.extensions_mut().insert(auth_user);
    req.extensions_mut().insert(RequestClientScopes(scopes));
    req.extensions_mut()
        .insert(RequestAccessToken(access_token.clone()));
    let response = access_token_scope::with_verified_request_scope(
        Some(access_token),
        use_internal_memory_service_auth,
        next.run(req),
    )
    .await;
    Ok(response)
}

fn device_proof_request(
    request: &Request<Body>,
) -> crate::services::user_service_api_client::DeviceProofVerificationRequest {
    let header = |name: &'static str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let internal_path = request.uri().path();
    let logical_path = internal_path.strip_prefix("/api").unwrap_or(internal_path);
    let mut target = format!("/api/chatos{logical_path}");
    if let Some(query) = request.uri().query().filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }
    crate::services::user_service_api_client::DeviceProofVerificationRequest {
        surface: "chatos".to_string(),
        method: request.method().as_str().to_string(),
        target,
        body_sha512: header("x-chatos-device-body-sha512"),
        client_session_id: header("x-chatos-device-session-id"),
        device_id: header("x-chatos-device-id"),
        timestamp: header("x-chatos-device-timestamp")
            .parse()
            .unwrap_or_default(),
        nonce: header("x-chatos-device-nonce"),
        signature_algorithm: header("x-chatos-device-signature-alg"),
        signature: header("x-chatos-device-signature"),
    }
}

async fn verify_and_restore_device_bound_body(
    request: &mut Request<Body>,
    expected_hash: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    const LIMIT: usize = 4 * 1024 * 1024;
    let body = std::mem::replace(request.body_mut(), Body::empty());
    let bytes = axum::body::to_bytes(body, LIMIT).await.map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "device-bound request body is too large" })),
        )
    })?;
    let actual = URL_SAFE_NO_PAD.encode(Sha512::digest(bytes.as_ref()));
    *request.body_mut() = Body::from(bytes);
    if actual != expected_hash {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "device proof body digest does not match the request" })),
        ));
    }
    Ok(())
}

fn enforce_client_scope(
    method: &Method,
    path: &str,
    scopes: &[String],
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !scopes.iter().any(|scope| scope == "wechat_companion") {
        return Ok(());
    }
    let allowed = (method == Method::GET
        && (path == "/api/auth/me"
            || path == "/api/realtime/ws"
            || companion_conversation_read_path(path)
            || companion_task_read_path(path)
            || path == "/api/ask-user-prompts"))
        || (method == Method::POST
            && matches!(
                path,
                "/api/auth/ws-ticket"
                    | "/api/agent/chat/send"
                    | "/api/agent/chat/guidance"
                    | "/api/agent/chat/stop"
            ))
        || (method == Method::POST
            && path.starts_with("/api/ask-user-prompts/")
            && (path.ends_with("/submit") || path.ends_with("/cancel")));
    if allowed {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "WeChat Companion session is not allowed to access this ChatOS endpoint"
            })),
        ))
    }
}

fn companion_conversation_read_path(path: &str) -> bool {
    let Some(suffix) = path.strip_prefix("/api/companion/conversations/") else {
        return false;
    };
    let segments = suffix.split('/').collect::<Vec<_>>();
    !segments[0].is_empty()
        && (segments.len() == 1
            || (segments.len() == 2 && matches!(segments[1], "compact-history" | "state")))
}

fn companion_task_read_path(path: &str) -> bool {
    let Some(suffix) = path.strip_prefix("/api/companion/messages/") else {
        return false;
    };
    let segments = suffix.split('/').collect::<Vec<_>>();
    matches!(segments.as_slice(), [message_id, "tasks"] if !message_id.is_empty())
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
include!("mod_inline_tests.rs");
