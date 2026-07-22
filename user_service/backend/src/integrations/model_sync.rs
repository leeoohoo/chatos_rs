// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

use crate::models::{UserModelConfigRecord, UserModelSettingsRecord};
use crate::state::AppState;

use super::http::{build_client, extract_error_message, normalized_text, normalized_url};
pub async fn sync_model_config_upsert(
    state: &AppState,
    config: &UserModelConfigRecord,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Err(err) = sync_memory_engine_model_profile(state, config).await {
        warn!(
            model_config_id = config.id.as_str(),
            owner_user_id = config.owner_user_id.as_str(),
            error = err.as_str(),
            "sync model config to memory_engine failed"
        );
        warnings.push(format!("memory_engine model update failed: {err}"));
    }

    if let Err(err) = sync_task_runner_model_config(state, config).await {
        warn!(
            model_config_id = config.id.as_str(),
            owner_user_id = config.owner_user_id.as_str(),
            error = err.as_str(),
            "sync model config to task_runner failed"
        );
        warnings.push(format!("task_runner model update failed: {err}"));
    }

    warnings
}

pub async fn sync_model_config_delete(state: &AppState, model_config_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Err(err) = delete_memory_engine_model_profile(state, model_config_id).await {
        warn!(
            model_config_id,
            error = err.as_str(),
            "delete memory_engine model profile failed"
        );
        warnings.push(format!("memory_engine delete failed: {err}"));
    }

    if let Err(err) = delete_task_runner_model_config(state, model_config_id).await {
        warn!(
            model_config_id,
            error = err.as_str(),
            "delete task_runner model config failed"
        );
        warnings.push(format!("task_runner delete failed: {err}"));
    }

    warnings
}

pub async fn sync_model_settings(
    state: &AppState,
    settings: &UserModelSettingsRecord,
) -> Vec<String> {
    let mut warnings = sync_task_runner_model_settings(state, settings).await;
    let Some(memory_engine_base_url) =
        normalized_url(state.config.memory_engine_base_url.as_deref())
    else {
        return warnings;
    };
    let Some(operator_token) =
        normalized_text(state.config.memory_engine_operator_token.as_deref())
    else {
        warnings.push("memory_engine operator token is not configured".to_string());
        return warnings;
    };
    let owner_user_id = settings.user_id.as_str();
    let profiles = match list_memory_engine_model_profiles(
        state,
        memory_engine_base_url.as_str(),
        operator_token.as_str(),
        owner_user_id,
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            warn!(
                owner_user_id,
                error = err.as_str(),
                "load memory_engine model profiles for settings update failed"
            );
            warnings.push(format!("memory_engine settings update failed: {err}"));
            return warnings;
        }
    };

    let selected_id = normalized_text(settings.memory_summary_model_config_id.as_deref());
    for profile in profiles {
        let profile_id = profile
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        if profile_id.is_empty() {
            continue;
        }
        let desired_default = selected_id.as_deref() == Some(profile_id.as_str());
        let current_default = profile
            .get("is_default")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let current_thinking_level = profile
            .get("thinking_level")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let desired_thinking_level = if desired_default {
            normalized_text(settings.memory_summary_thinking_level.as_deref())
        } else {
            current_thinking_level.map(ToOwned::to_owned)
        };
        if current_default == desired_default
            && current_thinking_level == desired_thinking_level.as_deref()
        {
            continue;
        }

        let body = serde_json::json!({
            "id": profile_id,
            "name": profile.get("name").and_then(Value::as_str),
            "provider": profile.get("provider").and_then(Value::as_str),
            "model": profile.get("model").and_then(Value::as_str),
            "base_url": Value::Null,
            "api_key": Value::Null,
            "supports_images": profile.get("supports_images"),
            "supports_reasoning": profile.get("supports_reasoning"),
            "supports_responses": profile.get("supports_responses"),
            "temperature": profile.get("temperature"),
            "thinking_level": desired_thinking_level,
            "model_request_max_retries": settings.model_request_max_retries,
            "is_default": desired_default,
            "enabled": profile.get("enabled"),
        });

        if let Err(err) = memory_engine_request_json::<Value, _>(
            state,
            Method::PUT,
            &format!(
                "{memory_engine_base_url}/admin/model-profiles/{}",
                urlencoding::encode(profile_id.as_str())
            ),
            operator_token.as_str(),
            Some(&body),
        )
        .await
        {
            warn!(
                owner_user_id,
                model_config_id = profile_id.as_str(),
                error = err.as_str(),
                "update memory_engine profile default flag failed"
            );
            warnings.push(format!(
                "memory_engine default model update failed for {}: {err}",
                profile_id
            ));
        }
    }

    warnings
}

async fn sync_task_runner_model_settings(
    state: &AppState,
    settings: &UserModelSettingsRecord,
) -> Vec<String> {
    let configs = match state
        .store
        .list_user_model_configs(Some(settings.user_id.as_str()))
        .await
    {
        Ok(configs) => configs,
        Err(err) => return vec![format!("task_runner settings update failed: {err}")],
    };
    let mut warnings = Vec::new();
    for config in configs {
        if let Err(err) = sync_task_runner_model_config(state, &config).await {
            warnings.push(format!(
                "task_runner retry setting update failed for {}: {err}",
                config.id
            ));
        }
    }
    warnings
}

async fn sync_memory_engine_model_profile(
    state: &AppState,
    config: &UserModelConfigRecord,
) -> Result<(), String> {
    ensure_concrete_model(config)?;
    ensure_supported_provider(config)?;
    let Some(memory_engine_base_url) =
        normalized_url(state.config.memory_engine_base_url.as_deref())
    else {
        return Ok(());
    };
    let Some(operator_token) =
        normalized_text(state.config.memory_engine_operator_token.as_deref())
    else {
        return Err("MEMORY_ENGINE_OPERATOR_TOKEN is not configured".to_string());
    };

    let settings = state
        .store
        .get_user_model_settings(config.owner_user_id.as_str())
        .await?;
    let is_default = settings
        .as_ref()
        .and_then(|settings| settings.memory_summary_model_config_id.as_deref())
        .is_some_and(|value| value == config.id);
    let thinking_level = if is_default {
        settings
            .as_ref()
            .and_then(|settings| settings.memory_summary_thinking_level.clone())
    } else {
        config.thinking_level.clone()
    };
    let model_request_max_retries = settings
        .as_ref()
        .map(|settings| settings.model_request_max_retries)
        .unwrap_or(crate::models::DEFAULT_MODEL_REQUEST_MAX_RETRIES);

    let payload = serde_json::json!({
        "id": config.id,
        "name": config.name,
        "provider": memory_engine_provider(config.provider.as_str()),
        "model": config.model,
        "base_url": config.base_url,
        "api_key": config.api_key,
        "supports_images": config.supports_images,
        "supports_reasoning": config.supports_reasoning,
        "supports_responses": config.supports_responses,
        "temperature": config.temperature,
        "thinking_level": thinking_level,
        "model_request_max_retries": model_request_max_retries,
        "is_default": is_default,
        "enabled": config.enabled,
    });

    let get_url = format!(
        "{memory_engine_base_url}/admin/model-profiles/{}",
        urlencoding::encode(config.id.as_str())
    );
    let exists = memory_engine_request_json::<Value, _>(
        state,
        Method::GET,
        get_url.as_str(),
        operator_token.as_str(),
        Option::<&()>::None,
    )
    .await
    .is_ok();

    let request_url = if exists {
        get_url
    } else {
        format!(
            "{memory_engine_base_url}/admin/model-profiles?owner_user_id={}",
            urlencoding::encode(config.owner_user_id.as_str())
        )
    };
    let method = if exists { Method::PUT } else { Method::POST };

    let _: Value = memory_engine_request_json(
        state,
        method,
        request_url.as_str(),
        operator_token.as_str(),
        Some(&payload),
    )
    .await?;
    Ok(())
}

async fn delete_memory_engine_model_profile(
    state: &AppState,
    model_config_id: &str,
) -> Result<(), String> {
    let Some(memory_engine_base_url) =
        normalized_url(state.config.memory_engine_base_url.as_deref())
    else {
        return Ok(());
    };
    let Some(operator_token) =
        normalized_text(state.config.memory_engine_operator_token.as_deref())
    else {
        return Err("MEMORY_ENGINE_OPERATOR_TOKEN is not configured".to_string());
    };

    let endpoint = format!(
        "{memory_engine_base_url}/admin/model-profiles/{}",
        urlencoding::encode(model_config_id)
    );
    let request = build_client(state)?.request(Method::DELETE, endpoint);
    let response = signed_memory_engine_request(request, operator_token.as_str())?
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if status.is_success() || status.as_u16() == 404 {
        return Ok(());
    }
    let body =
        read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
            .await;
    Err(format!(
        "memory_engine delete request failed: {} {}",
        status.as_u16(),
        extract_error_message(body.as_str())
    ))
}

async fn list_memory_engine_model_profiles(
    state: &AppState,
    base_url: &str,
    operator_token: &str,
    owner_user_id: &str,
) -> Result<Vec<Value>, String> {
    let endpoint = format!(
        "{base_url}/admin/model-profiles?owner_user_id={}",
        urlencoding::encode(owner_user_id)
    );
    let payload: Value = memory_engine_request_json(
        state,
        Method::GET,
        endpoint.as_str(),
        operator_token,
        Option::<&()>::None,
    )
    .await?;
    Ok(payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

async fn sync_task_runner_model_config(
    state: &AppState,
    config: &UserModelConfigRecord,
) -> Result<(), String> {
    ensure_concrete_model(config)?;
    ensure_supported_provider(config)?;
    let Some(task_runner_base_url) = normalized_url(state.config.task_runner_base_url.as_deref())
    else {
        return Ok(());
    };

    let model_request_max_retries = state
        .store
        .get_user_model_settings(config.owner_user_id.as_str())
        .await?
        .map(|settings| settings.model_request_max_retries)
        .unwrap_or(crate::models::DEFAULT_MODEL_REQUEST_MAX_RETRIES);
    let payload = serde_json::json!({
        "id": config.id,
        "owner_user_id": config.owner_user_id,
        "name": config.name,
        "provider": task_runner_provider(config.provider.as_str()),
        "prompt_vendor": config.prompt_vendor,
        "base_url": config.base_url,
        "api_key": config.api_key,
        "model": config.model,
        "usage_scenario": config.task_usage_scenario,
        "thinking_level": config.task_thinking_level,
        "temperature": config.temperature,
        "max_output_tokens": config.max_output_tokens,
        "model_request_max_retries": model_request_max_retries,
        "supports_responses": config.supports_responses,
        "enabled": config.enabled,
    });

    let _: Value = task_runner_request_json(
        state,
        Method::POST,
        &format!("{task_runner_base_url}/api/chatos-sync/model-configs"),
        Some(&payload),
    )
    .await?;
    Ok(())
}

async fn delete_task_runner_model_config(
    state: &AppState,
    model_config_id: &str,
) -> Result<(), String> {
    let Some(task_runner_base_url) = normalized_url(state.config.task_runner_base_url.as_deref())
    else {
        return Ok(());
    };
    let endpoint = format!(
        "{task_runner_base_url}/api/chatos-sync/model-configs/{}",
        urlencoding::encode(model_config_id)
    );
    let response = task_runner_request(state, Method::DELETE, endpoint.as_str())?
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if response.status().is_success() || response.status().as_u16() == 404 {
        return Ok(());
    }
    let status = response.status().as_u16();
    let body =
        read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
            .await;
    Err(format!(
        "task_runner delete request failed: {} {}",
        status,
        extract_error_message(body.as_str())
    ))
}

async fn memory_engine_request_json<TResp, TBody>(
    state: &AppState,
    method: Method,
    endpoint: &str,
    operator_token: &str,
    body: Option<&TBody>,
) -> Result<TResp, String>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let client = build_client(state)?;
    let mut request =
        signed_memory_engine_request(client.request(method, endpoint), operator_token)?;
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!(
            "memory_engine request failed: {} {}",
            status.as_u16(),
            extract_error_message(body.as_str())
        ));
    }
    read_response_json_limited::<TResp>(response, JSON_BODY_LIMIT_BYTES).await
}

fn signed_memory_engine_request(
    request: reqwest::RequestBuilder,
    secret: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let token = chatos_service_runtime::issue_internal_service_token(
        secret.trim(),
        "user-service",
        "memory-engine",
        "model-profile.sync",
        60,
    )?;
    Ok(request
        .header("x-memory-caller", "user-service")
        .header("x-memory-internal-token", token))
}

fn task_runner_request(
    state: &AppState,
    method: Method,
    endpoint: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let mut request = build_client(state)?.request(method, endpoint);
    if let Some(secret) = normalized_text(state.config.task_runner_callback_secret.as_deref()) {
        request = request.header("x-chatos-callback-secret", secret);
    }
    Ok(request)
}

async fn task_runner_request_json<TResp, TBody>(
    state: &AppState,
    method: Method,
    endpoint: &str,
    body: Option<&TBody>,
) -> Result<TResp, String>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let mut request = task_runner_request(state, method, endpoint)?;
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!(
            "task_runner request failed: {} {}",
            status.as_u16(),
            extract_error_message(body.as_str())
        ));
    }
    read_response_json_limited::<TResp>(response, JSON_BODY_LIMIT_BYTES).await
}

fn task_runner_provider(provider: &str) -> &'static str {
    match provider.trim() {
        "deepseek" => "deepseek",
        "kimi" => "kimik2",
        "glm" => "glm",
        _ => "openai",
    }
}

fn memory_engine_provider(provider: &str) -> &'static str {
    match provider.trim() {
        "deepseek" => "deepseek",
        "kimi" => "openai",
        "glm" => "openai",
        _ => "openai",
    }
}

fn ensure_concrete_model(config: &UserModelConfigRecord) -> Result<(), String> {
    if config.model.trim().is_empty() {
        return Err("model is empty; downstream services require a concrete model".to_string());
    }
    Ok(())
}

fn ensure_supported_provider(config: &UserModelConfigRecord) -> Result<(), String> {
    match config.provider.trim() {
        "gpt" | "deepseek" | "kimi" | "glm" => Ok(()),
        provider => Err(format!("unsupported configured model provider: {provider}")),
    }
}

#[cfg(test)]
mod tests {
    use super::signed_memory_engine_request;

    #[test]
    fn memory_engine_request_uses_scoped_token_without_operator_header() {
        let request = signed_memory_engine_request(
            reqwest::Client::new().get("http://127.0.0.1:7081/test"),
            "a-long-user-service-memory-secret",
        )
        .expect("signed request")
        .build()
        .expect("request");
        assert!(!request.headers().contains_key("x-memory-operator-token"));
        assert_eq!(
            request
                .headers()
                .get("x-memory-caller")
                .and_then(|value| value.to_str().ok()),
            Some("user-service")
        );
        let token = request
            .headers()
            .get("x-memory-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-user-service-memory-secret",
            "user-service",
            "memory-engine",
            "model-profile.sync",
        )
        .expect("valid token");
    }
}
