// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use bytes::BytesMut;
use futures::StreamExt;
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::sync::OnceLock;

static USER_SERVICE_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

const USER_SERVICE_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const USER_SERVICE_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

pub(super) async fn request_json<TBody, TResp>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<TResp, String>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let resolved_base_url =
        chatos_service_runtime::resolve_service_base_url("user-service", base_url).await;
    let response = build_request(
        method,
        resolved_base_url.as_str(),
        path,
        access_token,
        body,
        timeout_ms,
    )?
    .send()
    .await
    .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = read_user_service_body_limited(response, USER_SERVICE_ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
            .unwrap_or_default();
        return Err(format!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(status, body.as_str())
        ));
    }
    let body = read_user_service_body_limited(response, USER_SERVICE_RESPONSE_LIMIT_BYTES).await?;
    serde_json::from_slice::<TResp>(body.as_ref()).map_err(|err| err.to_string())
}

pub(super) async fn request_empty<TBody>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<(), String>
where
    TBody: Serialize + ?Sized,
{
    let resolved_base_url =
        chatos_service_runtime::resolve_service_base_url("user-service", base_url).await;
    let response = build_request(
        method,
        resolved_base_url.as_str(),
        path,
        access_token,
        body,
        timeout_ms,
    )?
    .send()
    .await
    .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = read_user_service_body_limited(response, USER_SERVICE_ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
            .unwrap_or_default();
        return Err(format!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(status, body.as_str())
        ));
    }
    Ok(())
}

fn build_request<TBody>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<reqwest::RequestBuilder, String>
where
    TBody: Serialize + ?Sized,
{
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let mut request = user_service_http_client()
        .request(method, endpoint)
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64));
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    Ok(request)
}

fn user_service_http_client() -> &'static reqwest::Client {
    USER_SERVICE_HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

async fn read_user_service_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_user_service_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_user_service_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_user_service_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "user_service response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

fn extract_error_message(status: StatusCode, body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            let error = value
                .get("error")
                .and_then(|item| item.as_str())
                .map(ToOwned::to_owned);
            let detail = value
                .get("detail")
                .and_then(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned);
            match (error, detail) {
                (Some(error), Some(detail)) => Some(format!("{error}: {detail}")),
                (Some(error), None) => Some(error),
                (None, Some(detail)) => Some(detail),
                (None, None) => None,
            }
        })
        .unwrap_or_else(|| format!("HTTP {}", status.as_u16()))
}

#[cfg(test)]
mod tests {
    use super::ensure_user_service_body_within_limit;

    #[test]
    fn user_service_body_limit_accepts_boundary_size() {
        assert!(ensure_user_service_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn user_service_body_limit_rejects_oversized_body() {
        let err = ensure_user_service_body_within_limit(1025, 1024)
            .expect_err("oversized body should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
