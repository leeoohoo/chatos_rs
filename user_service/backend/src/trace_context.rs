// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::http::{HeaderMap, Request};
use axum::middleware;
use axum::response::Response;
use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use reqwest::header::{HeaderMap as ReqwestHeaderMap, HeaderName, HeaderValue};
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|name| name.as_str()).collect()
    }
}

struct HeaderInjector<'a>(&'a mut ReqwestHeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let name = HeaderName::from_bytes(key.as_bytes())
            .expect("OpenTelemetry propagator produced an invalid header name");
        let value = HeaderValue::from_str(value.as_str())
            .expect("OpenTelemetry propagator produced an invalid header value");
        self.0.insert(name, value);
    }
}

pub(crate) trait InternalTraceContextExt {
    fn with_internal_trace_context(self) -> Self;
}

impl InternalTraceContextExt for reqwest::RequestBuilder {
    fn with_internal_trace_context(self) -> Self {
        let context = tracing::Span::current().context();
        let mut headers = ReqwestHeaderMap::new();
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&context, &mut HeaderInjector(&mut headers));
        });
        self.headers(headers)
    }
}

pub(crate) async fn accept_remote_parent(
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    let parent = global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(request.headers()))
    });
    let _ = tracing::Span::current().set_parent(parent);
    next.run(request).await
}
