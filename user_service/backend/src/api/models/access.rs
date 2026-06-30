use axum::Json;

use crate::auth::CurrentPrincipal;
use crate::models::{UserModelConfigRecord, UserModelProviderRecord};
use crate::state::AppState;

use super::super::{bad_request, forbidden, internal_error, not_found};

pub(super) fn resolve_target_user_id(
    principal: &CurrentPrincipal,
    requested_user_id: Option<&str>,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let requested_user_id = requested_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if principal.is_super_admin() {
        return Ok(requested_user_id);
    }
    match (principal.user_id.as_deref(), requested_user_id.as_deref()) {
        (Some(current), Some(requested)) if current != requested => {
            Err(forbidden("cannot access another user's model config"))
        }
        (Some(current), _) => Ok(Some(current.to_string())),
        _ => Err(not_found("current user not found")),
    }
}

pub(super) fn ensure_model_access(
    principal: &CurrentPrincipal,
    record: &UserModelConfigRecord,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if principal.is_super_admin()
        || principal.user_id.as_deref() == Some(record.owner_user_id.as_str())
    {
        Ok(())
    } else {
        Err(forbidden("cannot access another user's model config"))
    }
}

pub(super) fn ensure_provider_access(
    principal: &CurrentPrincipal,
    record: &UserModelProviderRecord,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if principal.is_super_admin()
        || principal.user_id.as_deref() == Some(record.owner_user_id.as_str())
    {
        Ok(())
    } else {
        Err(forbidden("cannot access another user's model provider"))
    }
}

pub(super) async fn ensure_owner_user_exists(
    state: &AppState,
    user_id: &str,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    let Some(user) = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("owner user not found"));
    };
    if !user.enabled {
        return Err(bad_request("owner user is disabled"));
    }
    Ok(())
}
