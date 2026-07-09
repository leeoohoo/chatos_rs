// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::{
    resolve_local_connector_model_runtime, LocalConnectorModelRuntimeLookup,
};

use crate::config::AppConfig;
use crate::models::EngineModelProfile;

pub(super) async fn resolve_model_runtime_for_profile(
    config: &AppConfig,
    profile: &EngineModelProfile,
    owner_user_id: Option<&str>,
) -> Result<EngineModelProfile, String> {
    let owner_user_id = owner_user_id
        .or(profile.owner_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let has_embedded_runtime = profile
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        && profile
            .base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
    let Some(owner_user_id) = owner_user_id else {
        return if has_embedded_runtime {
            Ok(profile.clone())
        } else {
            Err(format!(
                "owner_user_id is required to resolve Memory Engine model runtime for {}",
                profile.id
            ))
        };
    };
    let Some(secret) = config
        .local_connector_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return if has_embedded_runtime {
            Ok(profile.clone())
        } else {
            Err("MEMORY_ENGINE_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required to resolve local model runtime".to_string())
        };
    };

    let runtime = resolve_local_connector_model_runtime(LocalConnectorModelRuntimeLookup {
        base_url: config.local_connector_service_base_url.as_str(),
        request_timeout: std::time::Duration::from_millis(
            config.local_connector_service_request_timeout_ms,
        ),
        internal_secret: secret,
        owner_user_id,
        model_config_id: profile.id.as_str(),
    })
    .await
    .map_err(|err| err.to_string())?;
    let mut resolved = profile.clone();
    resolved.provider = runtime.provider;
    resolved.base_url = Some(runtime.base_url);
    resolved.api_key = Some(runtime.api_key);
    resolved.model = runtime.model;
    resolved.thinking_level = runtime.thinking_level.or(resolved.thinking_level);
    resolved.supports_images = runtime.supports_images;
    resolved.supports_reasoning = runtime.supports_reasoning;
    resolved.supports_responses = runtime.supports_responses;
    if runtime.temperature.is_some() {
        resolved.temperature = runtime.temperature;
    }
    Ok(resolved)
}
