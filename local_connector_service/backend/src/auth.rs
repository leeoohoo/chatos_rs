// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap;
use chatos_service_runtime::{
    bearer_token_from_headers as parse_bearer_token_from_headers,
    normalize_owned_identity_text as normalize_text, request_user_service_json, BearerTokenError,
    UserServiceVerifiedPrincipal, UserServiceVerifyResponse,
};
use reqwest::Method;
use serde::Serialize;

use crate::config::AppConfig;
use crate::models::CurrentUser;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceProofVerificationRequest {
    pub surface: String,
    pub method: String,
    pub target: String,
    pub body_sha512: String,
    pub client_session_id: String,
    pub device_id: String,
    pub timestamp: i64,
    pub nonce: String,
    pub signature_algorithm: String,
    pub signature: String,
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
    let payload = request_user_service_json::<(), UserServiceVerifyResponse>(
        client,
        config.user_service_base_url.as_str(),
        Method::GET,
        "/api/auth/verify",
        Some(token),
        None,
    )
    .await?;
    current_user_from_principal(payload.principal)
}

pub async fn verify_request_via_user_service(
    config: &AppConfig,
    client: &reqwest::Client,
    token: &str,
    proof: &DeviceProofVerificationRequest,
) -> Result<CurrentUser, String> {
    let payload = request_user_service_json::<_, UserServiceVerifyResponse>(
        client,
        config.user_service_base_url.as_str(),
        Method::POST,
        "/api/auth/device-proof/verify",
        Some(token),
        Some(proof),
    )
    .await?;
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
        token_jti: normalize_text(principal.jti),
        user_id,
        username: principal.username.and_then(normalize_text),
        display_name: principal.display_name.and_then(normalize_text),
        role: principal
            .role
            .and_then(normalize_text)
            .unwrap_or_else(|| "user".to_string()),
        owner_user_id: principal.owner_user_id.and_then(normalize_text),
        scopes: principal.scopes,
    })
}
