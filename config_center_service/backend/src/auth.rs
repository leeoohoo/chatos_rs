// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::header::AUTHORIZATION;
use axum::http::HeaderMap;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::models::{CurrentUser, LoginRequest, LoginResponse};

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
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifyResponse {
    principal: UserServiceVerifiedPrincipal,
}

pub async fn login(config: &AppConfig, input: &LoginRequest) -> Result<LoginResponse, String> {
    let response: UserServiceLoginResponse = request_json(
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
    Ok(LoginResponse {
        token: response.token,
        user: user_from_login(response.user)?,
    })
}

pub async fn verify(config: &AppConfig, token: &str) -> Result<CurrentUser, String> {
    let response: UserServiceVerifyResponse =
        request_json::<(), _>(config, Method::GET, "/api/auth/verify", Some(token), None).await?;
    let principal = response.principal;
    if principal.principal_type.trim() != "human_user" {
        return Err("configuration center only accepts human users".to_string());
    }
    let user_id = normalized(principal.user_id.as_deref())
        .ok_or_else(|| "verified principal missing user_id".to_string())?;
    Ok(CurrentUser {
        user_id: user_id.to_string(),
        username: normalized(principal.username.as_deref())
            .unwrap_or(user_id)
            .to_string(),
        display_name: normalized(principal.display_name.as_deref())
            .or_else(|| normalized(principal.username.as_deref()))
            .unwrap_or(user_id)
            .to_string(),
        role: normalized(principal.role.as_deref())
            .unwrap_or("user")
            .to_string(),
    })
}

pub fn bearer_token(headers: &HeaderMap) -> Result<&str, String> {
    let value = headers
        .get(AUTHORIZATION)
        .ok_or_else(|| "missing authorization header".to_string())?
        .to_str()
        .map_err(|_| "invalid authorization header".to_string())?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next().unwrap_or_default();
    let token = parts.next().unwrap_or_default();
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err("invalid bearer token".to_string());
    }
    Ok(token)
}

async fn request_json<TBody, TResponse>(
    config: &AppConfig,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResponse, String>
where
    TBody: Serialize + ?Sized,
    TResponse: serde::de::DeserializeOwned,
{
    let endpoint = format!(
        "{}{}",
        config.user_service_base_url.trim_end_matches('/'),
        path
    );
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.request(method, endpoint);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!("user service returned {status}")
        } else {
            body
        });
    }
    response.json().await.map_err(|err| err.to_string())
}

fn user_from_login(user: UserServiceAuthUser) -> Result<CurrentUser, String> {
    if user
        .principal_type
        .as_deref()
        .unwrap_or("human_user")
        .trim()
        != "human_user"
    {
        return Err("configuration center only accepts human users".to_string());
    }
    Ok(CurrentUser {
        user_id: user.id.clone(),
        username: normalized(user.username.as_deref())
            .unwrap_or(user.id.as_str())
            .to_string(),
        display_name: normalized(user.display_name.as_deref())
            .or_else(|| normalized(user.username.as_deref()))
            .unwrap_or(user.id.as_str())
            .to_string(),
        role: normalized(user.role.as_deref())
            .unwrap_or("user")
            .to_string(),
    })
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
