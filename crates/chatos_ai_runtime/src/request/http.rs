// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
            response = future => response.map_err(|err| err.to_string()),
        }
    } else {
        future.await.map_err(|err| err.to_string())
    }
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
