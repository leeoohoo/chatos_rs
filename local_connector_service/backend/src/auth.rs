// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap;
use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{
    bearer_token_from_headers as parse_bearer_token_from_headers,
    normalize_owned_identity_text as normalize_text, BearerTokenError,
};
use reqwest::Method;
use serde::Deserialize;

use crate::config::AppConfig;
use crate::models::CurrentUser;

#[derive(Debug, Deserialize)]
struct UserServiceVerifiedPrincipal {
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    owner_user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifyResponse {
    principal: UserServiceVerifiedPrincipal,
}

pub fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, String> {
    match parse_bearer_token_from_headers(headers) {
        Ok(token) => Ok(token),
        Err(BearerTokenError::MissingAuthorizationHeader) => Err("缺少登录令牌".to_string()),
        Err(
            BearerTokenError::InvalidAuthorizationHeader | BearerTokenError::InvalidBearerToken,
        ) => Err("登录令牌格式不正确".to_string()),
    }
}

pub async fn verify_token_via_user_service(
    config: &AppConfig,
    client: &reqwest::Client,
    token: &str,
) -> Result<CurrentUser, String> {
    let endpoint = format!(
        "{}/api/auth/verify",
        config.user_service_base_url.trim().trim_end_matches('/')
    );
    let response = client
        .request(Method::GET, endpoint)
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|err| format!("user_service request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(if text.trim().is_empty() {
            format!("user_service verify failed with status {status}")
        } else {
            text
        });
    }
    let payload =
        read_response_json_limited::<UserServiceVerifyResponse>(response, JSON_BODY_LIMIT_BYTES)
            .await
            .map_err(|err| format!("parse user_service verify response failed: {err}"))?;
    current_user_from_principal(payload.principal)
}

fn current_user_from_principal(
    principal: UserServiceVerifiedPrincipal,
) -> Result<CurrentUser, String> {
    let principal_type = principal.principal_type.trim().to_string();
    if principal_type != "human_user" && principal_type != "agent_account" {
        return Err("unsupported principal type for local connector service".to_string());
    }
    let user_id = principal
        .user_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "user_service principal missing user_id".to_string())?;
    Ok(CurrentUser {
        principal_type,
        user_id,
        username: principal.username.and_then(normalize_text),
        display_name: principal.display_name.and_then(normalize_text),
        role: principal
            .role
            .and_then(normalize_text)
            .unwrap_or_else(|| "user".to_string()),
        owner_user_id: principal.owner_user_id.and_then(normalize_text),
    })
}
