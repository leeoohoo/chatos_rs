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
    is_supported_provider, model_config_id_for, normalize_api_key_input, normalize_optional_string,
    normalize_prompt_vendor_input, normalize_provider_input, normalize_thinking_level_input,
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
            .filter(|item| {
                !item.model.trim().is_empty() && is_supported_provider(item.provider.as_str())
            })
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
    let prompt_vendor = normalize_prompt_vendor_input(input.prompt_vendor, provider.as_str())?;
    let api_key = normalize_api_key_input(input.api_key)?;
    let api_key_present = api_key.is_some();
    let base_url = normalize_optional_string(input.base_url);
    let task_thinking_level =
        normalize_thinking_level_input(provider.as_str(), input.task_thinking_level.as_deref())?;
    let task_usage_scenario = normalize_optional_string(input.task_usage_scenario);
    let temperature = validate_temperature(input.temperature)?;
    let max_output_tokens = validate_max_output_tokens(input.max_output_tokens)?;
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
        prompt_vendor,
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
        temperature: temperature.or_else(|| existing.as_ref().and_then(|item| item.temperature)),
        max_output_tokens: max_output_tokens
            .or_else(|| existing.as_ref().and_then(|item| item.max_output_tokens)),
        api_key: api_key.or_else(|| existing.as_ref().and_then(|item| item.api_key.clone())),
        has_api_key: input.has_api_key.unwrap_or_else(|| {
            api_key_present || existing.as_ref().is_some_and(|item| item.has_api_key)
        }),
        base_url: base_url.or_else(|| existing.as_ref().and_then(|item| item.base_url.clone())),
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
    if !is_supported_provider(record.provider.as_str()) {
        return Err(not_found("model config not found"));
    }
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
    if !is_supported_provider(record.provider.as_str()) {
        return Err(not_found("model config not found"));
    }
    ensure_model_access(&principal, &record)?;

    if let Some(name) = input.name {
        let Some(name) = normalize_optional_string(Some(name)) else {
            return Err(bad_request("name is required"));
        };
        record.name = name;
    }
    let provider_changed = input.provider.is_some();
    if let Some(provider) = input.provider {
        record.provider = normalize_provider_input(Some(provider))?;
    }
    if input.prompt_vendor.is_some() || provider_changed {
        let next = normalize_prompt_vendor_input(input.prompt_vendor, record.provider.as_str())?;
        if next.is_some() || record.prompt_vendor.is_none() {
            record.prompt_vendor = next;
        }
    }
    if let Some(model) = input.model {
        let Some(model) = normalize_optional_string(Some(model)) else {
            return Err(bad_request("model is required"));
        };
        record.model = model;
    }
    if input.clear_api_key.unwrap_or(false) {
        record.api_key = None;
        record.has_api_key = false;
    } else if let Some(api_key) = input.api_key {
        record.api_key = normalize_api_key_input(Some(api_key))?;
        record.has_api_key = record.api_key.is_some();
    } else if let Some(has_api_key) = input.has_api_key {
        record.has_api_key = has_api_key && record.api_key.is_some();
    }
    if let Some(base_url) = input.base_url {
        record.base_url = normalize_optional_string(Some(base_url));
    }
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
    if input.clear_temperature.unwrap_or(false) {
        record.temperature = None;
    } else if input.temperature.is_some() {
        record.temperature = validate_temperature(input.temperature)?;
    }
    if input.clear_max_output_tokens.unwrap_or(false) {
        record.max_output_tokens = None;
    } else if input.max_output_tokens.is_some() {
        record.max_output_tokens = validate_max_output_tokens(input.max_output_tokens)?;
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

fn validate_temperature(
    value: Option<f64>,
) -> Result<Option<f64>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match value {
        Some(value) if !value.is_finite() || !(0.0..=2.0).contains(&value) => {
            Err(bad_request("temperature must be between 0 and 2"))
        }
        value => Ok(value),
    }
}

fn validate_max_output_tokens(
    value: Option<i64>,
) -> Result<Option<i64>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match value {
        Some(value) if value <= 0 => Err(bad_request("max_output_tokens must be positive")),
        value => Ok(value),
    }
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
