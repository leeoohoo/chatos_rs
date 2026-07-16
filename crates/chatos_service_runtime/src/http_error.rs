// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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

#[cfg(test)]
mod tests {
    use super::{classify_http_request_error, HttpRequestErrorKind};

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
}
