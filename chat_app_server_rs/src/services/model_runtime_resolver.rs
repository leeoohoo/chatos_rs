use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_model_config::{resolve_chat_model_config, ResolvedChatModelConfig};
use crate::models::session::Session;
use crate::repositories::ai_model_configs;

use super::chatos_sessions;

fn normalize_optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn session_selected_model_id(session: &Session) -> Option<String> {
    normalize_optional_id(session.selected_model_id.as_deref())
}

fn pick_default_engine_profile(
    profiles: Vec<crate::models::ai_model_config::AiModelConfig>,
) -> Result<crate::models::ai_model_config::AiModelConfig, String> {
    let enabled = profiles
        .into_iter()
        .filter(|item| item.enabled)
        .collect::<Vec<_>>();

    match enabled.len() {
        0 => Err("未找到启用的模型，请先在 chatos 中启用至少一个模型".to_string()),
        1 => Ok(enabled[0].clone()),
        _ => Err(
            "检测到多个启用的模型，请显式传入 model_config_id，或先为当前会话绑定 selected_model_id"
                .to_string(),
        ),
    }
}

pub fn runtime_value_from_engine_profile(
    profile: &crate::models::ai_model_config::AiModelConfig,
) -> Value {
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

    for key in ["temperature", "system_prompt", "use_active_system_context"] {
        if let Some(value) = request_map.get(key) {
            base_map.insert(key.to_string(), value.clone());
        }
    }
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
                .map_err(|err| format!("读取会话失败: {err}"))?,
            None => None,
        }
    } else {
        None
    };
    let resolved_model_id =
        explicit_model_id.or_else(|| session.as_ref().and_then(session_selected_model_id));

    let profile = if let Some(model_id) = resolved_model_id {
        let profile = ai_model_configs::get_ai_model_config_by_id(model_id.as_str())
            .await
            .map_err(|err| format!("读取模型配置失败: {err}"))?
            .ok_or_else(|| format!("模型配置不存在: {}", model_id))?;
        if let Some(user_id) = user_id {
            if profile.user_id.as_deref() != Some(user_id) {
                return Err(format!("无权访问模型配置: {}", model_id));
            }
        }
        profile
    } else {
        let profiles = ai_model_configs::list_ai_model_configs(user_id)
            .await
            .map_err(|err| format!("读取模型配置失败: {err}"))?;
        pick_default_engine_profile(profiles)?
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
