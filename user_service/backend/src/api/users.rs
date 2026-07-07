// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};

use crate::auth::{hash_password, normalize_display_name, normalize_username, CurrentPrincipal};
use crate::integrations::{
    provision_harness_user_public_register, provision_harness_user_public_register_result,
};
use crate::models::{
    CreateUserRequest, UpdateUserRequest, UserRecord, UserSummaryRecord, USER_ROLE_SUPER_ADMIN,
    USER_ROLE_USER,
};
use crate::secrets::decrypt_secret;
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, forbidden, internal_error, not_found, require_super_admin, ApiResult};

pub async fn list_users(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<Vec<UserSummaryRecord>> {
    if principal.is_super_admin() {
        return state
            .store
            .list_users_summary()
            .await
            .map(Json)
            .map_err(internal_error);
    }

    let Some(user_id) = principal.user_id.as_deref() else {
        return Err(not_found("current user not found"));
    };
    let Some(summary) = state
        .store
        .get_user_summary(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("user not found"));
    };
    Ok(Json(vec![summary]))
}

pub async fn create_user(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateUserRequest>,
) -> ApiResult<UserSummaryRecord> {
    require_super_admin(&principal)?;

    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if state
        .store
        .find_user_by_username(username.as_str())
        .await
        .map_err(internal_error)?
        .is_some()
    {
        return Err(bad_request("username already exists"));
    }

    let role = normalize_role(input.role.as_deref())?;
    let now = now_rfc3339();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.clone(),
        display_name: normalize_display_name(input.display_name.as_deref(), &username),
        password_hash: hash_password(input.password.as_str()).map_err(bad_request)?,
        role: role.to_string(),
        enabled: input.enabled.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
        last_login_at: None,
    };
    state
        .store
        .insert_user_record(&user)
        .await
        .map_err(internal_error)?;
    if user.enabled {
        let _ =
            provision_harness_user_public_register(&state, &user, input.password.as_str()).await;
    }

    let summary = state
        .store
        .get_user_summary(user.id.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error("created user summary missing"))?;
    Ok(Json(summary))
}

pub async fn retry_harness_provisioning(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<UserSummaryRecord> {
    require_super_admin(&principal)?;
    if !state.config.harness_provisioning_enabled {
        return Err(bad_request("harness provisioning is disabled"));
    }

    let Some(user) = state
        .store
        .find_user_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("user not found"));
    };
    if !user.enabled {
        return Err(bad_request("cannot provision disabled user"));
    }

    let Some(record) = state
        .store
        .find_harness_provisioning_by_user_id(user.id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(bad_request("harness provisioning record not found"));
    };
    let Some(encrypted_password) = record.encrypted_password.as_deref() else {
        return Err(bad_request(
            "harness provisioning password is unavailable; reset password before retry",
        ));
    };
    let password = decrypt_secret(encrypted_password).map_err(internal_error)?;
    provision_harness_user_public_register_result(&state, &user, password.as_str())
        .await
        .map_err(internal_error)?;

    let summary = state
        .store
        .get_user_summary(user.id.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error("updated user summary missing"))?;
    Ok(Json(summary))
}

pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserRequest>,
) -> ApiResult<UserSummaryRecord> {
    let is_self = principal.user_id.as_deref() == Some(id.as_str());
    if !principal.is_super_admin() && !is_self {
        return Err(forbidden("cannot update another user"));
    }

    let Some(mut user) = state
        .store
        .find_user_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("user not found"));
    };

    if let Some(display_name) = input.display_name.as_deref() {
        user.display_name = normalize_display_name(Some(display_name), user.username.as_str());
    }
    if let Some(password) = input.password.as_deref() {
        user.password_hash = hash_password(password).map_err(bad_request)?;
    }

    if let Some(role) = input.role.as_deref() {
        if !principal.is_super_admin() {
            return Err(forbidden("only super_admin can change role"));
        }
        let role = normalize_role(Some(role))?;
        if is_self && role != USER_ROLE_SUPER_ADMIN {
            return Err(forbidden("cannot demote current super_admin session"));
        }
        if user.role == USER_ROLE_SUPER_ADMIN
            && role != USER_ROLE_SUPER_ADMIN
            && state
                .store
                .count_enabled_super_admins()
                .await
                .map_err(internal_error)?
                <= 1
        {
            return Err(forbidden("at least one enabled super_admin is required"));
        }
        user.role = role.to_string();
    }

    if let Some(enabled) = input.enabled {
        if !principal.is_super_admin() {
            return Err(forbidden("only super_admin can change enabled status"));
        }
        if is_self && !enabled {
            return Err(forbidden("cannot disable current authenticated user"));
        }
        if user.role == USER_ROLE_SUPER_ADMIN
            && user.enabled
            && !enabled
            && state
                .store
                .count_enabled_super_admins()
                .await
                .map_err(internal_error)?
                <= 1
        {
            return Err(forbidden("at least one enabled super_admin is required"));
        }
        user.enabled = enabled;
    }

    user.updated_at = now_rfc3339();
    state
        .store
        .update_user_record(&user)
        .await
        .map_err(internal_error)?;

    let summary = state
        .store
        .get_user_summary(user.id.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error("updated user summary missing"))?;
    Ok(Json(summary))
}

fn normalize_role(
    value: Option<&str>,
) -> Result<&'static str, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match value.unwrap_or(USER_ROLE_USER).trim() {
        USER_ROLE_SUPER_ADMIN => Ok(USER_ROLE_SUPER_ADMIN),
        USER_ROLE_USER => Ok(USER_ROLE_USER),
        _ => Err(bad_request("role must be super_admin or user")),
    }
}
