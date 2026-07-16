// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Json;
use tracing::{info, warn};

use crate::integrations::{sync_model_config_delete, sync_model_config_upsert};
use crate::models::{
    UpdateUserModelProviderRequest, UserModelConfigRecord, UserModelProviderRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, internal_error};
use super::normalization::{
    model_config_id_for, normalize_api_key_input, normalize_optional_string,
    normalize_prompt_vendor_input, normalize_provider_input, normalized_base_url,
};
use super::provider_fetch::fetch_provider_model_names;

pub(super) fn apply_model_provider_update(
    record: &mut UserModelProviderRecord,
    input: UpdateUserModelProviderRequest,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
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
    Ok(())
}

pub(super) async fn sync_imported_models_from_provider_state(
    state: &AppState,
    provider_record: &UserModelProviderRecord,
) -> Result<Vec<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let owner_models = state
        .store
        .list_user_model_configs(Some(provider_record.owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    let mut sync_warnings = Vec::new();
    let now = now_rfc3339();

    for mut model in owner_models {
        if model.provider.as_str() != provider_record.provider.as_str()
            || normalized_base_url(model.base_url.as_deref())
                != normalized_base_url(provider_record.base_url.as_deref())
            || model.model.trim().is_empty()
        {
            continue;
        }

        let changed = model.api_key != provider_record.api_key
            || model.prompt_vendor != provider_record.prompt_vendor
            || model.has_api_key != provider_record.has_api_key
            || model.enabled != provider_record.enabled
            || model.supports_images != provider_record.supports_images
            || model.supports_reasoning != provider_record.supports_reasoning
            || model.supports_responses != provider_record.supports_responses;
        if !changed {
            continue;
        }

        model.api_key = provider_record.api_key.clone();
        model.prompt_vendor = provider_record.prompt_vendor.clone();
        model.has_api_key = provider_record.has_api_key;
        model.enabled = provider_record.enabled;
        model.supports_images = provider_record.supports_images;
        model.supports_reasoning = provider_record.supports_reasoning;
        model.supports_responses = provider_record.supports_responses;
        model.updated_at = now.clone();

        let saved = state
            .store
            .save_user_model_config(&model)
            .await
            .map_err(internal_error)?;
        sync_warnings.extend(sync_model_config_upsert(state, &saved).await);
    }

    Ok(sync_warnings)
}

pub(super) async fn refresh_provider_models_from_record(
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
    for model in &model_names {
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
            prompt_vendor: provider_record.prompt_vendor.clone(),
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
            temperature: existing.as_ref().and_then(|item| item.temperature),
            max_output_tokens: existing.as_ref().and_then(|item| item.max_output_tokens),
            api_key: provider_record.api_key.clone(),
            has_api_key: true,
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
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let owner_models = state
        .store
        .list_user_model_configs(Some(provider_record.owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    let mut stale_deleted_count = 0usize;
    for stale in owner_models {
        if stale.provider != provider_record.provider
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
        imported_model_count = imported_count,
        stale_deleted_count = stale_deleted_count,
        warning_count = sync_warnings.len(),
        "model_provider.refresh.success"
    );
    Ok((saved_provider, sync_warnings))
}
