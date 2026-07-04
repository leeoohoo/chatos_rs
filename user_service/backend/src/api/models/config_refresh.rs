// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};
use serde_json::json;

use crate::auth::CurrentPrincipal;
use crate::integrations::{sync_model_config_delete, sync_model_config_upsert};
use crate::models::{UpdateUserModelConfigRequest, UserModelConfigRecord};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, internal_error, not_found, ApiResult};
use super::access::ensure_model_access;
use super::model_values::model_config_public_value;
use super::normalization::{
    model_config_id_for, normalize_api_key_input, normalize_optional_string,
    normalize_provider_input, normalized_base_url, provider_display_name_prefix,
};
use super::provider_fetch::fetch_provider_model_names;

pub(in crate::api) async fn refresh_model_config_provider_models(
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
        normalize_api_key_input(Some(api_key))?
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
