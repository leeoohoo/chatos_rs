use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};

use crate::auth::CurrentPrincipal;
use crate::integrations::{
    sync_model_config_delete, sync_model_config_upsert, sync_model_settings,
};
use crate::models::{
    CreateUserModelConfigRequest, CreateUserModelProviderRequest, UpdateUserModelConfigRequest,
    UpdateUserModelProviderRequest, UpdateUserModelSettingsRequest, UserModelConfigRecord,
    UserModelProviderRecord, UserModelSettingsRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, forbidden, internal_error, not_found, ApiResult, ApiStatusResult};

#[derive(Debug, Default, Deserialize)]
pub struct UserScopeQuery {
    user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ModelConfigGetQuery {
    include_secret: Option<bool>,
}

pub async fn list_model_configs(
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

pub async fn list_model_providers(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserScopeQuery>,
) -> ApiResult<Vec<serde_json::Value>> {
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

pub async fn create_model_provider(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateUserModelProviderRequest>,
) -> ApiResult<serde_json::Value> {
    let owner_user_id = resolve_target_user_id(&principal, input.owner_user_id.as_deref())?
        .ok_or_else(|| bad_request("owner_user_id is required"))?;
    ensure_owner_user_exists(&state, owner_user_id.as_str()).await?;

    let Some(name) = normalize_optional_string(Some(input.name)) else {
        return Err(bad_request("name is required"));
    };
    let provider = normalize_provider_input(input.provider)?;
    let api_key = normalize_optional_string(input.api_key);
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
        has_api_key: false,
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

pub async fn get_model_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<ModelConfigGetQuery>,
) -> ApiResult<serde_json::Value> {
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

pub async fn update_model_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelProviderRequest>,
) -> ApiResult<serde_json::Value> {
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
    Ok(Json(model_provider_public_value(saved, false, None)))
}

pub async fn refresh_model_provider_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelProviderRequest>,
) -> ApiResult<serde_json::Value> {
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

pub async fn delete_model_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
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

pub async fn create_model_config(
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
    let api_key = normalize_optional_string(input.api_key);
    if api_key.is_none() {
        return Err(bad_request("api_key is required"));
    }
    let base_url = normalize_optional_string(input.base_url);
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
        task_usage_scenario: existing
            .as_ref()
            .and_then(|item| item.task_usage_scenario.clone())
            .or_else(|| task_usage_scenario.clone()),
        task_thinking_level: existing
            .as_ref()
            .and_then(|item| item.task_thinking_level.clone())
            .or_else(|| task_thinking_level.clone()),
        api_key: api_key.clone(),
        has_api_key: false,
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

pub async fn get_model_config(
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

pub async fn update_model_config(
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
        record.api_key = None;
    } else if let Some(api_key) = input.api_key {
        record.api_key = normalize_optional_string(Some(api_key));
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

pub async fn refresh_model_config_provider_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelConfigRequest>,
) -> ApiResult<serde_json::Value> {
    let Some(existing_record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &existing_record)?;

    let owner_user_id = existing_record.owner_user_id.clone();
    let name = match input.name {
        Some(name) => {
            normalize_optional_string(Some(name)).ok_or_else(|| bad_request("name is required"))?
        }
        None => existing_record.name.clone(),
    };
    let display_name_prefix =
        provider_display_name_prefix(name.as_str(), existing_record.model.as_str());
    let provider = match input.provider {
        Some(provider) => normalize_provider_input(Some(provider))?,
        None => existing_record.provider.clone(),
    };
    let base_url = match input.base_url {
        Some(base_url) => normalize_optional_string(Some(base_url)),
        None => existing_record.base_url.clone(),
    };
    let api_key = if input.clear_api_key.unwrap_or(false) {
        None
    } else if let Some(api_key) = input.api_key {
        normalize_optional_string(Some(api_key))
    } else {
        existing_record.api_key.clone()
    };
    let Some(api_key_for_fetch) = api_key.clone() else {
        return Err(bad_request("api_key is required"));
    };
    let now = now_rfc3339();
    let supports_images = input
        .supports_images
        .unwrap_or(existing_record.supports_images);
    let supports_reasoning = input
        .supports_reasoning
        .unwrap_or(existing_record.supports_reasoning);
    let supports_responses = input
        .supports_responses
        .unwrap_or(existing_record.supports_responses);
    let enabled = input.enabled.unwrap_or(existing_record.enabled);

    let source_record = UserModelConfigRecord {
        id: existing_record.id.clone(),
        owner_user_id: owner_user_id.clone(),
        name: if existing_record.model.trim().is_empty() {
            display_name_prefix.clone()
        } else {
            name.clone()
        },
        provider: provider.clone(),
        model: existing_record.model.clone(),
        thinking_level: existing_record.thinking_level.clone(),
        task_usage_scenario: existing_record.task_usage_scenario.clone(),
        task_thinking_level: existing_record.task_thinking_level.clone(),
        api_key: api_key.clone(),
        has_api_key: false,
        base_url: base_url.clone(),
        enabled,
        supports_images,
        supports_reasoning,
        supports_responses,
        created_at: existing_record.created_at.clone(),
        updated_at: now.clone(),
    };
    let saved_source = state
        .store
        .save_user_model_config(&source_record)
        .await
        .map_err(internal_error)?;
    let mut sync_warnings = if saved_source.model.trim().is_empty() {
        Vec::new()
    } else {
        sync_model_config_upsert(&state, &saved_source).await
    };

    let model_names = match fetch_provider_model_names(
        provider.as_str(),
        base_url.as_deref(),
        api_key_for_fetch.as_str(),
        state.config.downstream_request_timeout_ms,
    )
    .await
    {
        Ok(model_names) => model_names,
        Err(err) => {
            sync_warnings.push(format!("fetch provider models failed: {err}"));
            let mut value = model_config_public_value(saved_source, false, Some(sync_warnings));
            value["imported_model_count"] = json!(0);
            value["provider_config_saved"] = json!(true);
            return Ok(Json(value));
        }
    };
    if model_names.is_empty() {
        sync_warnings.push("provider returned no models".to_string());
        let mut value = model_config_public_value(saved_source, false, Some(sync_warnings));
        value["imported_model_count"] = json!(0);
        value["provider_config_saved"] = json!(true);
        return Ok(Json(value));
    }

    let imported_count = model_names.len();
    let mut saved_items = Vec::new();

    for model in model_names.iter() {
        let target_id = model_config_id_for(
            owner_user_id.as_str(),
            provider.as_str(),
            base_url.as_deref(),
            model.as_str(),
        );
        let target_existing = if target_id == existing_record.id {
            Some(saved_source.clone())
        } else {
            state
                .store
                .find_user_model_config_by_id(target_id.as_str())
                .await
                .map_err(internal_error)?
        };
        let display_name = if imported_count == 1 {
            display_name_prefix.clone()
        } else {
            format!("{display_name_prefix} / {model}")
        };
        let record = UserModelConfigRecord {
            id: target_id,
            owner_user_id: owner_user_id.clone(),
            name: display_name,
            provider: provider.clone(),
            model: model.clone(),
            thinking_level: None,
            task_usage_scenario: target_existing
                .as_ref()
                .and_then(|item| item.task_usage_scenario.clone()),
            task_thinking_level: target_existing
                .as_ref()
                .and_then(|item| item.task_thinking_level.clone()),
            api_key: api_key.clone(),
            has_api_key: false,
            base_url: base_url.clone(),
            enabled,
            supports_images,
            supports_reasoning,
            supports_responses,
            created_at: target_existing
                .as_ref()
                .map(|item| item.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now.clone(),
        };

        let saved = state
            .store
            .save_user_model_config(&record)
            .await
            .map_err(internal_error)?;
        sync_warnings.extend(sync_model_config_upsert(&state, &saved).await);
        saved_items.push(saved);
    }

    let all_owner_models = state
        .store
        .list_user_model_configs(Some(owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    let imported = model_names
        .iter()
        .map(|item| item.as_str())
        .collect::<std::collections::HashSet<_>>();
    for stale in all_owner_models {
        if stale.provider.as_str() != provider.as_str()
            || normalized_base_url(stale.base_url.as_deref())
                != normalized_base_url(base_url.as_deref())
        {
            continue;
        }
        if stale.model.trim().is_empty() {
            continue;
        }
        if imported.contains(stale.model.trim()) {
            continue;
        }
        if state
            .store
            .delete_user_model_config(stale.id.as_str())
            .await
            .map_err(internal_error)?
        {
            sync_warnings.extend(sync_model_config_delete(&state, stale.id.as_str()).await);
        }
    }

    let Some(first_saved) = saved_items.into_iter().next() else {
        return Err(bad_request("provider returned no models"));
    };
    let mut value = model_config_public_value(first_saved, false, Some(sync_warnings));
    value["imported_model_count"] = json!(imported_count);
    Ok(Json(value))
}

pub async fn delete_model_config(
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

pub async fn get_model_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserScopeQuery>,
) -> ApiResult<serde_json::Value> {
    let user_id = resolve_target_user_id(&principal, query.user_id.as_deref())?
        .ok_or_else(|| bad_request("user_id is required"))?;
    ensure_owner_user_exists(&state, user_id.as_str()).await?;

    let settings = state
        .store
        .get_user_model_settings(user_id.as_str())
        .await
        .map_err(internal_error)?
        .unwrap_or(UserModelSettingsRecord {
            user_id: user_id.clone(),
            memory_summary_model_config_id: None,
            memory_summary_thinking_level: None,
            updated_at: now_rfc3339(),
        });

    Ok(Json(model_settings_public_value(settings, None)))
}

pub async fn put_model_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelSettingsRequest>,
) -> ApiResult<serde_json::Value> {
    let user_id = resolve_target_user_id(&principal, input.user_id.as_deref())?
        .ok_or_else(|| bad_request("user_id is required"))?;
    ensure_owner_user_exists(&state, user_id.as_str()).await?;

    let mut memory_summary_provider = "gpt".to_string();
    if let Some(model_config_id) = input
        .memory_summary_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some(model_config) = state
            .store
            .find_user_model_config_by_id(model_config_id)
            .await
            .map_err(internal_error)?
        else {
            return Err(bad_request("memory_summary_model_config_id does not exist"));
        };
        if model_config.owner_user_id != user_id {
            return Err(forbidden(
                "memory_summary_model_config_id does not belong to the target user",
            ));
        }
        if model_config.model.trim().is_empty() {
            return Err(bad_request(
                "memory_summary_model_config_id requires a concrete model name",
            ));
        }
        memory_summary_provider = model_config.provider;
    }

    let settings = UserModelSettingsRecord {
        user_id,
        memory_summary_model_config_id: normalize_optional_string(
            input.memory_summary_model_config_id,
        ),
        memory_summary_thinking_level: normalize_thinking_level_input(
            memory_summary_provider.as_str(),
            input.memory_summary_thinking_level.as_deref(),
        )?,
        updated_at: now_rfc3339(),
    };
    let saved = state
        .store
        .save_user_model_settings(&settings)
        .await
        .map_err(internal_error)?;
    let sync_warnings = sync_model_settings(&state, &saved).await;
    Ok(Json(model_settings_public_value(
        saved,
        Some(sync_warnings),
    )))
}

fn resolve_target_user_id(
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

fn ensure_model_access(
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

fn ensure_provider_access(
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

fn apply_model_provider_update(
    record: &mut UserModelProviderRecord,
    input: UpdateUserModelProviderRequest,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if let Some(name) = input.name {
        let Some(name) = normalize_optional_string(Some(name)) else {
            return Err(bad_request("name is required"));
        };
        record.name = name;
    }
    if let Some(provider) = input.provider {
        record.provider = normalize_provider_input(Some(provider))?;
    }
    if input.clear_api_key.unwrap_or(false) {
        record.api_key = None;
    } else if let Some(api_key) = input.api_key {
        record.api_key = normalize_optional_string(Some(api_key));
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
    Ok(())
}

async fn refresh_provider_models_from_record(
    state: &AppState,
    mut provider_record: UserModelProviderRecord,
) -> Result<(UserModelProviderRecord, Vec<String>), (axum::http::StatusCode, Json<serde_json::Value>)>
{
    let mut sync_warnings = Vec::new();
    info!(
        provider_id = %provider_record.id,
        owner_user_id = %provider_record.owner_user_id,
        provider = %provider_record.provider,
        base_url = %provider_record.base_url.as_deref().unwrap_or(""),
        "model_provider.refresh.start"
    );
    let Some(api_key) = provider_record
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    else {
        let message = "api_key is required".to_string();
        warn!(
            provider_id = %provider_record.id,
            owner_user_id = %provider_record.owner_user_id,
            provider = %provider_record.provider,
            "model_provider.refresh.missing_api_key"
        );
        sync_warnings.push(message.clone());
        provider_record.last_sync_status = Some("error".to_string());
        provider_record.last_sync_error = Some(message);
        provider_record.last_synced_at = Some(now_rfc3339());
        provider_record.updated_at = now_rfc3339();
        let saved = state
            .store
            .save_user_model_provider(&provider_record)
            .await
            .map_err(internal_error)?;
        return Ok((saved, sync_warnings));
    };

    let model_names = match fetch_provider_model_names(
        provider_record.provider.as_str(),
        provider_record.base_url.as_deref(),
        api_key.as_str(),
        state.config.downstream_request_timeout_ms,
    )
    .await
    {
        Ok(model_names) => model_names,
        Err(err) => {
            let message = format!("fetch provider models failed: {err}");
            warn!(
                provider_id = %provider_record.id,
                owner_user_id = %provider_record.owner_user_id,
                provider = %provider_record.provider,
                base_url = %provider_record.base_url.as_deref().unwrap_or(""),
                error = %err,
                "model_provider.refresh.fetch_failed"
            );
            sync_warnings.push(message.clone());
            provider_record.last_sync_status = Some("error".to_string());
            provider_record.last_sync_error = Some(message);
            provider_record.last_synced_at = Some(now_rfc3339());
            provider_record.updated_at = now_rfc3339();
            let saved = state
                .store
                .save_user_model_provider(&provider_record)
                .await
                .map_err(internal_error)?;
            return Ok((saved, sync_warnings));
        }
    };

    if model_names.is_empty() {
        let message = "provider returned no models".to_string();
        warn!(
            provider_id = %provider_record.id,
            owner_user_id = %provider_record.owner_user_id,
            provider = %provider_record.provider,
            base_url = %provider_record.base_url.as_deref().unwrap_or(""),
            "model_provider.refresh.empty_models"
        );
        sync_warnings.push(message.clone());
        provider_record.last_sync_status = Some("empty".to_string());
        provider_record.last_sync_error = Some(message);
        provider_record.last_synced_at = Some(now_rfc3339());
        provider_record.imported_model_count = 0;
        provider_record.updated_at = now_rfc3339();
        let saved = state
            .store
            .save_user_model_provider(&provider_record)
            .await
            .map_err(internal_error)?;
        return Ok((saved, sync_warnings));
    }

    let now = now_rfc3339();
    let imported_count = model_names.len();
    for model in model_names.iter() {
        let target_id = model_config_id_for(
            provider_record.owner_user_id.as_str(),
            provider_record.provider.as_str(),
            provider_record.base_url.as_deref(),
            model.as_str(),
        );
        let existing = state
            .store
            .find_user_model_config_by_id(target_id.as_str())
            .await
            .map_err(internal_error)?;
        let record = UserModelConfigRecord {
            id: target_id,
            owner_user_id: provider_record.owner_user_id.clone(),
            name: if imported_count == 1 {
                provider_record.name.clone()
            } else {
                format!("{} / {}", provider_record.name, model)
            },
            provider: provider_record.provider.clone(),
            model: model.clone(),
            thinking_level: existing
                .as_ref()
                .and_then(|item| item.thinking_level.clone()),
            task_usage_scenario: existing
                .as_ref()
                .and_then(|item| item.task_usage_scenario.clone()),
            task_thinking_level: existing
                .as_ref()
                .and_then(|item| item.task_thinking_level.clone()),
            api_key: provider_record.api_key.clone(),
            has_api_key: false,
            base_url: provider_record.base_url.clone(),
            enabled: provider_record.enabled,
            supports_images: provider_record.supports_images,
            supports_reasoning: provider_record.supports_reasoning,
            supports_responses: provider_record.supports_responses,
            created_at: existing
                .as_ref()
                .map(|item| item.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now.clone(),
        };
        let saved = state
            .store
            .save_user_model_config(&record)
            .await
            .map_err(internal_error)?;
        sync_warnings.extend(sync_model_config_upsert(state, &saved).await);
    }

    let imported = model_names
        .iter()
        .map(|item| item.as_str())
        .collect::<std::collections::HashSet<_>>();
    let owner_models = state
        .store
        .list_user_model_configs(Some(provider_record.owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    let mut stale_deleted_count = 0usize;
    for stale in owner_models {
        if stale.provider.as_str() != provider_record.provider.as_str()
            || normalized_base_url(stale.base_url.as_deref())
                != normalized_base_url(provider_record.base_url.as_deref())
            || stale.model.trim().is_empty()
            || imported.contains(stale.model.trim())
        {
            continue;
        }
        if state
            .store
            .delete_user_model_config(stale.id.as_str())
            .await
            .map_err(internal_error)?
        {
            sync_warnings.extend(sync_model_config_delete(state, stale.id.as_str()).await);
            stale_deleted_count += 1;
        }
    }

    provider_record.last_sync_status = Some("ok".to_string());
    provider_record.last_sync_error = None;
    provider_record.last_synced_at = Some(now_rfc3339());
    provider_record.imported_model_count = imported_count as i64;
    provider_record.updated_at = now_rfc3339();
    let saved_provider = state
        .store
        .save_user_model_provider(&provider_record)
        .await
        .map_err(internal_error)?;
    info!(
        provider_id = %saved_provider.id,
        owner_user_id = %saved_provider.owner_user_id,
        provider = %saved_provider.provider,
        base_url = %saved_provider.base_url.as_deref().unwrap_or(""),
        imported_model_count = imported_count,
        stale_deleted_count = stale_deleted_count,
        warning_count = sync_warnings.len(),
        "model_provider.refresh.success"
    );
    Ok((saved_provider, sync_warnings))
}

async fn ensure_owner_user_exists(
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

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn model_config_id_for(
    owner_user_id: &str,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(owner_user_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(provider.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(base_url.unwrap_or_default().trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(model.trim().as_bytes());
    let digest = hasher.finalize();
    format!("model_{}", hex_prefix(&digest, 32))
}

fn hex_prefix(bytes: &[u8], max_chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
        if out.len() >= max_chars {
            out.truncate(max_chars);
            break;
        }
    }
    out
}

async fn fetch_provider_model_names(
    provider: &str,
    base_url: Option<&str>,
    api_key: &str,
    timeout_ms: i64,
) -> Result<Vec<String>, String> {
    let base_url = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url_for_provider(provider))
        .trim_end_matches('/')
        .to_string();
    let endpoint = format!("{base_url}/models");
    let started_at = std::time::Instant::now();
    info!(
        provider = %provider,
        base_url = %base_url,
        endpoint = %endpoint,
        timeout_ms = timeout_ms.max(300),
        "provider_models.fetch.start"
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64))
        .build()
        .map_err(|err| {
            error!(
                provider = %provider,
                base_url = %base_url,
                endpoint = %endpoint,
                error = %err,
                "provider_models.fetch.client_build_failed"
            );
            err.to_string()
        })?;
    let mut request = client.request(Method::GET, endpoint);
    let api_key = api_key.trim();
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.map_err(|err| {
        warn!(
            provider = %provider,
            base_url = %base_url,
            elapsed_ms = started_at.elapsed().as_millis(),
            error = %err,
            "provider_models.fetch.request_failed"
        );
        err.to_string()
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        warn!(
            provider = %provider,
            base_url = %base_url,
            status = status.as_u16(),
            elapsed_ms = started_at.elapsed().as_millis(),
            body_preview = %log_preview(body.as_str(), 800),
            "provider_models.fetch.http_error"
        );
        return Err(format!(
            "provider models request failed: {} {}",
            status.as_u16(),
            body.trim()
        ));
    }
    let payload: Value = serde_json::from_str(body.as_str()).map_err(|err| {
        warn!(
            provider = %provider,
            base_url = %base_url,
            status = status.as_u16(),
            elapsed_ms = started_at.elapsed().as_millis(),
            body_preview = %log_preview(body.as_str(), 800),
            error = %err,
            "provider_models.fetch.parse_failed"
        );
        err.to_string()
    })?;
    let model_names = extract_model_names(&payload);
    info!(
        provider = %provider,
        base_url = %base_url,
        status = status.as_u16(),
        elapsed_ms = started_at.elapsed().as_millis(),
        model_count = model_names.len(),
        "provider_models.fetch.success"
    );
    Ok(model_names)
}

fn log_preview(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let mut out = String::new();
    for (index, ch) in trimmed.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn extract_model_names(payload: &Value) -> Vec<String> {
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.as_array());
    let mut out = Vec::new();
    if let Some(items) = items {
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(id) = id {
                let id = id.to_string();
                if !out.iter().any(|existing| existing == &id) {
                    out.push(id);
                }
            }
        }
    }
    out
}

fn default_base_url_for_provider(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com",
        "kimi" => "https://api.moonshot.ai/v1",
        "minimax" => "https://api.minimax.chat/v1",
        _ => "https://api.openai.com/v1",
    }
}

fn normalized_base_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

fn provider_display_name_prefix(name: &str, model: &str) -> String {
    let name = name.trim();
    let model = model.trim();
    if !model.is_empty() {
        let suffix = format!(" / {model}");
        if let Some(prefix) = name.strip_suffix(suffix.as_str()) {
            let prefix = prefix.trim();
            if !prefix.is_empty() {
                return prefix.to_string();
            }
        }
    }
    name.to_string()
}

fn normalize_provider_input(
    provider: Option<String>,
) -> Result<String, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let provider = provider
        .unwrap_or_else(|| "gpt".to_string())
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");
    match provider.as_str() {
        "openai" | "gpt" => Ok("gpt".to_string()),
        "deepseek" => Ok("deepseek".to_string()),
        "kimi" | "kimik2" | "moonshot" => Ok("kimi".to_string()),
        "minimax" => Ok("minimax".to_string()),
        "openai_compatible" => Ok("openai_compatible".to_string()),
        _ => Err(bad_request(
            "provider only supports gpt / deepseek / kimi / minimax / openai_compatible",
        )),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    value: Option<&str>,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let provider = match provider
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "openai" | "gpt" => "gpt".to_string(),
        "kimik2" | "kimi" | "moonshot" => "kimi".to_string(),
        "openai_compatible" | "compatible" => "openai_compatible".to_string(),
        other => other.to_string(),
    };
    let Some(level) = value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
    else {
        return Ok(None);
    };
    let normalized = match level.to_ascii_lowercase().as_str() {
        "none" | "off" | "disabled" => "none",
        "auto" => "auto",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" | "max" => {
            if provider == "deepseek" {
                "max"
            } else {
                "xhigh"
            }
        }
        _ => {
            return Err(bad_request(
                "thinking_level only supports none/auto/minimal/low/medium/high/xhigh/max",
            ))
        }
    };
    let allowed = match provider.as_str() {
        "gpt" => ["none", "minimal", "low", "medium", "high", "xhigh"].as_slice(),
        "deepseek" => ["none", "low", "medium", "high", "max"].as_slice(),
        "kimi" => ["none", "auto", "low", "medium", "high", "xhigh"].as_slice(),
        _ => ["none", "low", "medium", "high", "xhigh"].as_slice(),
    };
    if provider == "openai_compatible" && normalized == "minimal" {
        return Ok(Some("low".to_string()));
    }
    if !allowed.contains(&normalized) {
        return Err(bad_request(
            "thinking_level is not supported by the selected provider",
        ));
    }
    Ok(Some(normalized.to_string()))
}

fn model_config_public_value(
    record: UserModelConfigRecord,
    include_secret: bool,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "id": record.id,
        "owner_user_id": record.owner_user_id,
        "name": record.name,
        "provider": record.provider,
        "model": record.model,
        "model_name": record.model,
        "thinking_level": record.thinking_level,
        "task_usage_scenario": record.task_usage_scenario,
        "task_thinking_level": record.task_thinking_level,
        "has_api_key": record.has_api_key
            || record
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
        "base_url": record.base_url,
        "enabled": record.enabled,
        "supports_images": record.supports_images,
        "supports_reasoning": record.supports_reasoning,
        "supports_responses": record.supports_responses,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    });
    if include_secret {
        value["api_key"] = Value::String(record.api_key.unwrap_or_default());
    }
    if let Some(sync_warnings) = sync_warnings.filter(|items| !items.is_empty()) {
        value["sync_warnings"] = json!(sync_warnings);
    }
    value
}

fn model_provider_public_value(
    record: UserModelProviderRecord,
    include_secret: bool,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "id": record.id,
        "owner_user_id": record.owner_user_id,
        "name": record.name,
        "provider": record.provider,
        "has_api_key": record.has_api_key
            || record
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
        "base_url": record.base_url,
        "enabled": record.enabled,
        "supports_images": record.supports_images,
        "supports_reasoning": record.supports_reasoning,
        "supports_responses": record.supports_responses,
        "last_sync_status": record.last_sync_status,
        "last_sync_error": record.last_sync_error,
        "last_synced_at": record.last_synced_at,
        "imported_model_count": record.imported_model_count,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    });
    if include_secret {
        value["api_key"] = Value::String(record.api_key.unwrap_or_default());
    }
    if let Some(sync_warnings) = sync_warnings.filter(|items| !items.is_empty()) {
        value["sync_warnings"] = json!(sync_warnings);
    }
    value
}

fn model_settings_public_value(
    record: UserModelSettingsRecord,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "user_id": record.user_id,
        "memory_summary_model_config_id": record.memory_summary_model_config_id,
        "memory_summary_thinking_level": record.memory_summary_thinking_level,
        "updated_at": record.updated_at,
    });
    if let Some(sync_warnings) = sync_warnings.filter(|items| !items.is_empty()) {
        value["sync_warnings"] = json!(sync_warnings);
    }
    value
}
