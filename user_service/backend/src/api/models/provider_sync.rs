// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Json;
use tracing::{info, warn};

use crate::models::{
    UpdateUserModelProviderRequest, UserModelConfigRecord, UserModelProviderRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, internal_error};
use super::normalization::{
    model_config_belongs_to_provider, normalize_api_key_input, normalize_optional_string,
    normalize_prompt_vendor_input, normalize_provider_input, provider_model_config_id_for,
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
    previous_provider_record: Option<&UserModelProviderRecord>,
) -> Result<Vec<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let owner_models = state
        .store
        .list_user_model_configs(Some(provider_record.owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    let now = now_rfc3339();

    for mut model in owner_models {
        let belongs_to_current = model_config_belongs_to_provider(
            model.source_provider_id.as_deref(),
            model.provider.as_str(),
            model.base_url.as_deref(),
            provider_record.id.as_str(),
            provider_record.provider.as_str(),
            provider_record.base_url.as_deref(),
        );
        let belongs_to_previous = previous_provider_record.is_some_and(|previous| {
            model_config_belongs_to_provider(
                model.source_provider_id.as_deref(),
                model.provider.as_str(),
                model.base_url.as_deref(),
                previous.id.as_str(),
                previous.provider.as_str(),
                previous.base_url.as_deref(),
            )
        });
        if (!belongs_to_current && !belongs_to_previous) || model.model.trim().is_empty() {
            continue;
        }

        if !apply_provider_managed_fields(&mut model, provider_record) {
            continue;
        }

        model.updated_at = now.clone();

        state
            .store
            .save_user_model_config(&model)
            .await
            .map_err(internal_error)?;
    }

    Ok(Vec::new())
}

fn apply_provider_managed_fields(
    model: &mut UserModelConfigRecord,
    provider_record: &UserModelProviderRecord,
) -> bool {
    let next_name = imported_model_name(provider_record, model.model.as_str());
    let next_task_enabled = model.task_enabled.unwrap_or(model.enabled);
    let changed = model.source_provider_id.as_deref() != Some(provider_record.id.as_str())
        || model.name != next_name
        || model.provider != provider_record.provider
        || model.prompt_vendor != provider_record.prompt_vendor
        || model.base_url != provider_record.base_url
        || model.api_key != provider_record.api_key
        || model.enabled != provider_record.enabled
        || model.task_enabled != Some(next_task_enabled)
        || model.has_api_key != provider_record.has_api_key
        || model.supports_images != provider_record.supports_images
        || model.supports_reasoning != provider_record.supports_reasoning
        || model.supports_responses != provider_record.supports_responses;
    if !changed {
        return false;
    }

    model.source_provider_id = Some(provider_record.id.clone());
    model.name = next_name;
    model.provider = provider_record.provider.clone();
    model.prompt_vendor = provider_record.prompt_vendor.clone();
    model.base_url = provider_record.base_url.clone();
    model.api_key = provider_record.api_key.clone();
    model.enabled = provider_record.enabled;
    model.task_enabled = Some(next_task_enabled);
    model.has_api_key = provider_record.has_api_key;
    model.supports_images = provider_record.supports_images;
    model.supports_reasoning = provider_record.supports_reasoning;
    model.supports_responses = provider_record.supports_responses;
    true
}

fn imported_model_name(provider_record: &UserModelProviderRecord, model: &str) -> String {
    if provider_record.imported_model_count <= 1 {
        provider_record.name.clone()
    } else {
        format!("{} / {}", provider_record.name, model.trim())
    }
}

fn canonical_existing_model<'a>(
    models: &'a [UserModelConfigRecord],
    model_name: &str,
) -> Option<&'a UserModelConfigRecord> {
    models
        .iter()
        .filter(|candidate| candidate.model.trim() == model_name.trim())
        .min_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        })
}

fn imported_model_enabled(
    _existing: Option<&UserModelConfigRecord>,
    provider_enabled: bool,
) -> bool {
    provider_enabled
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
    provider_record.imported_model_count = imported_count as i64;
    let owner_models = state
        .store
        .list_user_model_configs(Some(provider_record.owner_user_id.as_str()))
        .await
        .map_err(internal_error)?;
    let provider_models = owner_models
        .into_iter()
        .filter(|model| {
            model_config_belongs_to_provider(
                model.source_provider_id.as_deref(),
                model.provider.as_str(),
                model.base_url.as_deref(),
                provider_record.id.as_str(),
                provider_record.provider.as_str(),
                provider_record.base_url.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let mut retained_ids = std::collections::HashSet::new();
    for model in &model_names {
        let existing = canonical_existing_model(provider_models.as_slice(), model.as_str());
        let target_id = existing.map(|item| item.id.clone()).unwrap_or_else(|| {
            provider_model_config_id_for(
                provider_record.owner_user_id.as_str(),
                provider_record.id.as_str(),
                model.as_str(),
            )
        });
        let record = UserModelConfigRecord {
            id: target_id.clone(),
            owner_user_id: provider_record.owner_user_id.clone(),
            source_provider_id: Some(provider_record.id.clone()),
            name: imported_model_name(&provider_record, model.as_str()),
            provider: provider_record.provider.clone(),
            prompt_vendor: provider_record.prompt_vendor.clone(),
            model: model.clone(),
            thinking_level: existing.and_then(|item| item.thinking_level.clone()),
            task_usage_scenario: existing.and_then(|item| item.task_usage_scenario.clone()),
            task_thinking_level: existing.and_then(|item| item.task_thinking_level.clone()),
            temperature: existing.and_then(|item| item.temperature),
            max_output_tokens: existing.and_then(|item| item.max_output_tokens),
            api_key: provider_record.api_key.clone(),
            has_api_key: true,
            base_url: provider_record.base_url.clone(),
            enabled: imported_model_enabled(existing, provider_record.enabled),
            task_enabled: existing
                .and_then(|item| item.task_enabled)
                .or_else(|| existing.map(|item| item.enabled))
                .or(Some(true)),
            supports_images: provider_record.supports_images,
            supports_reasoning: provider_record.supports_reasoning,
            supports_responses: provider_record.supports_responses,
            created_at: existing
                .map(|item| item.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now.clone(),
        };
        state
            .store
            .save_user_model_config(&record)
            .await
            .map_err(internal_error)?;
        retained_ids.insert(target_id);
    }

    let mut stale_deleted_count = 0usize;
    for stale in provider_models {
        if retained_ids.contains(stale.id.as_str()) {
            continue;
        }
        if state
            .store
            .delete_user_model_config(stale.id.as_str())
            .await
            .map_err(internal_error)?
        {
            stale_deleted_count += 1;
        }
    }

    provider_record.last_sync_status = Some("ok".to_string());
    provider_record.last_sync_error = None;
    provider_record.last_synced_at = Some(now_rfc3339());
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

#[cfg(test)]
mod tests {
    use super::{apply_provider_managed_fields, canonical_existing_model, imported_model_enabled};
    use crate::models::{UserModelConfigRecord, UserModelProviderRecord};

    fn model(enabled: bool) -> UserModelConfigRecord {
        UserModelConfigRecord {
            id: "model-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_provider_id: Some("provider-1".to_string()),
            name: "Provider / gpt-5.5".to_string(),
            provider: "gpt".to_string(),
            prompt_vendor: Some("gpt".to_string()),
            model: "gpt-5.5".to_string(),
            thinking_level: None,
            task_usage_scenario: None,
            task_thinking_level: None,
            temperature: None,
            max_output_tokens: None,
            api_key: Some("old-key".to_string()),
            has_api_key: true,
            base_url: Some("https://example.com/v1".to_string()),
            enabled,
            task_enabled: Some(enabled),
            supports_images: false,
            supports_reasoning: false,
            supports_responses: false,
            created_at: "created".to_string(),
            updated_at: "updated".to_string(),
        }
    }

    fn provider(enabled: bool) -> UserModelProviderRecord {
        UserModelProviderRecord {
            id: "provider-1".to_string(),
            owner_user_id: "user-1".to_string(),
            name: "Provider".to_string(),
            provider: "gpt".to_string(),
            prompt_vendor: Some("gpt".to_string()),
            api_key: Some("new-key".to_string()),
            has_api_key: true,
            base_url: Some("https://example.com/v1".to_string()),
            enabled,
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
            last_sync_status: None,
            last_sync_error: None,
            last_synced_at: None,
            imported_model_count: 1,
            created_at: "created".to_string(),
            updated_at: "updated".to_string(),
        }
    }

    #[test]
    fn provider_refresh_preserves_existing_model_enabled_state() {
        let disabled_model = model(false);
        let enabled_model = model(true);

        assert!(imported_model_enabled(Some(&disabled_model), true));
        assert!(!imported_model_enabled(Some(&enabled_model), false));
    }

    #[test]
    fn new_models_inherit_provider_enabled_state() {
        assert!(imported_model_enabled(None, true));
        assert!(!imported_model_enabled(None, false));
    }

    #[test]
    fn provider_settings_sync_does_not_overwrite_model_enabled_state() {
        let mut model = model(false);
        let mut provider = provider(true);
        provider.name = "Renamed Provider".to_string();
        provider.provider = "glm".to_string();
        provider.prompt_vendor = Some("glm".to_string());
        provider.base_url = Some("https://new.example.com/v1".to_string());

        assert!(apply_provider_managed_fields(&mut model, &provider));
        assert!(model.enabled);
        assert_eq!(model.task_enabled, Some(false));
        assert_eq!(model.api_key.as_deref(), Some("new-key"));
        assert_eq!(model.name, "Renamed Provider");
        assert_eq!(model.provider, "glm");
        assert_eq!(model.prompt_vendor.as_deref(), Some("glm"));
        assert_eq!(
            model.base_url.as_deref(),
            Some("https://new.example.com/v1")
        );
        assert!(model.supports_images);
        assert!(model.supports_reasoning);
        assert!(model.supports_responses);
    }

    #[test]
    fn refresh_reuses_oldest_existing_model_and_converges_duplicates() {
        let mut oldest = model(true);
        oldest.id = "original-id".to_string();
        oldest.created_at = "2026-08-01T00:00:00Z".to_string();
        let mut duplicate = oldest.clone();
        duplicate.id = "duplicate-id".to_string();
        duplicate.created_at = "2026-08-02T00:00:00Z".to_string();

        let candidates = vec![duplicate, oldest];
        let selected =
            canonical_existing_model(candidates.as_slice(), "gpt-5.5").expect("canonical model");

        assert_eq!(selected.id, "original-id");
    }
}
