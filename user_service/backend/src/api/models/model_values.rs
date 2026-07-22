// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::{UserModelConfigRecord, UserModelProviderRecord, UserModelSettingsRecord};

pub(super) fn model_config_public_value(
    record: UserModelConfigRecord,
    include_secret: bool,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "id": record.id,
        "owner_user_id": record.owner_user_id,
        "name": record.name,
        "provider": record.provider,
        "prompt_vendor": record.prompt_vendor,
        "model": record.model,
        "model_name": record.model,
        "thinking_level": record.thinking_level,
        "task_usage_scenario": record.task_usage_scenario,
        "task_thinking_level": record.task_thinking_level,
        "temperature": record.temperature,
        "max_output_tokens": record.max_output_tokens,
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

pub(super) fn model_provider_public_value(
    record: UserModelProviderRecord,
    include_secret: bool,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "id": record.id,
        "owner_user_id": record.owner_user_id,
        "name": record.name,
        "provider": record.provider,
        "prompt_vendor": record.prompt_vendor,
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

pub(super) fn model_settings_public_value(
    record: UserModelSettingsRecord,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "user_id": record.user_id,
        "model_request_max_retries": record.model_request_max_retries,
        "memory_summary_model_config_id": record.memory_summary_model_config_id,
        "memory_summary_thinking_level": record.memory_summary_thinking_level,
        "project_management_agent_model_config_id": record.project_management_agent_model_config_id,
        "project_management_agent_thinking_level": record.project_management_agent_thinking_level,
        "environment_initialization_model_config_id": record.environment_initialization_model_config_id,
        "environment_initialization_thinking_level": record.environment_initialization_thinking_level,
        "updated_at": record.updated_at,
    });
    if let Some(sync_warnings) = sync_warnings.filter(|items| !items.is_empty()) {
        value["sync_warnings"] = json!(sync_warnings);
    }
    value
}
