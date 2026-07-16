// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::{ensure_provider_access, resolve_target_user_id};
    use crate::auth::CurrentPrincipal;
    use crate::models::UserModelProviderRecord;

    fn principal(user_id: &str) -> CurrentPrincipal {
        CurrentPrincipal {
            sub: user_id.to_string(),
            jti: "test-jti".to_string(),
            exp: usize::MAX,
            principal_type: "user".to_string(),
            user_id: Some(user_id.to_string()),
            username: Some(user_id.to_string()),
            display_name: None,
            role: Some("user".to_string()),
            agent_account_id: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            scopes: Vec::new(),
        }
    }

    fn provider(owner_user_id: &str) -> UserModelProviderRecord {
        UserModelProviderRecord {
            id: "provider-1".to_string(),
            owner_user_id: owner_user_id.to_string(),
            name: "Cloud provider".to_string(),
            provider: "openai".to_string(),
            api_key: Some("secret".to_string()),
            has_api_key: true,
            base_url: Some("https://api.openai.com/v1".to_string()),
            enabled: true,
            supports_images: false,
            supports_reasoning: true,
            supports_responses: true,
            last_sync_status: None,
            last_sync_error: None,
            last_synced_at: None,
            imported_model_count: 0,
            created_at: "2026-07-01T00:00:00Z".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn normal_user_is_scoped_to_own_model_records() {
        let current = principal("user-1");

        assert_eq!(
            resolve_target_user_id(&current, None).expect("own scope should resolve"),
            Some("user-1".to_string()),
        );
        assert!(ensure_provider_access(&current, &provider("user-1")).is_ok());
    }

    #[test]
    fn normal_user_cannot_access_another_users_model_records() {
        let current = principal("user-1");

        let target_error = resolve_target_user_id(&current, Some("user-2"))
            .expect_err("cross-user target should be rejected");
        assert_eq!(target_error.0, StatusCode::FORBIDDEN);

        let provider_error = ensure_provider_access(&current, &provider("user-2"))
            .expect_err("cross-user provider should be rejected");
        assert_eq!(provider_error.0, StatusCode::FORBIDDEN);
    }
}
