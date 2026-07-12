// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use bytes::BytesMut;
use futures::StreamExt;
use serde::Deserialize;

const DEFAULT_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

pub(super) async fn send_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    send_json_with_limit(request, DEFAULT_RESPONSE_LIMIT_BYTES).await
}

pub(super) async fn resolve_project_service_base_url(base_url: &str) -> String {
    chatos_service_runtime::resolve_service_base_url("project-service", base_url).await
}

pub(super) async fn send_json_with_limit<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
    response_limit_bytes: usize,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = read_body_limited(response, ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
            .unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    let body = read_body_limited(response, response_limit_bytes).await?;
    serde_json::from_slice::<T>(body.as_ref()).map_err(|err| err.to_string())
}

pub(super) async fn send_optional_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<Option<T>, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        let body = read_body_limited(response, ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
            .unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    let body = read_body_limited(response, DEFAULT_RESPONSE_LIMIT_BYTES).await?;
    serde_json::from_slice::<T>(body.as_ref())
        .map(Some)
        .map_err(|err| err.to_string())
}

async fn read_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_body_within_limit(content_length as usize, limit_bytes)?;
    }
    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_body_within_limit(actual_bytes: usize, limit_bytes: usize) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "Project service response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_body_within_limit;

    #[test]
    fn body_limit_accepts_boundary_size() {
        assert!(ensure_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn body_limit_rejects_oversized_body() {
        let err = ensure_body_within_limit(1025, 1024).expect_err("oversized body should fail");
        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
