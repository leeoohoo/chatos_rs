// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::{sync_model_config_delete, sync_model_config_upsert};
use crate::models::{
    CreateUserModelConfigRequest, UpdateUserModelConfigRequest, UserModelConfigRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, internal_error, not_found, ApiResult, ApiStatusResult};
use super::access::{ensure_model_access, ensure_owner_user_exists, resolve_target_user_id};
use super::contracts::{ModelConfigGetQuery, UserScopeQuery};
use super::model_values::model_config_public_value;
use super::normalization::{
    model_config_id_for, normalize_api_key_input, normalize_optional_string,
    normalize_provider_input, normalize_thinking_level_input,
};

pub(in crate::api) async fn list_model_configs(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserScopeQuery>,
) -> ApiResult<Vec<serde_json::Value>> {
    let owner_user_id = resolve_target_user_id(&principal, query.user_id.as_deref())?;
    let items = if principal.is_super_admin() && owner_user_id.is_none() {
        state.store.list_user_model_configs(None).await
    } else {
        state
            .store
            .list_user_model_configs(owner_user_id.as_deref())
            .await
    }
    .map_err(internal_error)?;

    Ok(Json(
        items
            .into_iter()
            .filter(|item| !item.model.trim().is_empty())
            .map(|item| model_config_public_value(item, false, None))
            .collect::<Vec<_>>(),
    ))
}

pub(in crate::api) async fn create_model_config(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateUserModelConfigRequest>,
) -> ApiResult<serde_json::Value> {
    let owner_user_id = resolve_target_user_id(&principal, input.owner_user_id.as_deref())?
        .ok_or_else(|| bad_request("owner_user_id is required"))?;
    ensure_owner_user_exists(&state, owner_user_id.as_str()).await?;

    let Some(name) = normalize_optional_string(Some(input.name)) else {
        return Err(bad_request("name is required"));
    };
    let provider = normalize_provider_input(input.provider)?;
    let api_key_present = normalize_api_key_input(input.api_key)?
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let base_url: Option<String> = None;
    let task_thinking_level =
        normalize_thinking_level_input(provider.as_str(), input.task_thinking_level.as_deref())?;
    let task_usage_scenario = normalize_optional_string(input.task_usage_scenario);
    let now = now_rfc3339();
    let Some(model) = normalize_optional_string(input.model) else {
        return Err(bad_request(
            "model is required; use /api/model-providers to save provider credentials and import models",
        ));
    };
    let mut sync_warnings = Vec::new();

    let id = input.id.clone().unwrap_or_else(|| {
        model_config_id_for(
            owner_user_id.as_str(),
            provider.as_str(),
            base_url.as_deref(),
            model.as_str(),
        )
    });
    let existing = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?;
    let record = UserModelConfigRecord {
        id,
        owner_user_id: owner_user_id.clone(),
        name,
        provider: provider.clone(),
        model,
        thinking_level: normalize_thinking_level_input(
            provider.as_str(),
            input.thinking_level.as_deref(),
        )?,
        task_usage_scenario: task_usage_scenario.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|item| item.task_usage_scenario.clone())
        }),
        task_thinking_level: task_thinking_level.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|item| item.task_thinking_level.clone())
        }),
        api_key: None,
        has_api_key: input.has_api_key.unwrap_or(api_key_present),
        base_url: base_url.clone(),
        enabled: input.enabled.unwrap_or(true),
        supports_images: input.supports_images.unwrap_or(false),
        supports_reasoning: input.supports_reasoning.unwrap_or(false),
        supports_responses: input.supports_responses.unwrap_or(false),
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };

    let saved = state
        .store
        .save_user_model_config(&record)
        .await
        .map_err(internal_error)?;
    sync_warnings.extend(sync_model_config_upsert(&state, &saved).await);
    let value = model_config_public_value(saved, false, Some(sync_warnings));
    Ok(Json(value))
}

pub(in crate::api) async fn get_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<ModelConfigGetQuery>,
) -> ApiResult<serde_json::Value> {
    let Some(record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &record)?;
    let include_secret = query.include_secret.unwrap_or(false);
    Ok(Json(model_config_public_value(
        record,
        include_secret,
        None,
    )))
}

pub(in crate::api) async fn update_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelConfigRequest>,
) -> ApiResult<serde_json::Value> {
    let Some(mut record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &record)?;

    if let Some(name) = input.name {
        let Some(name) = normalize_optional_string(Some(name)) else {
            return Err(bad_request("name is required"));
        };
        record.name = name;
    }
    if let Some(provider) = input.provider {
        record.provider = normalize_provider_input(Some(provider))?;
    }
    if let Some(model) = input.model {
        let Some(model) = normalize_optional_string(Some(model)) else {
            return Err(bad_request("model is required"));
        };
        record.model = model;
    }
    if input.clear_api_key.unwrap_or(false) {
        record.has_api_key = false;
    } else if input.has_api_key.is_some() || input.api_key.is_some() {
        let api_key_present = normalize_api_key_input(input.api_key)?
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        record.has_api_key = input.has_api_key.unwrap_or(api_key_present);
    }
    record.api_key = None;
    record.base_url = None;
    let _ = input.base_url;
    if let Some(enabled) = input.enabled {
        record.enabled = enabled;
    }
    if let Some(supports_images) = input.supports_images {
        record.supports_images = supports_images;
    }
    if let Some(supports_reasoning) = input.supports_reasoning {
        record.supports_reasoning = supports_reasoning;
    }
    if let Some(supports_responses) = input.supports_responses {
        record.supports_responses = supports_responses;
    }
    if input.thinking_level.is_some() {
        record.thinking_level = normalize_thinking_level_input(
            record.provider.as_str(),
            input.thinking_level.as_deref(),
        )?;
    }
    if let Some(task_usage_scenario) = input.task_usage_scenario {
        record.task_usage_scenario = normalize_optional_string(Some(task_usage_scenario));
    }
    if input.task_thinking_level.is_some() {
        record.task_thinking_level = normalize_thinking_level_input(
            record.provider.as_str(),
            input.task_thinking_level.as_deref(),
        )?;
    }
    record.updated_at = now_rfc3339();

    let saved = state
        .store
        .save_user_model_config(&record)
        .await
        .map_err(internal_error)?;
    let sync_warnings = if saved.model.trim().is_empty() {
        vec!["model is empty; refresh provider models before it can be used".to_string()]
    } else {
        sync_model_config_upsert(&state, &saved).await
    };
    Ok(Json(model_config_public_value(
        saved,
        false,
        Some(sync_warnings),
    )))
}

pub(in crate::api) async fn delete_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
    let Some(record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &record)?;

    let deleted = state
        .store
        .delete_user_model_config(id.as_str())
        .await
        .map_err(internal_error)?;
    if deleted {
        let _sync_warnings = sync_model_config_delete(&state, id.as_str()).await;
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(not_found("model config not found"))
    }
}
