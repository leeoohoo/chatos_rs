// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_model_config::{resolve_chat_model_config, ResolvedChatModelConfig};
use crate::models::ai_model_config::AiModelConfig;
use crate::models::session::Session;
use crate::repositories::ai_model_configs;

use super::{access_token_scope, chatos_sessions, user_service_api_client};

fn normalize_optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn session_selected_model_id(session: &Session) -> Option<String> {
    normalize_optional_id(session.selected_model_id.as_deref())
}

fn configured_user_service_base_url(cfg: &Config) -> Option<String> {
    cfg.user_service_base_url
        .clone()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn from_user_service_model_config(
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
        provider: record.provider,
        model,
        thinking_level: record.thinking_level,
        task_usage_scenario: record.task_usage_scenario,
        task_thinking_level: record.task_thinking_level,
        api_key: record.api_key,
        has_api_key: record.has_api_key,
        base_url: record.base_url,
        enabled: record.enabled,
        supports_images: record.supports_images,
        supports_reasoning: record.supports_reasoning,
        supports_responses: record.supports_responses,
        sync_warnings: record.sync_warnings,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

async fn get_user_service_model_config_by_id(
    cfg: &Config,
    model_id: &str,
    user_id: &str,
) -> Result<AiModelConfig, String> {
    let base_url = configured_user_service_base_url(cfg)
        .ok_or_else(|| "user_service is not configured".to_string())?;
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| "current user access token is required".to_string())?;
    let profile = user_service_api_client::get_model_config(
        base_url.as_str(),
        access_token.as_str(),
        model_id,
        true,
        cfg.user_service_request_timeout_ms,
    )
    .await?;
    if profile.owner_user_id != user_id {
        return Err(format!("forbidden model config access: {model_id}"));
    }
    Ok(from_user_service_model_config(profile))
}

async fn list_user_service_model_configs(
    cfg: &Config,
    user_id: &str,
) -> Result<Vec<AiModelConfig>, String> {
    let base_url = configured_user_service_base_url(cfg)
        .ok_or_else(|| "user_service is not configured".to_string())?;
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| "current user access token is required".to_string())?;
    let items = user_service_api_client::list_model_configs(
        base_url.as_str(),
        access_token.as_str(),
        Some(user_id),
        cfg.user_service_request_timeout_ms,
    )
    .await?;
    Ok(items
        .into_iter()
        .map(from_user_service_model_config)
        .collect())
}

fn pick_default_engine_profile(profiles: Vec<AiModelConfig>) -> Result<AiModelConfig, String> {
    let enabled = profiles
        .into_iter()
        .filter(|item| item.enabled)
        .collect::<Vec<_>>();

    match enabled.len() {
        0 => Err("no enabled model config found".to_string()),
        1 => Ok(enabled[0].clone()),
        _ => Err(
            "multiple enabled model configs found; please provide model_config_id or bind a selected_model_id".to_string(),
        ),
    }
}

pub fn runtime_value_from_engine_profile(profile: &AiModelConfig) -> Value {
    json!({
        "provider": profile.provider,
        "model_name": profile.model,
        "temperature": 0.7,
        "thinking_level": profile.thinking_level,
        "api_key": profile.api_key,
        "base_url": profile.base_url,
        "supports_images": profile.supports_images,
        "supports_reasoning": profile.supports_reasoning,
        "supports_responses": profile.supports_responses,
    })
}

fn merge_safe_request_overrides(base: &mut Value, request_model_cfg: &Value) {
    let Some(base_map) = base.as_object_mut() else {
        return;
    };
    let Some(request_map) = request_model_cfg.as_object() else {
        return;
    };

    for key in [
        "temperature",
        "system_prompt",
        "use_active_system_context",
        "model_name",
        "thinking_level",
    ] {
        if let Some(value) = request_map.get(key) {
            base_map.insert(key.to_string(), value.clone());
        }
    }
}

async fn load_profile_by_id(
    cfg: &Config,
    model_id: &str,
    user_id: Option<&str>,
) -> Result<AiModelConfig, String> {
    if configured_user_service_base_url(cfg).is_some() {
        if let Some(user_id) = user_id {
            return get_user_service_model_config_by_id(cfg, model_id, user_id).await;
        }
    }

    let profile = ai_model_configs::get_ai_model_config_by_id(model_id)
        .await
        .map_err(|err| format!("load model config failed: {err}"))?
        .ok_or_else(|| format!("model config not found: {model_id}"))?;
    if let Some(user_id) = user_id {
        if profile.user_id.as_deref() != Some(user_id) {
            return Err(format!("forbidden model config access: {model_id}"));
        }
    }
    Ok(profile)
}

async fn load_default_profile(
    cfg: &Config,
    user_id: Option<&str>,
) -> Result<AiModelConfig, String> {
    if configured_user_service_base_url(cfg).is_some() {
        if let Some(user_id) = user_id {
            let profiles = list_user_service_model_configs(cfg, user_id).await?;
            return pick_default_engine_profile(profiles);
        }
    }

    let profiles = ai_model_configs::list_ai_model_configs(user_id)
        .await
        .map_err(|err| format!("load model configs failed: {err}"))?;
    pick_default_engine_profile(profiles)
}

pub async fn resolve_model_runtime_for_request(
    requested_model_config_id: Option<&str>,
    request_model_cfg: Option<&Value>,
    session_id: Option<&str>,
    user_id: Option<&str>,
    default_model: &str,
    request_reasoning_enabled: Option<bool>,
    respect_model_flags: bool,
) -> Result<ResolvedChatModelConfig, String> {
    let cfg = Config::try_get()?;

    let explicit_model_id = normalize_optional_id(requested_model_config_id);
    let session = if explicit_model_id.is_none() {
        match session_id
            .and_then(|item| normalize_optional_id(Some(item)).filter(|v| !v.is_empty()))
        {
            Some(valid_session_id) => chatos_sessions::get_session_by_id(valid_session_id.as_str())
                .await
                .map_err(|err| format!("load session failed: {err}"))?,
            None => None,
        }
    } else {
        None
    };
    let resolved_model_id =
        explicit_model_id.or_else(|| session.as_ref().and_then(session_selected_model_id));

    let profile = if let Some(model_id) = resolved_model_id {
        load_profile_by_id(cfg, model_id.as_str(), user_id).await?
    } else {
        load_default_profile(cfg, user_id).await?
    };

    let mut model_cfg = runtime_value_from_engine_profile(&profile);
    if let Some(request_model_cfg) = request_model_cfg {
        merge_safe_request_overrides(&mut model_cfg, request_model_cfg);
    }

    Ok(resolve_chat_model_config(
        &model_cfg,
        default_model,
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        request_reasoning_enabled,
        respect_model_flags,
    ))
}
