// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Json;
use tracing::info;

use crate::integrations::sync_model_config_upsert;
use crate::models::{UpdateUserModelProviderRequest, UserModelProviderRecord};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, internal_error};
use super::normalization::{
    normalize_api_key_input, normalize_optional_string, normalize_provider_input,
    normalized_base_url,
};

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
    if let Some(provider) = input.provider {
        record.provider = normalize_provider_input(Some(provider))?;
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

        let changed = model.has_api_key != provider_record.has_api_key
            || model.enabled != provider_record.enabled
            || model.supports_images != provider_record.supports_images
            || model.supports_reasoning != provider_record.supports_reasoning
            || model.supports_responses != provider_record.supports_responses;
        if !changed {
            continue;
        }

        model.api_key = provider_record.api_key.clone();
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
    info!(
        provider_id = %provider_record.id,
        owner_user_id = %provider_record.owner_user_id,
        provider = %provider_record.provider,
        "model_provider.refresh.local_connector_managed"
    );
    provider_record.api_key = None;
    provider_record.base_url = None;
    provider_record.last_sync_status = Some("local_connector_managed".to_string());
    provider_record.last_sync_error = None;
    provider_record.last_synced_at = Some(now_rfc3339());
    provider_record.updated_at = now_rfc3339();
    let saved_provider = state
        .store
        .save_user_model_provider(&provider_record)
        .await
        .map_err(internal_error)?;
    Ok((
        saved_provider,
        vec!["model provider refresh is managed by Local Connector client".to_string()],
    ))
}
