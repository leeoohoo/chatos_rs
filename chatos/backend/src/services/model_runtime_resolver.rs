// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use tracing::warn;

use crate::config::Config;
use crate::core::ai_model_config::{resolve_chat_model_config, ResolvedChatModelConfig};
use crate::models::ai_model_config::AiModelConfig;
use crate::models::session::Session;
use crate::repositories::{ai_model_configs, session_runtime_settings};

use super::{access_token_scope, chatos_sessions, user_service_api_client};

fn normalize_optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn session_metadata_selected_model_id(session: &Session) -> Option<String> {
    normalize_optional_id(session.selected_model_id.as_deref())
}

fn select_model_id_by_precedence(
    explicit_model_id: Option<String>,
    runtime_model_id: Option<String>,
    metadata_model_id: Option<String>,
) -> Option<String> {
    explicit_model_id.or(runtime_model_id).or(metadata_model_id)
}

async fn session_bound_model_id(
    session_id: &str,
    user_id: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(user_id) = user_id {
        let runtime = session_runtime_settings::get_session_runtime_settings(session_id, user_id)
            .await
            .map_err(|err| format!("load session runtime settings failed: {err}"))?;
        if let Some(model_id) = runtime
            .as_ref()
            .and_then(|settings| normalize_optional_id(settings.selected_model_id.as_deref()))
        {
            return Ok(Some(model_id));
        }
    }

    let session = chatos_sessions::get_session_by_id(session_id)
        .await
        .map_err(|err| format!("load session failed: {err}"))?;
    Ok(session
        .as_ref()
        .and_then(session_metadata_selected_model_id))
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
        prompt_vendor: record.prompt_vendor,
        model,
        thinking_level: record.thinking_level,
        task_usage_scenario: record.task_usage_scenario,
        task_thinking_level: record.task_thinking_level,
        temperature: record.temperature,
        max_output_tokens: record.max_output_tokens,
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

pub(crate) async fn resolve_model_request_max_retries(
    cfg: &Config,
    user_id: Option<&str>,
) -> usize {
    let Some(user_id) = user_id else {
        return chatos_ai_runtime::DEFAULT_MODEL_REQUEST_MAX_RETRIES;
    };
    let Some(base_url) = configured_user_service_base_url(cfg) else {
        return chatos_ai_runtime::DEFAULT_MODEL_REQUEST_MAX_RETRIES;
    };
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return chatos_ai_runtime::DEFAULT_MODEL_REQUEST_MAX_RETRIES;
    };
    match user_service_api_client::get_model_settings(
        base_url.as_str(),
        access_token.as_str(),
        Some(user_id),
        cfg.user_service_request_timeout_ms,
    )
    .await
    {
        Ok(settings) => usize::try_from(settings.model_request_max_retries)
            .unwrap_or(chatos_ai_runtime::DEFAULT_MODEL_REQUEST_MAX_RETRIES),
        Err(err) => {
            warn!(
                user_id,
                error = err.as_str(),
                "load model request retry setting failed; using default"
            );
            chatos_ai_runtime::DEFAULT_MODEL_REQUEST_MAX_RETRIES
        }
    }
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
        "prompt_vendor": profile.prompt_vendor,
        "model_name": profile.model,
        "temperature": profile.temperature.unwrap_or(0.7),
        "thinking_level": profile.thinking_level,
        "api_key": profile.api_key,
        "base_url": profile.base_url,
        "supports_images": profile.supports_images,
        "supports_reasoning": profile.supports_reasoning,
        "supports_responses": profile.supports_responses,
    })
}

fn ensure_profile_api_key_is_usable(profile: &AiModelConfig) -> Result<(), String> {
    let api_key = profile
        .api_key
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if api_key.starts_with("enc:v1:") {
        return Err(format!(
            "model config {} api_key is still encrypted; update the provider credentials or run the secret migration",
            profile.id
        ));
    }
    if profile.has_api_key && api_key.is_empty() {
        return Err(format!(
            "model config {} api_key could not be decrypted; update the provider credentials or run the secret migration",
            profile.id
        ));
    }
    Ok(())
}

fn compatible_replacement_profiles(
    original: &AiModelConfig,
    profiles: Vec<AiModelConfig>,
) -> Vec<AiModelConfig> {
    let original_provider = original.provider.trim();
    let original_model = original.model.trim();
    let original_name = original.name.trim();
    let mut candidates = profiles
        .into_iter()
        .filter(|candidate| {
            candidate.id != original.id
                && candidate.enabled
                && candidate.has_api_key
                && candidate.provider.trim() == original_provider
                && candidate.model.trim() == original_model
                && candidate.name.trim() == original_name
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_has_base_url = left
            .base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        let right_has_base_url = right
            .base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        right_has_base_url
            .cmp(&left_has_base_url)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    candidates
}

async fn recover_compatible_user_service_profile(
    cfg: &Config,
    original: &AiModelConfig,
    user_id: Option<&str>,
) -> Option<AiModelConfig> {
    let user_id = user_id?;
    configured_user_service_base_url(cfg)?;
    let profiles = list_user_service_model_configs(cfg, user_id).await.ok()?;
    for candidate in compatible_replacement_profiles(original, profiles) {
        let Ok(profile) =
            get_user_service_model_config_by_id(cfg, candidate.id.as_str(), user_id).await
        else {
            continue;
        };
        if ensure_profile_api_key_is_usable(&profile).is_ok() {
            return Some(profile);
        }
    }
    None
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
            let profile = pick_default_engine_profile(profiles)?;
            return get_user_service_model_config_by_id(cfg, profile.id.as_str(), user_id).await;
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
    let session_model_id = if explicit_model_id.is_none() {
        match session_id.and_then(|item| normalize_optional_id(Some(item))) {
            Some(valid_session_id) => {
                session_bound_model_id(valid_session_id.as_str(), user_id).await?
            }
            None => None,
        }
    } else {
        None
    };
    let resolved_model_id =
        select_model_id_by_precedence(explicit_model_id, session_model_id, None);

    let profile = if let Some(model_id) = resolved_model_id {
        load_profile_by_id(cfg, model_id.as_str(), user_id).await?
    } else {
        load_default_profile(cfg, user_id).await?
    };

    let profile = match ensure_profile_api_key_is_usable(&profile) {
        Ok(()) => profile,
        Err(original_error) => {
            match recover_compatible_user_service_profile(cfg, &profile, user_id).await {
                Some(replacement) => {
                    warn!(
                        selected_model_config_id = profile.id,
                        replacement_model_config_id = replacement.id,
                        model = replacement.model,
                        provider = replacement.provider,
                        "recovered stale selected model config with compatible refreshed profile"
                    );
                    replacement
                }
                None => return Err(original_error),
            }
        }
    };
    let mut model_cfg = runtime_value_from_engine_profile(&profile);
    model_cfg["model_request_max_retries"] =
        json!(resolve_model_request_max_retries(cfg, user_id).await);
    if let Some(request_model_cfg) = request_model_cfg {
        merge_safe_request_overrides(&mut model_cfg, request_model_cfg);
    }

    let mut resolved = resolve_chat_model_config(
        &model_cfg,
        default_model,
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        request_reasoning_enabled,
        respect_model_flags,
    );
    resolved.model_config_id = Some(profile.id.clone());
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{compatible_replacement_profiles, select_model_id_by_precedence};
    use crate::models::ai_model_config::AiModelConfig;

    fn model_profile(
        id: &str,
        provider: &str,
        model: &str,
        name: &str,
        base_url: Option<&str>,
        has_api_key: bool,
        enabled: bool,
        updated_at: &str,
    ) -> AiModelConfig {
        AiModelConfig {
            id: id.to_string(),
            user_id: Some("user-1".to_string()),
            name: name.to_string(),
            provider: provider.to_string(),
            prompt_vendor: None,
            model: model.to_string(),
            thinking_level: None,
            task_usage_scenario: None,
            task_thinking_level: None,
            temperature: None,
            max_output_tokens: None,
            api_key: None,
            has_api_key,
            base_url: base_url.map(ToOwned::to_owned),
            enabled,
            supports_images: false,
            supports_reasoning: true,
            supports_responses: true,
            sync_warnings: Vec::new(),
            created_at: updated_at.to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn explicit_model_selection_has_highest_precedence() {
        assert_eq!(
            select_model_id_by_precedence(
                Some("explicit".to_string()),
                Some("runtime".to_string()),
                Some("metadata".to_string()),
            ),
            Some("explicit".to_string())
        );
    }

    #[test]
    fn runtime_selection_precedes_session_metadata() {
        assert_eq!(
            select_model_id_by_precedence(
                None,
                Some("runtime".to_string()),
                Some("metadata".to_string()),
            ),
            Some("runtime".to_string())
        );
    }

    #[test]
    fn stale_model_selection_prefers_latest_compatible_refreshed_profile() {
        let original = model_profile(
            "legacy-id",
            "gpt",
            "gpt-5.4",
            "my_api / gpt-5.4",
            None,
            true,
            true,
            "2026-07-10T00:00:00Z",
        );
        let candidates = compatible_replacement_profiles(
            &original,
            vec![
                model_profile(
                    "wrong-provider",
                    "glm",
                    "gpt-5.4",
                    "my_api / gpt-5.4",
                    Some("https://other.example/v1"),
                    true,
                    true,
                    "2026-07-19T00:00:00Z",
                ),
                model_profile(
                    "disabled",
                    "gpt",
                    "gpt-5.4",
                    "my_api / gpt-5.4",
                    Some("https://api.example/v1"),
                    true,
                    false,
                    "2026-07-20T00:00:00Z",
                ),
                model_profile(
                    "older-compatible",
                    "gpt",
                    "gpt-5.4",
                    "my_api / gpt-5.4",
                    Some("https://old.example/v1"),
                    true,
                    true,
                    "2026-07-17T00:00:00Z",
                ),
                model_profile(
                    "latest-compatible",
                    "gpt",
                    "gpt-5.4",
                    "my_api / gpt-5.4",
                    Some("https://new.example/v1"),
                    true,
                    true,
                    "2026-07-18T00:00:00Z",
                ),
            ],
        );

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>(),
            vec!["latest-compatible", "older-compatible"]
        );
    }
}
