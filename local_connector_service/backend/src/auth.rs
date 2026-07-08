// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::header::AUTHORIZATION;
use axum::http::HeaderMap;
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
    let value = headers
        .get(AUTHORIZATION)
        .ok_or_else(|| "缺少登录令牌".to_string())?
        .to_str()
        .map_err(|_| "登录令牌格式不正确".to_string())?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next().unwrap_or_default();
    let token = parts.next().unwrap_or_default();
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err("登录令牌格式不正确".to_string());
    }
    Ok(token)
}

pub async fn verify_token_via_user_service(
    config: &AppConfig,
    token: &str,
) -> Result<CurrentUser, String> {
    let endpoint = format!(
        "{}/api/auth/verify",
        config.user_service_base_url.trim().trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response = client
        .request(Method::GET, endpoint)
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|err| format!("user_service request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(if text.trim().is_empty() {
            format!("user_service verify failed with status {status}")
        } else {
            text
        });
    }
    let payload = response
        .json::<UserServiceVerifyResponse>()
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

fn normalize_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
