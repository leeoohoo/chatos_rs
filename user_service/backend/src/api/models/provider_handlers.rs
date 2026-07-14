// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::sync_model_config_delete;
use crate::models::{
    CreateUserModelProviderRequest, UpdateUserModelProviderRequest, UserModelProviderRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, internal_error, not_found, ApiResult, ApiStatusResult};
use super::access::{ensure_owner_user_exists, ensure_provider_access, resolve_target_user_id};
use super::contracts::{ModelConfigGetQuery, UserScopeQuery};
use super::model_values::model_provider_public_value;
use super::normalization::{
    normalize_api_key_input, normalize_optional_string, normalize_provider_input,
    normalized_base_url,
};
use super::provider_sync::{
    apply_model_provider_update, refresh_provider_models_from_record,
    sync_imported_models_from_provider_state,
};

pub(in crate::api) async fn list_model_providers(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserScopeQuery>,
) -> ApiResult<Vec<serde_json::Value>> {
    ensure_super_admin(&principal)?;
    let owner_user_id = resolve_target_user_id(&principal, query.user_id.as_deref())?;
    let items = if principal.is_super_admin() && owner_user_id.is_none() {
        state.store.list_user_model_providers(None).await
    } else {
        state
            .store
            .list_user_model_providers(owner_user_id.as_deref())
            .await
    }
    .map_err(internal_error)?;

    Ok(Json(
        items
            .into_iter()
            .map(|item| model_provider_public_value(item, false, None))
            .collect::<Vec<_>>(),
    ))
}

pub(in crate::api) async fn create_model_provider(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateUserModelProviderRequest>,
) -> ApiResult<serde_json::Value> {
    ensure_super_admin(&principal)?;
    let owner_user_id = resolve_target_user_id(&principal, input.owner_user_id.as_deref())?
        .ok_or_else(|| bad_request("owner_user_id is required"))?;
    ensure_owner_user_exists(&state, owner_user_id.as_str()).await?;

    let Some(name) = normalize_optional_string(Some(input.name)) else {
        return Err(bad_request("name is required"));
    };
    let provider = normalize_provider_input(input.provider)?;
    let api_key = normalize_api_key_input(input.api_key)?;
    if api_key.is_none() {
        return Err(bad_request("api_key is required"));
    }
    let base_url = normalize_optional_string(input.base_url);
    let now = now_rfc3339();
    let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing = state
        .store
        .find_user_model_provider_by_id(id.as_str())
        .await
        .map_err(internal_error)?;
    let record = UserModelProviderRecord {
        id,
        owner_user_id,
        name,
        provider,
        api_key,
        has_api_key: true,
        base_url,
        enabled: input.enabled.unwrap_or(true),
        supports_images: input.supports_images.unwrap_or(false),
        supports_reasoning: input.supports_reasoning.unwrap_or(false),
        supports_responses: input.supports_responses.unwrap_or(false),
        last_sync_status: existing
            .as_ref()
            .and_then(|item| item.last_sync_status.clone()),
        last_sync_error: existing
            .as_ref()
            .and_then(|item| item.last_sync_error.clone()),
        last_synced_at: existing
            .as_ref()
            .and_then(|item| item.last_synced_at.clone()),
        imported_model_count: existing
            .as_ref()
            .map(|item| item.imported_model_count)
            .unwrap_or_default(),
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    let saved = state
        .store
        .save_user_model_provider(&record)
        .await
        .map_err(internal_error)?;
    let (provider, sync_warnings) = refresh_provider_models_from_record(&state, saved).await?;
    Ok(Json(model_provider_public_value(
        provider,
        false,
        Some(sync_warnings),
    )))
}

pub(in crate::api) async fn get_model_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<ModelConfigGetQuery>,
) -> ApiResult<serde_json::Value> {
    ensure_super_admin(&principal)?;
    let Some(record) = state
        .store
        .find_user_model_provider_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model provider not found"));
    };
    ensure_provider_access(&principal, &record)?;
    let include_secret = query.include_secret.unwrap_or(false);
    Ok(Json(model_provider_public_value(
        record,
        include_secret,
        None,
    )))
}

pub(in crate::api) async fn update_model_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelProviderRequest>,
) -> ApiResult<serde_json::Value> {
    ensure_super_admin(&principal)?;
    let Some(mut record) = state
        .store
        .find_user_model_provider_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model provider not found"));
    };
    ensure_provider_access(&principal, &record)?;
    apply_model_provider_update(&mut record, input)?;
    record.updated_at = now_rfc3339();
    let saved = state
        .store
        .save_user_model_provider(&record)
        .await
        .map_err(internal_error)?;
    let sync_warnings = sync_imported_models_from_provider_state(&state, &saved).await?;
    Ok(Json(model_provider_public_value(
        saved,
        false,
        Some(sync_warnings),
    )))
}

pub(in crate::api) async fn refresh_model_provider_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelProviderRequest>,
) -> ApiResult<serde_json::Value> {
    ensure_super_admin(&principal)?;
    let Some(mut record) = state
        .store
        .find_user_model_provider_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model provider not found"));
    };
    ensure_provider_access(&principal, &record)?;
    apply_model_provider_update(&mut record, input)?;
    record.updated_at = now_rfc3339();
    let saved = state
        .store
        .save_user_model_provider(&record)
        .await
        .map_err(internal_error)?;
    let (provider, sync_warnings) = refresh_provider_models_from_record(&state, saved).await?;
    Ok(Json(model_provider_public_value(
        provider,
        false,
        Some(sync_warnings),
    )))
}

pub(in crate::api) async fn delete_model_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
    ensure_super_admin(&principal)?;
    let Some(record) = state
        .store
        .find_user_model_provider_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model provider not found"));
    };
    ensure_provider_access(&principal, &record)?;

    let owner_models = state
        .store
        .list_user_model_configs(Some(record.owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    for model in owner_models {
        if model.provider == record.provider
            && normalized_base_url(model.base_url.as_deref())
                == normalized_base_url(record.base_url.as_deref())
            && !model.model.trim().is_empty()
            && state
                .store
                .delete_user_model_config(model.id.as_str())
                .await
                .map_err(internal_error)?
        {
            let _sync_warnings = sync_model_config_delete(&state, model.id.as_str()).await;
        }
    }

    let deleted = state
        .store
        .delete_user_model_provider(id.as_str())
        .await
        .map_err(internal_error)?;
    if deleted {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(not_found("model provider not found"))
    }
}

fn ensure_super_admin(
    principal: &CurrentPrincipal,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if principal.is_super_admin() {
        Ok(())
    } else {
        Err(super::super::forbidden(
            "model provider management requires super admin access",
        ))
    }
}
