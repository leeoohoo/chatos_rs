// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::error::Error as _;

use chatos_service_runtime::http_body::read_response_preview_text_limited_or_message;
use tokio_util::sync::CancellationToken;

const ERROR_RESPONSE_BODY_LIMIT_BYTES: usize = 16 * 1024;

pub(super) async fn send_json_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    payload_body: Vec<u8>,
    abort_token: Option<CancellationToken>,
    force_identity_encoding: bool,
) -> Result<reqwest::Response, String> {
    let mut request = client
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload_body);
    if force_identity_encoding {
        request = request
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONNECTION, "close")
            .version(reqwest::Version::HTTP_11);
    }

    let future = request.send();
    if let Some(token) = abort_token {
        tokio::select! {
            _ = token.cancelled() => Err("aborted".to_string()),
            response = future => response.map_err(format_reqwest_error),
        }
    } else {
        future.await.map_err(format_reqwest_error)
    }
}

fn format_reqwest_error(err: reqwest::Error) -> String {
    let kind = if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_body() {
        "body"
    } else if err.is_decode() {
        "decode"
    } else if err.is_request() {
        "request"
    } else {
        "unknown"
    };
    let mut message = format!("AI transport error (kind={kind}): {err}");
    let mut source = err.source();
    while let Some(cause) = source {
        let detail = cause.to_string();
        if !detail.is_empty() && !message.ends_with(detail.as_str()) {
            message.push_str("; caused by: ");
            message.push_str(detail.as_str());
        }
        source = cause.source();
    }
    message
}

pub(super) fn retry_after_delay_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1_000))
}

pub(super) fn serialize_request_payload(payload: &serde_json::Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(payload)
        .map_err(|err| format!("failed to serialize AI request payload: {err}"))
}

pub(super) fn validate_request_payload_size(
    size: usize,
    request_body_limit_bytes: Option<usize>,
) -> Result<(), String> {
    let Some(limit) = request_body_limit_bytes.filter(|value| *value > 0) else {
        return Ok(());
    };
    if size > limit {
        Err(format!(
            "AI request payload too large: {size} bytes exceeds {limit} bytes"
        ))
    } else {
        Ok(())
    }
}

pub(super) fn log_preview(value: &str) -> String {
    const MAX_LOG_PREVIEW_CHARS: usize = 2_000;
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_LOG_PREVIEW_CHARS {
        return trimmed.to_string();
    }
    let preview = trimmed
        .chars()
        .take(MAX_LOG_PREVIEW_CHARS)
        .collect::<String>();
    format!("{preview}... [truncated]")
}

pub(super) async fn read_error_response_text_limited(response: reqwest::Response) -> String {
    read_response_preview_text_limited_or_message(response, ERROR_RESPONSE_BODY_LIMIT_BYTES).await
}

#[cfg(test)]
mod tests {
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    use super::retry_after_delay_ms;

    #[test]
    fn reads_retry_after_delta_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(retry_after_delay_ms(&headers), Some(7_000));
    }

    #[test]
    fn ignores_invalid_retry_after_values() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("later"));
        assert_eq!(retry_after_delay_ms(&headers), None);
    }
}
