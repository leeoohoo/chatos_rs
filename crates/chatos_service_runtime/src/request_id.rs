// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

pub const REQUEST_ID_HEADER: &str = "x-request-id";
const MAX_REQUEST_ID_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(String);

impl RequestId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

pub fn request_id_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| valid_request_id(value))
}

pub fn resolve_request_id(headers: &HeaderMap) -> RequestId {
    request_id_from_headers(headers)
        .map(ToOwned::to_owned)
        .map(RequestId::new)
        .unwrap_or_else(|| RequestId::new(uuid::Uuid::new_v4().to_string()))
}

pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = resolve_request_id(request.headers());
    request.extensions_mut().insert(request_id.clone());
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(request_id.as_str()) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_BYTES
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::HeaderMap;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::{
        request_id_from_headers, request_id_middleware, resolve_request_id, REQUEST_ID_HEADER,
    };

    #[test]
    fn preserves_valid_incoming_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, "request-123".parse().unwrap());

        assert_eq!(request_id_from_headers(&headers), Some("request-123"));
        assert_eq!(resolve_request_id(&headers).as_str(), "request-123");
    }

    #[test]
    fn rejects_whitespace_and_oversized_request_ids() {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, "request id".parse().unwrap());
        assert_eq!(request_id_from_headers(&headers), None);

        headers.insert(REQUEST_ID_HEADER, "x".repeat(129).parse().unwrap());
        assert_eq!(request_id_from_headers(&headers), None);
        assert!(!resolve_request_id(&headers).as_str().is_empty());
    }

    #[tokio::test]
    async fn middleware_preserves_request_id_on_response() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(request_id_middleware));
        let request = axum::http::Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, "request-456")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(
            response
                .headers()
                .get(REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("request-456")
        );
    }
}
