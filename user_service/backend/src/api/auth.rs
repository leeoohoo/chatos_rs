// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::{Extension, Json};
use serde_json::json;

use crate::auth::{
    encode_user_token, hash_password, normalize_display_name, normalize_username, verify_password,
    CurrentPrincipal,
};
use crate::models::{
    CurrentUserResponse, LoginRequest, LoginResponse, RegisterRequest, TokenVerifyResponse,
    UserRecord, VerifiedPrincipal, USER_ROLE_USER,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, internal_error, not_found, ApiResult, ApiStatusResult};

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }

    let Some(user) = state
        .store
        .find_user_by_username(username.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(unauthorized("invalid username or password"));
    };
    if !user.enabled {
        return Err(unauthorized("account has been disabled"));
    }
    if !verify_password(input.password.as_str(), user.password_hash.as_str()) {
        return Err(unauthorized("invalid username or password"));
    }

    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let token = encode_user_token(&state.config, &user).map_err(internal_error)?;

    Ok(Json(LoginResponse {
        token,
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            String::new(),
            0,
        ),
    }))
}

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> ApiResult<LoginResponse> {
    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }
    if state
        .store
        .find_user_by_username(username.as_str())
        .await
        .map_err(internal_error)?
        .is_some()
    {
        return Err(bad_request("username already exists"));
    }

    let now = now_rfc3339();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.clone(),
        display_name: normalize_display_name(input.display_name.as_deref(), &username),
        password_hash: hash_password(input.password.as_str()).map_err(bad_request)?,
        role: USER_ROLE_USER.to_string(),
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
        last_login_at: None,
    };
    state
        .store
        .insert_user_record(&user)
        .await
        .map_err(internal_error)?;
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let token = encode_user_token(&state.config, &user).map_err(internal_error)?;

    Ok(Json(LoginResponse {
        token,
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            String::new(),
            0,
        ),
    }))
}

pub async fn me(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<CurrentUserResponse> {
    let Some(user_id) = principal.user_id.as_deref() else {
        return Err(not_found("current user not found"));
    };
    let Some(user) = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("current user not found"));
    };
    Ok(Json(CurrentUserResponse {
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            principal.jti,
            principal.exp,
        ),
    }))
}

pub async fn verify(
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<TokenVerifyResponse> {
    Ok(Json(TokenVerifyResponse {
        principal: VerifiedPrincipal {
            sub: principal.sub,
            jti: principal.jti,
            exp: principal.exp,
            principal_type: principal.principal_type,
            user_id: principal.user_id,
            username: principal.username,
            display_name: principal.display_name,
            role: principal.role,
            agent_account_id: principal.agent_account_id,
            owner_user_id: principal.owner_user_id,
            owner_username: principal.owner_username,
            owner_display_name: principal.owner_display_name,
            scopes: principal.scopes,
        },
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
    state
        .store
        .revoke_token(
            principal.jti.as_str(),
            principal.sub.as_str(),
            principal.exp as i64,
        )
        .await
        .map_err(internal_error)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

fn unauthorized(message: &str) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        Json(json!({ "error": message })),
    )
}

fn current_auth_user(
    id: String,
    username: String,
    display_name: String,
    role: String,
    jti: String,
    exp: usize,
) -> crate::models::AuthUser {
    CurrentPrincipal {
        sub: format!("user:{id}"),
        jti,
        exp,
        principal_type: crate::models::PRINCIPAL_TYPE_HUMAN_USER.to_string(),
        user_id: Some(id),
        username: Some(username),
        display_name: Some(display_name),
        role: Some(role),
        agent_account_id: None,
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
        scopes: vec!["user_service".to_string()],
    }
    .auth_user()
}
