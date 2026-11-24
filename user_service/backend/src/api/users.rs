// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::Deserialize;

use crate::auth::{
    hash_password_async, normalize_display_name, normalize_username, verify_password_async,
    CurrentPrincipal,
};
use crate::integrations::{
    provision_harness_user_public_register, provision_harness_user_public_register_result,
};
use crate::models::{
    ChangePasswordRequest, CreateUserRequest, ProvisionHarnessUserRequest,
    ResetUserPasswordRequest, UpdateUserRequest, UserOptionPageResponse, UserOptionRecord,
    UserRecord, UserSummaryPageResponse, UserSummaryRecord, USER_ROLE_SUPER_ADMIN, USER_ROLE_USER,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{
    bad_request, forbidden, internal_error, not_found, require_super_admin, unauthorized,
    ApiResult, ApiStatusResult,
};

const DEFAULT_USER_PAGE_LIMIT: i64 = 20;
const MAX_USER_PAGE_LIMIT: i64 = 100;

#[derive(Debug, Default, Deserialize)]
pub struct UserListQuery {
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UserOptionsResolveRequest {
    ids: Vec<String>,
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserListQuery>,
) -> ApiResult<UserSummaryPageResponse> {
    let (search, limit, offset) = validated_user_list_query(query).map_err(bad_request)?;
    if principal.is_super_admin() {
        return state
            .store
            .list_users_summary_page(search.as_deref(), limit, offset)
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
    let matches = search
        .as_deref()
        .is_none_or(|search| user_matches_search(&summary.username, &summary.display_name, search));
    Ok(Json(UserSummaryPageResponse {
        items: (matches && offset == 0)
            .then_some(summary)
            .into_iter()
            .collect(),
        total: if matches { 1 } else { 0 },
    }))
}

pub async fn list_user_options(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserListQuery>,
) -> ApiResult<UserOptionPageResponse> {
    let (search, limit, offset) = validated_user_list_query(query).map_err(bad_request)?;
    if principal.is_super_admin() {
        return state
            .store
            .list_user_options_page(search.as_deref(), limit, offset)
            .await
            .map(Json)
            .map_err(internal_error);
    }

    let Some(user_id) = principal.user_id else {
        return Err(not_found("current user not found"));
    };
    let username = principal.username.unwrap_or_default();
    let option = UserOptionRecord {
        id: user_id,
        display_name: principal.display_name.unwrap_or_else(|| username.clone()),
        username,
    };
    let matches = search
        .as_deref()
        .is_none_or(|search| user_matches_search(&option.username, &option.display_name, search));
    Ok(Json(UserOptionPageResponse {
        items: (matches && offset == 0)
            .then_some(option)
            .into_iter()
            .collect(),
        total: if matches { 1 } else { 0 },
    }))
}

pub async fn resolve_user_options(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UserOptionsResolveRequest>,
) -> ApiResult<Vec<UserOptionRecord>> {
    let requested_ids = input
        .ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    if principal.is_super_admin() {
        return state
            .store
            .resolve_user_options(&requested_ids)
            .await
            .map(Json)
            .map_err(internal_error);
    }

    let Some(user_id) = principal.user_id else {
        return Err(not_found("current user not found"));
    };
    if !requested_ids.iter().any(|id| id == &user_id) {
        return Ok(Json(Vec::new()));
    }
    state
        .store
        .resolve_user_options(&[user_id])
        .await
        .map(Json)
        .map_err(internal_error)
}

fn validated_user_list_query(query: UserListQuery) -> Result<(Option<String>, i64, u64), String> {
    let limit = query.limit.unwrap_or(DEFAULT_USER_PAGE_LIMIT);
    if !(1..=MAX_USER_PAGE_LIMIT).contains(&limit) {
        return Err(format!("limit must be between 1 and {MAX_USER_PAGE_LIMIT}"));
    }
    let offset = query.offset.unwrap_or(0);
    i64::try_from(offset).map_err(|_| "offset is too large".to_string())?;
    Ok((
        query
            .query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        limit,
        offset,
    ))
}

fn user_matches_search(username: &str, display_name: &str, search: &str) -> bool {
    let search = search.to_lowercase();
    username.to_lowercase().starts_with(&search) || display_name.to_lowercase().starts_with(&search)
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
        password_hash: hash_password_async(&state.password_worker, input.password.as_str())
            .await
            .map_err(bad_request)?,
        credential_version: 0,
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
        let _ = provision_harness_user_public_register(&state, &user).await;
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
    if record.encrypted_provisioning_secret.is_none() {
        return Err(bad_request(
            "harness provisioning secret is unavailable; reprovision before retry",
        ));
    }
    provision_harness_user_public_register_result(&state, &user)
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

pub async fn provision_harness_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<ProvisionHarnessUserRequest>,
) -> ApiResult<UserSummaryRecord> {
    require_super_admin(&principal)?;
    if !state.config.harness_provisioning_enabled {
        return Err(bad_request("harness provisioning is disabled"));
    }

    let Some(mut user) = state
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

    user.password_hash = hash_password_async(&state.password_worker, input.password.as_str())
        .await
        .map_err(bad_request)?;
    user.credential_version = user
        .credential_version
        .checked_add(1)
        .ok_or_else(|| internal_error("credential version overflowed"))?;
    user.updated_at = now_rfc3339();
    state
        .store
        .update_user_credentials_and_revoke_sessions(&user)
        .await
        .map_err(internal_error)?;

    provision_harness_user_public_register_result(&state, &user)
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

pub async fn change_password(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<ChangePasswordRequest>,
) -> ApiStatusResult {
    let user_id = principal
        .user_id
        .as_deref()
        .ok_or_else(|| unauthorized("human user identity is required"))?;
    let Some(mut user) = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("user not found"));
    };
    if !verify_password_async(
        &state.password_worker,
        input.current_password.as_str(),
        user.password_hash.as_str(),
    )
    .await
    .map_err(internal_error)?
    {
        return Err(unauthorized("current password is incorrect"));
    }
    if verify_password_async(
        &state.password_worker,
        input.new_password.as_str(),
        user.password_hash.as_str(),
    )
    .await
    .map_err(internal_error)?
    {
        return Err(bad_request(
            "new password must differ from the current password",
        ));
    }
    update_user_credentials(&state, &mut user, input.new_password.as_str()).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn reset_user_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<ResetUserPasswordRequest>,
) -> ApiResult<UserSummaryRecord> {
    require_super_admin(&principal)?;
    if principal.user_id.as_deref() == Some(id.as_str()) {
        return Err(forbidden(
            "use the authenticated change-password endpoint for your own password",
        ));
    }
    let Some(mut user) = state
        .store
        .find_user_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("user not found"));
    };
    update_user_credentials(&state, &mut user, input.password.as_str()).await?;
    let summary = state
        .store
        .get_user_summary(user.id.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error("updated user summary missing"))?;
    Ok(Json(summary))
}

async fn update_user_credentials(
    state: &AppState,
    user: &mut UserRecord,
    password: &str,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    user.password_hash = hash_password_async(&state.password_worker, password)
        .await
        .map_err(bad_request)?;
    user.credential_version = user
        .credential_version
        .checked_add(1)
        .ok_or_else(|| internal_error("credential version overflowed"))?;
    user.updated_at = now_rfc3339();
    state
        .store
        .update_user_credentials_and_revoke_sessions(user)
        .await
        .map_err(internal_error)
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

#[cfg(test)]
mod list_query_tests {
    use super::{user_matches_search, validated_user_list_query, UserListQuery};

    #[test]
    fn list_query_uses_existing_pagination_bounds() {
        let (search, limit, offset) = validated_user_list_query(UserListQuery::default())
            .expect("default pagination must be valid");
        assert_eq!(search, None);
        assert_eq!(limit, 20);
        assert_eq!(offset, 0);

        assert!(validated_user_list_query(UserListQuery {
            limit: Some(0),
            ..UserListQuery::default()
        })
        .is_err());
        assert!(validated_user_list_query(UserListQuery {
            limit: Some(101),
            ..UserListQuery::default()
        })
        .is_err());
    }

    #[test]
    fn list_query_trims_search_and_matches_prefix_case_insensitively() {
        let (search, _, _) = validated_user_list_query(UserListQuery {
            query: Some("  ALi  ".to_string()),
            ..UserListQuery::default()
        })
        .expect("search query must be valid");
        assert_eq!(search.as_deref(), Some("ALi"));
        assert!(user_matches_search("alice", "Alice Zhang", "ALi"));
        assert!(!user_matches_search("malice", "Malice", "ALi"));
    }
}
