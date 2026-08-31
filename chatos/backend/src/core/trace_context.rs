// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use opentelemetry::global;
use opentelemetry::propagation::Injector;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
            return;
        };
        let Ok(value) = HeaderValue::from_str(value.as_str()) else {
            return;
        };
        self.0.insert(name, value);
    }
}

pub fn inject_current_trace_context(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let context = tracing::Span::current().context();
    let mut headers = HeaderMap::new();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut HeaderInjector(&mut headers));
    });
    request.headers(headers)
}
