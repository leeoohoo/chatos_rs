// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::error::Error as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpRequestErrorKind {
    Timeout,
    Connect,
    Decode,
    Body,
    Builder,
    Redirect,
    Status,
    Request,
    Other,
}

impl HttpRequestErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Decode => "decode",
            Self::Body => "body",
            Self::Builder => "builder",
            Self::Redirect => "redirect",
            Self::Status => "status",
            Self::Request => "request",
            Self::Other => "other",
        }
    }
}

pub fn classify_http_request_error(error: &reqwest::Error) -> HttpRequestErrorKind {
    if error.is_timeout() {
        HttpRequestErrorKind::Timeout
    } else if error.is_connect() {
        HttpRequestErrorKind::Connect
    } else if error.is_decode() {
        HttpRequestErrorKind::Decode
    } else if error.is_body() {
        HttpRequestErrorKind::Body
    } else if error.is_builder() {
        HttpRequestErrorKind::Builder
    } else if error.is_redirect() {
        HttpRequestErrorKind::Redirect
    } else if error.is_status() {
        HttpRequestErrorKind::Status
    } else if error.is_request() {
        HttpRequestErrorKind::Request
    } else {
        HttpRequestErrorKind::Other
    }
}

pub fn format_http_request_error(context: &str, error: reqwest::Error) -> String {
    let mut message = format!(
        "{context} failed (kind={}): {error}",
        classify_http_request_error(&error).as_str()
    );
    let mut source = error.source();
    let mut source_count = 0usize;
    while let Some(cause) = source {
        let detail = cause.to_string();
        if !detail.is_empty() && !message.ends_with(detail.as_str()) {
            message.push_str("; caused by: ");
            message.push_str(detail.as_str());
        }
        source = cause.source();
        source_count += 1;
        if source_count >= 8 {
            message.push_str("; source chain truncated");
            break;
        }
    }
    message
}

#[cfg(test)]
mod tests {
    use super::{classify_http_request_error, format_http_request_error, HttpRequestErrorKind};

    #[test]
    fn classifies_invalid_url_as_builder_error() {
        let error = reqwest::Client::new()
            .get("://invalid-url")
            .build()
            .expect_err("invalid URL should fail request construction");

        assert_eq!(
            classify_http_request_error(&error),
            HttpRequestErrorKind::Builder
        );
        assert_eq!(HttpRequestErrorKind::Builder.as_str(), "builder");
    }

    #[test]
    fn formats_error_kind_and_context_without_request_headers() {
        let error = reqwest::Client::new()
            .get("://invalid-url")
            .build()
            .expect_err("invalid URL should fail request construction");
        let message = format_http_request_error("project service request", error);

        assert!(message.starts_with("project service request failed (kind=builder):"));
        assert!(!message.contains("authorization"));
    }
}
