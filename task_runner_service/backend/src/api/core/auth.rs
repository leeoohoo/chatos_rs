// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::UserRole;
use chatos_service_runtime::{
    bearer_token_from_headers as parse_bearer_token_from_headers,
    normalized_identity_text as normalize_identity_text, query_has_nonempty_parameter,
    BearerTokenError,
};
use serde::{Deserialize, Serialize};

use super::user_service_client::{request_user_service_empty, request_user_service_json};

pub(in crate::api) async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let token = bearer_token_from_request(&state, &request).map_err(ApiError::unauthorized)?;
    let access_token = token;
    let current_user =
        current_user_from_user_service_token(&state.config, access_token.as_str()).await?;
    let downstream_access_token = downstream_access_token_from_headers(
        &state.config,
        request.headers(),
        access_token.as_str(),
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

pub(in crate::api) async fn sse_ticket_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SseTicketResponse>, ApiError> {
    let token = bearer_token_from_headers(&headers).map_err(ApiError::unauthorized)?;
    let issued = state.sse_tickets.issue(token);
    Ok(Json(SseTicketResponse {
        ticket: issued.ticket,
        expires_in: issued.expires_in,
        expires_at_unix: issued.expires_at_unix,
    }))
}

fn bearer_token_from_request(state: &AppState, request: &Request) -> Result<String, String> {
    if let Ok(token) = bearer_token_from_headers(request.headers()) {
        return Ok(token.to_string());
    }

    let Some(ticket) = sse_ticket_from_query(request.uri().query()) else {
        if query_has_nonempty_parameter(request.uri().query(), &["access_token", "token"]) {
            return Err(
                "URL query access tokens are not supported; use Authorization header".to_string(),
            );
        }
        return Err("缺少登录令牌".to_string());
    };
    state
        .sse_tickets
        .consume(ticket)
        .map(|record| record.access_token)
        .ok_or_else(|| "SSE ticket is invalid or expired".to_string())
}

pub(in crate::api) fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, String> {
    match parse_bearer_token_from_headers(headers) {
        Ok(token) => Ok(token),
        Err(BearerTokenError::MissingAuthorizationHeader) => Err("缺少登录令牌".to_string()),
        Err(
            BearerTokenError::InvalidAuthorizationHeader | BearerTokenError::InvalidBearerToken,
        ) => Err("登录令牌格式不正确".to_string()),
    }
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

fn sse_ticket_from_query(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        (key == "sse_ticket" && !value.is_empty()).then_some(value)
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

#[cfg(test)]
mod tests {
    use super::sse_ticket_from_query;
    use chatos_service_runtime::query_has_nonempty_parameter;

    #[test]
    fn sse_ticket_query_is_supported() {
        assert_eq!(
            sse_ticket_from_query(Some("plain=value&sse_ticket=ticket-1")),
            Some("ticket-1")
        );
    }

    #[test]
    fn legacy_query_access_tokens_are_detected() {
        let names = ["access_token", "token"];
        assert!(query_has_nonempty_parameter(
            Some("access_token=long-lived"),
            &names
        ));
        assert!(query_has_nonempty_parameter(
            Some("token=long-lived"),
            &names
        ));
        assert!(!query_has_nonempty_parameter(
            Some("sse_ticket=ticket-1"),
            &names
        ));
    }
}
