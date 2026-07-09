// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::models::ai_model_config::AiModelConfig;
use crate::services::user_service_api_client;
use crate::utils::model_config::{normalize_provider, normalize_thinking_level};

use super::super::AiModelConfigRequest;

fn normalize_provider_input(provider: Option<String>) -> Result<String, String> {
    let raw = provider.unwrap_or_else(|| "gpt".to_string());
    let provider = normalize_provider(&raw);

    match provider.as_str() {
        "gpt" | "deepseek" | "kimi" | "minimax" | "openai_compatible" => Ok(provider),
        _ => Err("provider 仅支持 gpt / deepseek / kimi / minimax / openai_compatible".to_string()),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    level: Option<String>,
) -> Result<Option<String>, String> {
    let level = level
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(level) = level else {
        return Ok(None);
    };

    normalize_thinking_level(provider, Some(level.as_str()))
        .map_err(|_| "思考等级仅支持 none/auto/minimal/low/medium/high/xhigh/max".to_string())
}

fn normalize_optional_input(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(super) fn from_user_service_model_config(
    record: user_service_api_client::UserServiceModelConfigRecord,
) -> AiModelConfig {
    let model = if !record.model_name.trim().is_empty() {
        record.model_name
    } else {
        record.model
    };
    AiModelConfig {
        id: record.id,
        user_id: Some(record.owner_user_id),
        name: record.name,
        provider: normalize_provider(record.provider.as_str()),
        model,
        thinking_level: record.thinking_level,
        task_usage_scenario: record.task_usage_scenario,
        task_thinking_level: record.task_thinking_level,
        api_key: None,
        has_api_key: record.has_api_key,
        base_url: None,
        enabled: record.enabled,
        supports_images: record.supports_images,
        supports_reasoning: record.supports_reasoning,
        supports_responses: record.supports_responses,
        sync_warnings: record.sync_warnings,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

pub(super) fn to_user_service_create_request(
    auth: &AuthUser,
    req: AiModelConfigRequest,
) -> user_service_api_client::CreateUserServiceModelConfigRequest {
    user_service_api_client::CreateUserServiceModelConfigRequest {
        id: req
            .id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        owner_user_id: Some(auth.user_id.clone()),
        name: req.name.unwrap_or_default(),
        provider: req.provider,
        model: normalize_optional_input(req.model),
        thinking_level: normalize_optional_input(req.thinking_level),
        task_usage_scenario: normalize_optional_input(req.task_usage_scenario),
        task_thinking_level: normalize_optional_input(req.task_thinking_level),
        api_key: None,
        base_url: None,
        enabled: req.enabled,
        supports_images: req.supports_images,
        supports_reasoning: req.supports_reasoning,
        supports_responses: req.supports_responses,
    }
}

pub(super) fn to_user_service_update_request(
    req: AiModelConfigRequest,
) -> user_service_api_client::UpdateUserServiceModelConfigRequest {
    user_service_api_client::UpdateUserServiceModelConfigRequest {
        name: req.name,
        provider: req.provider,
        model: req.model,
        thinking_level: req.thinking_level,
        task_usage_scenario: req.task_usage_scenario,
        task_thinking_level: req.task_thinking_level,
        api_key: None,
        clear_api_key: None,
        base_url: None,
        enabled: req.enabled,
        supports_images: req.supports_images,
        supports_reasoning: req.supports_reasoning,
        supports_responses: req.supports_responses,
    }
}

pub(super) fn to_response_value(cfg: &AiModelConfig) -> Value {
    let mut value = json!({
        "id": cfg.id,
        "name": cfg.name,
        "provider": cfg.provider,
        "model": cfg.model,
        "model_name": cfg.model,
        "thinking_level": cfg.thinking_level,
        "task_usage_scenario": cfg.task_usage_scenario,
        "task_thinking_level": cfg.task_thinking_level,
        "has_api_key": cfg.has_api_key,
        "base_url": Value::Null,
        "enabled": cfg.enabled,
        "supports_images": cfg.supports_images,
        "supports_reasoning": cfg.supports_reasoning,
        "supports_responses": cfg.supports_responses,
        "created_at": cfg.created_at,
        "updated_at": cfg.updated_at
    });
    if !cfg.sync_warnings.is_empty() {
        value["sync_warnings"] = json!(cfg.sync_warnings);
    }
    value
}

pub(super) fn to_response_value_with_secret(cfg: &AiModelConfig, include_secret: bool) -> Value {
    let _ = include_secret;
    to_response_value(cfg)
}

pub(super) fn model_settings_response_value(
    settings: user_service_api_client::UserServiceModelSettingsRecord,
) -> Value {
    let mut value = json!({
        "user_id": settings.user_id,
        "memory_summary_model_config_id": settings.memory_summary_model_config_id,
        "memory_summary_thinking_level": settings.memory_summary_thinking_level,
        "project_management_agent_model_config_id": settings.project_management_agent_model_config_id,
        "project_management_agent_thinking_level": settings.project_management_agent_thinking_level,
        "updated_at": settings.updated_at,
    });
    if !settings.sync_warnings.is_empty() {
        value["sync_warnings"] = json!(settings.sync_warnings);
    }
    value
}

pub(super) fn to_user_service_create_provider_request(
    auth: &AuthUser,
    req: AiModelConfigRequest,
) -> user_service_api_client::CreateUserServiceModelProviderRequest {
    user_service_api_client::CreateUserServiceModelProviderRequest {
        id: req
            .id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        owner_user_id: Some(auth.user_id.clone()),
        name: req.name.unwrap_or_default(),
        provider: req.provider,
        api_key: None,
        base_url: None,
        enabled: req.enabled,
        supports_images: req.supports_images,
        supports_reasoning: req.supports_reasoning,
        supports_responses: req.supports_responses,
    }
}

pub(super) fn to_user_service_update_provider_request(
    req: AiModelConfigRequest,
) -> user_service_api_client::UpdateUserServiceModelProviderRequest {
    user_service_api_client::UpdateUserServiceModelProviderRequest {
        name: req.name,
        provider: req.provider,
        api_key: None,
        clear_api_key: None,
        base_url: None,
        enabled: req.enabled,
        supports_images: req.supports_images,
        supports_reasoning: req.supports_reasoning,
        supports_responses: req.supports_responses,
    }
}

pub(super) fn model_provider_response_value(
    provider: user_service_api_client::UserServiceModelProviderRecord,
    include_secret: bool,
) -> Value {
    let value = json!({
        "id": provider.id,
        "name": provider.name,
        "provider": normalize_provider(provider.provider.as_str()),
        "has_api_key": provider.has_api_key,
        "base_url": Value::Null,
        "enabled": provider.enabled,
        "supports_images": provider.supports_images,
        "supports_reasoning": provider.supports_reasoning,
        "supports_responses": provider.supports_responses,
        "last_sync_status": provider.last_sync_status,
        "last_sync_error": provider.last_sync_error,
        "last_synced_at": provider.last_synced_at,
        "imported_model_count": provider.imported_model_count,
        "sync_warnings": provider.sync_warnings,
        "created_at": provider.created_at,
        "updated_at": provider.updated_at,
    });
    let _ = include_secret;
    value
}

pub(super) fn build_model_config(
    user_id: String,
    id: String,
    req: AiModelConfigRequest,
    _existing_api_key: Option<String>,
    _require_api_key: bool,
) -> Result<AiModelConfig, String> {
    let Some(name) = normalize_optional_input(req.name.clone()) else {
        return Err("name 为必填项".to_string());
    };
    let Some(model) = normalize_optional_input(req.model.clone()) else {
        return Err("model 为必填项".to_string());
    };

    let provider = normalize_provider_input(req.provider.clone())?;
    let thinking_level =
        normalize_thinking_level_input(provider.as_str(), req.thinking_level.clone())?;
    Ok(AiModelConfig {
        id,
        user_id: Some(user_id),
        name,
        provider,
        model,
        task_usage_scenario: None,
        task_thinking_level: None,
        base_url: None,
        api_key: None,
        has_api_key: false,
        enabled: req.enabled.unwrap_or(true),
        thinking_level,
        supports_images: req.supports_images.unwrap_or(false),
        supports_reasoning: req.supports_reasoning.unwrap_or(false),
        supports_responses: req.supports_responses.unwrap_or(false),
        sync_warnings: Vec::new(),
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::{build_model_config, to_response_value};
    use crate::api::configs::AiModelConfigRequest;
    use crate::models::ai_model_config::AiModelConfig;

    fn sample_request() -> AiModelConfigRequest {
        AiModelConfigRequest {
            id: None,
            name: Some("Model".to_string()),
            provider: Some("gpt".to_string()),
            model: Some("gpt-4o".to_string()),
            thinking_level: Some("medium".to_string()),
            enabled: Some(true),
            supports_images: Some(true),
            supports_reasoning: Some(true),
            supports_responses: Some(true),
            task_usage_scenario: None,
            task_thinking_level: None,
        }
    }

    #[test]
    fn response_hides_sensitive_runtime_fields() {
        let value = to_response_value(&AiModelConfig {
            id: "cfg_1".to_string(),
            user_id: Some("user_1".to_string()),
            name: "Model".to_string(),
            provider: "gpt".to_string(),
            model: "gpt-4o".to_string(),
            thinking_level: Some("medium".to_string()),
            task_usage_scenario: None,
            task_thinking_level: None,
            api_key: Some("secret".to_string()),
            has_api_key: true,
            base_url: Some("https://api.openai.com/v1".to_string()),
            enabled: true,
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
            sync_warnings: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        });

        assert!(value.get("api_key").is_none());
        assert!(value.get("base_url").is_some_and(|item| item.is_null()));
        assert_eq!(
            value.get("has_api_key").and_then(|item| item.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn update_does_not_store_existing_api_key() {
        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            sample_request(),
            Some("stored-secret".to_string()),
            false,
        )
        .expect("config should build");

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, None);
        assert!(!config.has_api_key);
    }

    #[test]
    fn create_does_not_require_or_store_api_key() {
        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            sample_request(),
            None,
            true,
        )
        .expect("config should build without api key");

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, None);
        assert!(!config.has_api_key);
    }

    #[test]
    fn clear_api_key_removes_stored_secret_on_update() {
        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            sample_request(),
            Some("stored-secret".to_string()),
            false,
        )
        .expect("config should build");

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, None);
        assert!(!config.has_api_key);
    }

    #[test]
    fn accepts_kimi_alias_provider_with_auto_thinking() {
        let mut request = sample_request();
        request.provider = Some("kimik2".to_string());
        request.model = Some("kimi-k2.5".to_string());
        request.thinking_level = Some("auto".to_string());

        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            request,
            None,
            true,
        )
        .expect("config should build");

        assert_eq!(config.provider, "kimi");
        assert_eq!(config.thinking_level.as_deref(), Some("auto"));
    }

    #[test]
    fn accepts_openai_compatible_provider() {
        let mut request = sample_request();
        request.provider = Some("openai-compatible".to_string());
        request.model = Some("custom-model".to_string());
        request.thinking_level = Some("max".to_string());

        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            request,
            None,
            true,
        )
        .expect("config should build");

        assert_eq!(config.provider, "openai_compatible");
        assert_eq!(config.thinking_level.as_deref(), Some("xhigh"));
    }
}
