// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::extract::MatchedPath;
use axum::http::{header, Method, Request};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use once_cell::sync::Lazy;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";
const DURATION_BUCKETS_SECONDS: [f64; 12] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HttpSurface {
    Public,
    Internal,
}

impl HttpSurface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WebSocketKind {
    Realtime,
    Terminal,
    RemoteTerminal,
}

impl WebSocketKind {
    fn counter(self) -> &'static AtomicU64 {
        match self {
            Self::Realtime => &METRICS.realtime_websockets,
            Self::Terminal => &METRICS.terminal_websockets,
            Self::RemoteTerminal => &METRICS.remote_terminal_websockets,
        }
    }
}

pub(crate) struct ActiveWebSocketConnection {
    kind: WebSocketKind,
}

impl ActiveWebSocketConnection {
    pub(crate) fn start(kind: WebSocketKind) -> Self {
        kind.counter().fetch_add(1, Ordering::Relaxed);
        Self { kind }
    }
}

impl Drop for ActiveWebSocketConnection {
    fn drop(&mut self) {
        self.kind.counter().fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HttpMetricKey {
    surface: HttpSurface,
    method: &'static str,
    route: String,
    status_class: &'static str,
}

#[derive(Debug, Clone)]
struct HttpMetricValue {
    request_count: u64,
    duration_sum_seconds: f64,
    duration_bucket_counts: [u64; DURATION_BUCKETS_SECONDS.len()],
}

impl Default for HttpMetricValue {
    fn default() -> Self {
        Self {
            request_count: 0,
            duration_sum_seconds: 0.0,
            duration_bucket_counts: [0; DURATION_BUCKETS_SECONDS.len()],
        }
    }
}

#[derive(Default)]
struct MetricsRegistry {
    http: DashMap<HttpMetricKey, HttpMetricValue>,
    public_active_requests: AtomicU64,
    internal_active_requests: AtomicU64,
    realtime_websockets: AtomicU64,
    terminal_websockets: AtomicU64,
    remote_terminal_websockets: AtomicU64,
}

static METRICS: Lazy<MetricsRegistry> = Lazy::new(MetricsRegistry::default);

struct ActiveHttpRequest {
    counter: &'static AtomicU64,
}

impl ActiveHttpRequest {
    fn start(surface: HttpSurface) -> Self {
        let counter = match surface {
            HttpSurface::Public => &METRICS.public_active_requests,
            HttpSurface::Internal => &METRICS.internal_active_requests,
        };
        counter.fetch_add(1, Ordering::Relaxed);
        Self { counter }
    }
}

impl Drop for ActiveHttpRequest {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

pub(crate) async fn observe_public_http(
    request: Request<axum::body::Body>,
    next: middleware::Next,
) -> Response {
    observe_http(HttpSurface::Public, request, next).await
}

pub(crate) async fn observe_internal_http(
    request: Request<axum::body::Body>,
    next: middleware::Next,
) -> Response {
    observe_http(HttpSurface::Internal, request, next).await
}

async fn observe_http(
    surface: HttpSurface,
    request: Request<axum::body::Body>,
    next: middleware::Next,
) -> Response {
    let route = route_label(&request);
    if surface == HttpSurface::Public && route == "/metrics" {
        return next.run(request).await;
    }

    let method = normalize_method(request.method());
    let active_request = ActiveHttpRequest::start(surface);
    let started_at = Instant::now();
    let response = next.run(request).await;
    let elapsed = started_at.elapsed().as_secs_f64();
    drop(active_request);

    record_http_request(
        HttpMetricKey {
            surface,
            method,
            route,
            status_class: status_class(response.status().as_u16()),
        },
        elapsed,
    );
    response
}

fn record_http_request(key: HttpMetricKey, duration_seconds: f64) {
    let mut value = METRICS.http.entry(key).or_default();
    value.request_count = value.request_count.saturating_add(1);
    value.duration_sum_seconds += duration_seconds;
    for (index, upper_bound) in DURATION_BUCKETS_SECONDS.iter().enumerate() {
        if duration_seconds <= *upper_bound {
            value.duration_bucket_counts[index] =
                value.duration_bucket_counts[index].saturating_add(1);
        }
    }
}

fn route_label(request: &Request<axum::body::Body>) -> String {
    request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_string())
        .unwrap_or_else(|| "/unmatched".to_string())
}

fn normalize_method(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::CONNECT => "CONNECT",
        Method::TRACE => "TRACE",
        _ => "OTHER",
    }
}

fn status_class(status: u16) -> &'static str {
    match status {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

pub(crate) async fn prometheus_metrics() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)],
        render_prometheus_metrics(),
    )
}

fn render_prometheus_metrics() -> String {
    let mut body = String::new();
    body.push_str(
        "# HELP chatos_http_requests_total HTTP requests completed by the ChatOS backend.\n\
# TYPE chatos_http_requests_total counter\n",
    );

    let mut http_metrics = METRICS
        .http
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect::<Vec<_>>();
    http_metrics.sort_by(|(left, _), (right, _)| {
        (
            left.surface.as_str(),
            left.route.as_str(),
            left.method,
            left.status_class,
        )
            .cmp(&(
                right.surface.as_str(),
                right.route.as_str(),
                right.method,
                right.status_class,
            ))
    });

    for (key, value) in &http_metrics {
        let labels = http_labels(key);
        let _ = writeln!(
            body,
            "chatos_http_requests_total{{{labels}}} {}",
            value.request_count
        );
    }

    body.push_str(
        "# HELP chatos_http_request_duration_seconds HTTP request duration in seconds.\n\
# TYPE chatos_http_request_duration_seconds histogram\n",
    );
    for (key, value) in &http_metrics {
        let labels = http_labels(key);
        for (index, upper_bound) in DURATION_BUCKETS_SECONDS.iter().enumerate() {
            let _ = writeln!(
                body,
                "chatos_http_request_duration_seconds_bucket{{{labels},le=\"{upper_bound}\"}} {}",
                value.duration_bucket_counts[index]
            );
        }
        let _ = writeln!(
            body,
            "chatos_http_request_duration_seconds_bucket{{{labels},le=\"+Inf\"}} {}",
            value.request_count
        );
        let _ = writeln!(
            body,
            "chatos_http_request_duration_seconds_sum{{{labels}}} {}",
            value.duration_sum_seconds
        );
        let _ = writeln!(
            body,
            "chatos_http_request_duration_seconds_count{{{labels}}} {}",
            value.request_count
        );
    }

    body.push_str(
        "# HELP chatos_http_requests_active HTTP requests currently being processed.\n\
# TYPE chatos_http_requests_active gauge\n",
    );
    append_surface_gauge(
        &mut body,
        "public",
        METRICS.public_active_requests.load(Ordering::Relaxed),
    );
    append_surface_gauge(
        &mut body,
        "internal",
        METRICS.internal_active_requests.load(Ordering::Relaxed),
    );

    body.push_str(
        "# HELP chatos_websocket_connections_active Active ChatOS WebSocket connections.\n\
# TYPE chatos_websocket_connections_active gauge\n",
    );
    append_kind_gauge(
        &mut body,
        "realtime",
        METRICS.realtime_websockets.load(Ordering::Relaxed),
    );
    append_kind_gauge(
        &mut body,
        "terminal",
        METRICS.terminal_websockets.load(Ordering::Relaxed),
    );
    append_kind_gauge(
        &mut body,
        "remote_terminal",
        METRICS.remote_terminal_websockets.load(Ordering::Relaxed),
    );

    body.push_str(
        "# HELP chatos_terminal_sessions_active Active local terminal processes managed by ChatOS.\n\
# TYPE chatos_terminal_sessions_active gauge\n",
    );
    let _ = writeln!(
        body,
        "chatos_terminal_sessions_active{{service=\"chatos-backend\"}} {}",
        crate::services::terminal_manager::get_terminal_manager().active_session_count()
    );

    body.push_str(
        "# HELP chatos_conversation_turns_active Active conversation turns managed by ChatOS.\n\
# TYPE chatos_conversation_turns_active gauge\n",
    );
    let _ = writeln!(
        body,
        "chatos_conversation_turns_active{{service=\"chatos-backend\"}} {}",
        crate::services::runtime_guidance_manager::runtime_guidance_manager().active_turn_count()
    );
    body
}

fn http_labels(key: &HttpMetricKey) -> String {
    format!(
        "service=\"chatos-backend\",surface=\"{}\",method=\"{}\",route=\"{}\",status_class=\"{}\"",
        key.surface.as_str(),
        key.method,
        escape_prometheus_label(key.route.as_str()),
        key.status_class
    )
}

fn append_surface_gauge(body: &mut String, surface: &str, value: u64) {
    let _ = writeln!(
        body,
        "chatos_http_requests_active{{service=\"chatos-backend\",surface=\"{surface}\"}} {value}"
    );
}

fn append_kind_gauge(body: &mut String, kind: &str, value: u64) {
    let _ = writeln!(
        body,
        "chatos_websocket_connections_active{{service=\"chatos-backend\",kind=\"{kind}\"}} {value}"
    );
}

fn escape_prometheus_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_method, observe_internal_http, observe_public_http, prometheus_metrics,
        status_class,
    };
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn method_and_status_labels_are_bounded() {
        assert_eq!(normalize_method(&Method::GET), "GET");
        assert_eq!(
            normalize_method(&Method::from_bytes(b"CUSTOM").unwrap()),
            "OTHER"
        );
        assert_eq!(status_class(101), "1xx");
        assert_eq!(status_class(204), "2xx");
        assert_eq!(status_class(404), "4xx");
        assert_eq!(status_class(503), "5xx");
        assert_eq!(status_class(700), "other");
    }

    #[tokio::test]
    async fn metrics_use_matched_route_patterns_instead_of_resource_ids() {
        let app = Router::new()
            .route(
                "/widgets/{widget_id}",
                get(|| async { StatusCode::NO_CONTENT }),
            )
            .route("/metrics", get(prometheus_metrics))
            .layer(middleware::from_fn(observe_public_http));

        let response = app
            .clone()
            .oneshot(
                Request::get("/widgets/widget-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("route=\"/widgets/{widget_id}\""));
        assert!(!body.contains("widget-123"));
        assert!(body.contains("status_class=\"2xx\""));
        assert!(body.contains("chatos_http_request_duration_seconds_bucket{"));
    }

    #[tokio::test]
    async fn internal_requests_are_classified_separately() {
        let app = Router::new()
            .route("/internal/callback", get(|| async { StatusCode::ACCEPTED }))
            .layer(middleware::from_fn(observe_internal_http));
        let response = app
            .oneshot(
                Request::get("/internal/callback")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let body = super::render_prometheus_metrics();
        assert!(body.contains("surface=\"internal\""));
        assert!(body.contains("route=\"/internal/callback\""));
    }
}
