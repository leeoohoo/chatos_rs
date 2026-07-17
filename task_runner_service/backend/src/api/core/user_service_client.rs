// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use serde::Serialize;

pub(super) async fn request_user_service_json<TBody, TResp>(
    config: &crate::config::AppConfig,
    method: reqwest::Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResp, ApiError>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let response = request_user_service(config, method, path, access_token, body).await?;
    read_response_json_limited::<TResp>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| ApiError::bad_gateway(format!("parse user_service response failed: {err}")))
}

pub(super) async fn request_user_service_empty<TBody>(
    config: &crate::config::AppConfig,
    method: reqwest::Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<(), ApiError>
where
    TBody: Serialize + ?Sized,
{
    let _response = request_user_service(config, method, path, access_token, body).await?;
    Ok(())
}

async fn request_user_service<TBody>(
    config: &crate::config::AppConfig,
    method: reqwest::Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<reqwest::Response, ApiError>
where
    TBody: Serialize + ?Sized,
{
    let endpoint = format!(
        "{}{}",
        config.user_service_base_url.trim().trim_end_matches('/'),
        path
    );
    let client = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
        .map_err(|err| ApiError::bad_gateway(format!("build user_service client failed: {err}")))?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| ApiError {
        status: upstream_gateway_status(&err),
        message: format!("user_service request failed: {err}"),
    })?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let message =
        read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
    Err(ApiError {
        status,
        message: if message.trim().is_empty() {
            "user_service request failed".to_string()
        } else {
            message
        },
    })
}
