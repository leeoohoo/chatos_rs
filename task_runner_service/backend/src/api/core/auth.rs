// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use crate::models::UserRole;
use serde::{Deserialize, Serialize};

pub(in crate::api) async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let token = bearer_token_from_request(&request).map_err(ApiError::unauthorized)?;
    let access_token = token.to_string();
    let current_user = current_user_from_user_service_token(&state.config, token).await?;
    let downstream_access_token = downstream_access_token_from_headers(
        &state.config,
        request.headers(),
        &access_token,
        &current_user,
    )
    .await?;
    request.extensions_mut().insert(current_user);
    Ok(
        crate::auth::with_access_token_scope(Some(downstream_access_token), next.run(request))
            .await,
    )
}

pub(in crate::api) async fn login_handler(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let payload = login_via_user_service(&state.config, &input).await?;
    let user = current_user_from_user_service_auth_user(payload.user)?.public_user();
    Ok(Json(LoginResponse {
        token: payload.token,
        user,
    }))
}

pub(in crate::api) async fn agent_token_handler(
    State(_state): State<AppState>,
    Json(_input): Json<AgentTokenRequest>,
) -> Result<Json<AgentTokenResponse>, ApiError> {
    Err(ApiError::forbidden(
        "agent token must be exchanged through user_service",
    ))
}

pub(in crate::api) async fn current_user_handler(
    Extension(current_user): Extension<CurrentUser>,
) -> Json<CurrentUserResponse> {
    Json(CurrentUserResponse {
        user: current_user.public_user(),
    })
}

pub(in crate::api) async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = bearer_token_from_headers(&headers).map_err(ApiError::unauthorized)?;
    logout_via_user_service(&state.config, token).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(in crate::api) async fn current_user_from_user_service_token(
    config: &crate::config::AppConfig,
    token: &str,
) -> Result<CurrentUser, ApiError> {
    let payload = verify_token_via_user_service(config, token).await?;
    current_user_from_verified_principal(payload.principal)
}

fn bearer_token_from_request(request: &Request) -> Result<&str, String> {
    bearer_token_from_headers(request.headers()).or_else(|_| {
        token_from_query(request.uri().query()).ok_or_else(|| "缺少登录令牌".to_string())
    })
}

pub(in crate::api) fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, String> {
    let value = headers
        .get(header::AUTHORIZATION)
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

async fn downstream_access_token_from_headers(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
    access_token: &str,
    current_user: &CurrentUser,
) -> Result<String, ApiError> {
    let Some(user_access_token) = user_access_token_from_headers(headers)? else {
        return Ok(access_token.to_string());
    };
    let user = current_user_from_user_service_token(config, user_access_token.as_str()).await?;
    ensure_same_owner_scope(current_user, &user)?;
    Ok(user_access_token)
}

fn user_access_token_from_headers(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    for key in [
        "x-chatos-user-authorization",
        "x-user-service-authorization",
        "x-chatos-user-token",
    ] {
        let Some(value) = header_text(headers, key) else {
            continue;
        };
        let token = if let Some(token) = value.strip_prefix("Bearer ").map(str::trim) {
            token
        } else if let Some(token) = value.strip_prefix("bearer ").map(str::trim) {
            token
        } else {
            value.as_str()
        };
        if token.is_empty() {
            continue;
        }
        return Ok(Some(token.to_string()));
    }
    Ok(None)
}

fn ensure_same_owner_scope(current_user: &CurrentUser, user: &CurrentUser) -> Result<(), ApiError> {
    let current_owner = current_user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("token missing owner scope"))?;
    let user_owner = user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("user token missing owner scope"))?;
    if current_owner == user_owner {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "token and user token owner scope do not match",
        ))
    }
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn token_from_query(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        ((key == "access_token" || key == "token") && !value.is_empty()).then_some(value)
    })
}

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

async fn login_via_user_service(
    config: &crate::config::AppConfig,
    input: &LoginRequest,
) -> Result<UserServiceLoginResponse, ApiError> {
    request_user_service_json(
        config,
        reqwest::Method::POST,
        "/api/auth/login",
        None,
        Some(&UserServiceLoginRequest {
            username: input.username.as_str(),
            password: input.password.as_str(),
        }),
    )
    .await
}

async fn verify_token_via_user_service(
    config: &crate::config::AppConfig,
    token: &str,
) -> Result<UserServiceVerifyResponse, ApiError> {
    request_user_service_json::<(), _>(
        config,
        reqwest::Method::GET,
        "/api/auth/verify",
        Some(token),
        None,
    )
    .await
}

async fn logout_via_user_service(
    config: &crate::config::AppConfig,
    token: &str,
) -> Result<(), ApiError> {
    request_user_service_empty::<()>(
        config,
        reqwest::Method::POST,
        "/api/auth/logout",
        Some(token),
        None,
    )
    .await
}

async fn request_user_service_json<TBody, TResp>(
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

async fn request_user_service_empty<TBody>(
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
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
        .map_err(|err| ApiError::bad_gateway(format!("build user_service client failed: {err}")))?;
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
        .map_err(|err| ApiError::bad_gateway(format!("user_service request failed: {err}")))?;
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

fn current_user_from_user_service_auth_user(
    user: UserServiceAuthUser,
) -> Result<CurrentUser, ApiError> {
    let principal_type = user
        .principal_type
        .as_deref()
        .unwrap_or("human_user")
        .trim()
        .to_string();
    if principal_type != "human_user" {
        return Err(ApiError::unauthorized(
            "task_runner login requires a human user",
        ));
    }
    let username = normalize_identity_text(user.username.as_deref()).unwrap_or(user.id.as_str());
    Ok(CurrentUser {
        id: user.id.clone(),
        username: username.to_string(),
        display_name: normalize_identity_text(user.display_name.as_deref())
            .unwrap_or(username)
            .to_string(),
        role: map_user_service_role(user.role.as_deref()),
        owner_user_id: Some(user.id.clone()),
        owner_username: Some(username.to_string()),
        owner_display_name: normalize_identity_text(user.display_name.as_deref())
            .map(ToOwned::to_owned),
    })
}

fn current_user_from_verified_principal(
    principal: UserServiceVerifiedPrincipal,
) -> Result<CurrentUser, ApiError> {
    match principal.principal_type.as_str() {
        "human_user" => {
            let user_id = normalize_identity_text(principal.user_id.as_deref())
                .ok_or_else(|| ApiError::unauthorized("token missing user identity"))?;
            let username =
                normalize_identity_text(principal.username.as_deref()).unwrap_or(user_id);
            Ok(CurrentUser {
                id: user_id.to_string(),
                username: username.to_string(),
                display_name: normalize_identity_text(principal.display_name.as_deref())
                    .unwrap_or(username)
                    .to_string(),
                role: map_user_service_role(principal.role.as_deref()),
                owner_user_id: Some(user_id.to_string()),
                owner_username: Some(username.to_string()),
                owner_display_name: normalize_identity_text(principal.display_name.as_deref())
                    .map(ToOwned::to_owned),
            })
        }
        "agent_account" => {
            let agent_account_id =
                normalize_identity_text(principal.agent_account_id.as_deref())
                    .ok_or_else(|| ApiError::unauthorized("token missing agent identity"))?;
            let username = normalize_identity_text(principal.username.as_deref())
                .or_else(|| normalize_identity_text(principal.owner_username.as_deref()))
                .unwrap_or(agent_account_id);
            Ok(CurrentUser {
                id: agent_account_id.to_string(),
                username: username.to_string(),
                display_name: normalize_identity_text(principal.display_name.as_deref())
                    .unwrap_or(username)
                    .to_string(),
                role: UserRole::Agent,
                owner_user_id: normalize_identity_text(principal.owner_user_id.as_deref())
                    .map(ToOwned::to_owned),
                owner_username: normalize_identity_text(principal.owner_username.as_deref())
                    .map(ToOwned::to_owned),
                owner_display_name: normalize_identity_text(
                    principal.owner_display_name.as_deref(),
                )
                .map(ToOwned::to_owned),
            })
        }
        _ => Err(ApiError::unauthorized("unsupported principal type")),
    }
}

fn map_user_service_role(role: Option<&str>) -> UserRole {
    if role.map(str::trim) == Some("super_admin") {
        UserRole::Admin
    } else {
        UserRole::Agent
    }
}

fn normalize_identity_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
