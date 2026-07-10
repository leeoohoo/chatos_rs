// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::header::AUTHORIZATION;
use axum::http::HeaderMap;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::models::{CurrentUser, LoginRequest, LoginResponse};

#[derive(Debug, Clone)]
pub struct AccessToken(pub String);

#[derive(Debug, Serialize)]
struct UserServiceLoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Debug, Deserialize)]
struct UserServiceAuthUser {
    id: String,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    principal_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceLoginResponse {
    token: String,
    user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifiedPrincipal {
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    agent_account_id: Option<String>,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
    owner_display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifyResponse {
    principal: UserServiceVerifiedPrincipal,
}

pub async fn login_via_user_service(
    config: &AppConfig,
    input: &LoginRequest,
) -> Result<LoginResponse, String> {
    let payload: UserServiceLoginResponse = request_user_service_json(
        config,
        Method::POST,
        "/api/auth/login",
        None,
        Some(&UserServiceLoginRequest {
            username: input.username.as_str(),
            password: input.password.as_str(),
        }),
    )
    .await?;
    let user = current_user_from_user_service_auth_user(payload.user)?;
    Ok(LoginResponse {
        token: payload.token,
        user,
    })
}

pub async fn verify_token_via_user_service(
    config: &AppConfig,
    token: &str,
) -> Result<CurrentUser, String> {
    let payload: UserServiceVerifyResponse = request_user_service_json::<(), _>(
        config,
        Method::GET,
        "/api/auth/verify",
        Some(token),
        None,
    )
    .await?;
    current_user_from_verified_principal(payload.principal)
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

async fn request_user_service_json<TBody, TResp>(
    config: &AppConfig,
    method: Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResp, String>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let endpoint = format!(
        "{}{}",
        config.user_service_base_url.trim().trim_end_matches('/'),
        path
    );
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("user_service request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(if text.trim().is_empty() {
            format!("user_service request failed with status {status}")
        } else {
            text
        });
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| format!("parse user_service response failed: {err}"))
}

fn current_user_from_user_service_auth_user(
    user: UserServiceAuthUser,
) -> Result<CurrentUser, String> {
    let principal_type = user
        .principal_type
        .as_deref()
        .unwrap_or("human_user")
        .trim();
    if principal_type != "human_user" {
        return Err("plugin management login requires a human user".to_string());
    }
    Ok(CurrentUser {
        principal_type: "human_user".to_string(),
        user_id: user.id,
        username: normalize_identity_text(user.username.as_deref())
            .unwrap_or("user")
            .to_string(),
        display_name: normalize_identity_text(user.display_name.as_deref())
            .or_else(|| normalize_identity_text(user.username.as_deref()))
            .unwrap_or("User")
            .to_string(),
        role: normalize_identity_text(user.role.as_deref())
            .unwrap_or("user")
            .to_string(),
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
    })
}

fn current_user_from_verified_principal(
    principal: UserServiceVerifiedPrincipal,
) -> Result<CurrentUser, String> {
    let principal_type = principal.principal_type.trim();
    match principal_type {
        "human_user" => {
            let user_id = normalize_identity_text(principal.user_id.as_deref())
                .ok_or_else(|| "verified user principal missing user_id".to_string())?;
            Ok(CurrentUser {
                principal_type: "human_user".to_string(),
                user_id: user_id.to_string(),
                username: normalize_identity_text(principal.username.as_deref())
                    .unwrap_or(user_id)
                    .to_string(),
                display_name: normalize_identity_text(principal.display_name.as_deref())
                    .or_else(|| normalize_identity_text(principal.username.as_deref()))
                    .unwrap_or(user_id)
                    .to_string(),
                role: normalize_identity_text(principal.role.as_deref())
                    .unwrap_or("user")
                    .to_string(),
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
            })
        }
        "agent_account" => {
            let owner_user_id = normalize_identity_text(principal.owner_user_id.as_deref())
                .ok_or_else(|| "verified agent principal missing owner_user_id".to_string())?;
            let agent_id = normalize_identity_text(principal.agent_account_id.as_deref())
                .unwrap_or(owner_user_id);
            Ok(CurrentUser {
                principal_type: "agent_account".to_string(),
                user_id: owner_user_id.to_string(),
                username: normalize_identity_text(principal.username.as_deref())
                    .unwrap_or(agent_id)
                    .to_string(),
                display_name: normalize_identity_text(principal.display_name.as_deref())
                    .or_else(|| normalize_identity_text(principal.username.as_deref()))
                    .unwrap_or(agent_id)
                    .to_string(),
                role: normalize_identity_text(principal.role.as_deref())
                    .unwrap_or("user")
                    .to_string(),
                owner_user_id: Some(owner_user_id.to_string()),
                owner_username: normalize_identity_text(principal.owner_username.as_deref())
                    .map(ToOwned::to_owned),
                owner_display_name: normalize_identity_text(
                    principal.owner_display_name.as_deref(),
                )
                .map(ToOwned::to_owned),
            })
        }
        _ => Err(format!("unsupported principal_type: {principal_type}")),
    }
}

fn normalize_identity_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
