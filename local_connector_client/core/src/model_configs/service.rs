// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::config::{api_url, normalize_optional};
use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::{local_now_rfc3339, AuthState, LocalState};

use super::types::{
    LocalModelCatalogResponse, LocalModelConfigDraft, LocalModelConfigPublic,
    LocalModelConfigRecord, LocalModelRuntimeResponse, LocalModelSettings,
    LocalProviderModelRecord,
};

pub(crate) fn list_local_model_configs(state: &LocalState) -> Vec<LocalModelConfigPublic> {
    state
        .model_configs
        .configs
        .iter()
        .map(LocalModelConfigRecord::public_value)
        .collect()
}

pub(crate) async fn preview_local_model_catalog(
    http_client: &reqwest::Client,
    state: &LocalState,
    draft: LocalModelConfigDraft,
) -> Result<LocalModelCatalogResponse> {
    let provider = normalize_provider(draft.provider.clone());
    let existing = draft
        .id
        .as_deref()
        .and_then(|id| normalize_optional(Some(id)))
        .and_then(|id| {
            state
                .model_configs
                .configs
                .iter()
                .find(|item| item.id == id)
        })
        .cloned();
    let base_url = normalize_optional(draft.base_url.as_deref())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.base_url.as_deref()))
        })
        .unwrap_or_else(|| default_base_url_for_provider(provider.as_str()));
    let api_key = if draft.clear_api_key.unwrap_or(false) {
        None
    } else {
        normalize_optional(draft.api_key.as_deref()).or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.api_key.as_deref()))
        })
    };
    let fallback_model = normalize_optional(draft.model.as_deref()).or_else(|| {
        existing
            .as_ref()
            .and_then(|item| normalize_optional(Some(item.model.as_str())))
    });
    let Some(api_key) = api_key else {
        return Ok(LocalModelCatalogResponse {
            provider,
            base_url,
            source: "fallback".to_string(),
            fetched_at: None,
            models: fallback_model_list(fallback_model.as_deref()),
            error: Some("当前供应商配置未提供 API Key".to_string()),
        });
    };

    match fetch_provider_models(
        http_client,
        provider.as_str(),
        base_url.as_str(),
        api_key.as_str(),
    )
    .await
    {
        Ok(models) => Ok(LocalModelCatalogResponse {
            provider,
            base_url,
            source: "live".to_string(),
            fetched_at: Some(local_now_rfc3339()),
            models,
            error: None,
        }),
        Err(error) => Ok(LocalModelCatalogResponse {
            provider,
            base_url,
            source: "fallback".to_string(),
            fetched_at: None,
            models: fallback_model_list(fallback_model.as_deref()),
            error: Some(error),
        }),
    }
}

pub(crate) fn save_local_model_config(
    state: &mut LocalState,
    draft: LocalModelConfigDraft,
) -> Result<LocalModelConfigRecord> {
    let name = required_text(Some(draft.name), "name")?;
    let model = required_text(draft.model, "model")?;
    let provider = normalize_provider(draft.provider);
    let now = local_now_rfc3339();
    let id = draft
        .id
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_index = state
        .model_configs
        .configs
        .iter()
        .position(|item| item.id == id);
    let existing = existing_index.and_then(|idx| state.model_configs.configs.get(idx).cloned());
    let copied_api_key = draft
        .copy_api_key_from_id
        .as_deref()
        .and_then(|copy_id| normalize_optional(Some(copy_id)))
        .and_then(|copy_id| {
            state
                .model_configs
                .configs
                .iter()
                .find(|item| item.id == copy_id)
        })
        .and_then(|item| normalize_optional(item.api_key.as_deref()));
    let api_key = if draft.clear_api_key.unwrap_or(false) {
        None
    } else {
        normalize_optional(draft.api_key.as_deref())
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|item| normalize_optional(item.api_key.as_deref()))
            })
            .or(copied_api_key)
    };
    let record = LocalModelConfigRecord {
        id,
        server_model_config_id: normalize_optional(draft.server_model_config_id.as_deref())
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|item| item.server_model_config_id.clone())
            }),
        name,
        provider,
        model,
        base_url: normalize_optional(draft.base_url.as_deref()).or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.base_url.as_deref()))
        }),
        api_key,
        enabled: draft
            .enabled
            .or_else(|| existing.as_ref().map(|item| item.enabled))
            .unwrap_or(true),
        supports_images: draft
            .supports_images
            .or_else(|| existing.as_ref().map(|item| item.supports_images))
            .unwrap_or(false),
        supports_reasoning: draft
            .supports_reasoning
            .or_else(|| existing.as_ref().map(|item| item.supports_reasoning))
            .unwrap_or(false),
        supports_responses: draft
            .supports_responses
            .or_else(|| existing.as_ref().map(|item| item.supports_responses))
            .unwrap_or(true),
        thinking_level: optional_text_update(
            draft.thinking_level.as_deref(),
            existing
                .as_ref()
                .and_then(|item| item.thinking_level.as_deref()),
        ),
        task_usage_scenario: optional_text_update(
            draft.task_usage_scenario.as_deref(),
            existing
                .as_ref()
                .and_then(|item| item.task_usage_scenario.as_deref()),
        ),
        task_thinking_level: optional_text_update(
            draft.task_thinking_level.as_deref(),
            existing
                .as_ref()
                .and_then(|item| item.task_thinking_level.as_deref()),
        ),
        temperature: if draft.clear_temperature.unwrap_or(false) {
            None
        } else {
            draft
                .temperature
                .or_else(|| existing.as_ref().and_then(|item| item.temperature))
        },
        max_output_tokens: if draft.clear_max_output_tokens.unwrap_or(false) {
            None
        } else {
            draft
                .max_output_tokens
                .or_else(|| existing.as_ref().and_then(|item| item.max_output_tokens))
        },
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    if let Some(index) = existing_index {
        state.model_configs.configs[index] = record.clone();
    } else {
        state.model_configs.configs.push(record.clone());
    }
    Ok(record)
}

pub(crate) async fn sync_local_model_config(
    http_client: &reqwest::Client,
    state: &mut LocalState,
    local_model_config_id: &str,
) -> Result<LocalModelConfigRecord> {
    let auth = state
        .auth
        .clone()
        .ok_or_else(|| anyhow!("Local Connector must be logged in before syncing model configs"))?;
    let owner_user_id = owner_user_id_from_auth(&auth)?;
    let index = state
        .model_configs
        .configs
        .iter()
        .position(|item| item.id == local_model_config_id)
        .ok_or_else(|| anyhow!("local model config not found: {local_model_config_id}"))?;
    let current = state.model_configs.configs[index].clone();
    let server_model_config_id = current
        .server_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let payload = json!({
        "id": server_model_config_id.unwrap_or(current.id.as_str()),
        "owner_user_id": owner_user_id,
        "name": current.name,
        "provider": current.provider,
        "model": current.model,
        "thinking_level": current.thinking_level,
        "task_usage_scenario": current.task_usage_scenario.clone().unwrap_or_default(),
        "task_thinking_level": current.task_thinking_level.clone().unwrap_or_default(),
        "has_api_key": current.api_key.as_deref().map(str::trim).is_some_and(|value| !value.is_empty()),
        "enabled": current.enabled,
        "supports_images": current.supports_images,
        "supports_reasoning": current.supports_reasoning,
        "supports_responses": current.supports_responses,
    });
    let (method, path) = if let Some(server_model_config_id) = server_model_config_id {
        (
            Method::PATCH,
            format!(
                "/api/model-configs/{}",
                urlencoding::encode(server_model_config_id)
            ),
        )
    } else {
        (Method::POST, "/api/model-configs".to_string())
    };
    let saved = request_user_service_json::<Value, Value>(
        http_client,
        &auth,
        method,
        path.as_str(),
        Some(&payload),
    )
    .await?;
    let server_model_config_id = saved
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("user_service model config response missing id"))?
        .to_string();
    state.model_configs.configs[index].server_model_config_id = Some(server_model_config_id);
    state.model_configs.configs[index].updated_at = local_now_rfc3339();
    Ok(state.model_configs.configs[index].clone())
}

pub(crate) async fn delete_local_model_config(
    http_client: &reqwest::Client,
    state: &mut LocalState,
    local_model_config_id: &str,
) -> Result<()> {
    let Some(index) = state
        .model_configs
        .configs
        .iter()
        .position(|item| item.id == local_model_config_id)
    else {
        return Ok(());
    };
    let removed = state.model_configs.configs.remove(index);
    state
        .model_configs
        .settings
        .clear_model_id(local_model_config_id);
    if let Some(server_id) = removed.server_model_config_id.as_deref() {
        if let Some(auth) = state.auth.clone() {
            let path = format!("/api/model-configs/{}", urlencoding::encode(server_id));
            let _ = request_user_service_json::<(), Value>(
                http_client,
                &auth,
                Method::DELETE,
                path.as_str(),
                None,
            )
            .await;
        }
    }
    Ok(())
}

pub(crate) fn save_local_model_settings(
    state: &mut LocalState,
    mut settings: LocalModelSettings,
) -> Result<LocalModelSettings> {
    settings.memory_summary_model_config_id =
        normalize_optional(settings.memory_summary_model_config_id.as_deref());
    settings.memory_summary_thinking_level =
        normalize_optional(settings.memory_summary_thinking_level.as_deref());
    settings.project_management_agent_model_config_id =
        normalize_optional(settings.project_management_agent_model_config_id.as_deref());
    settings.project_management_agent_thinking_level =
        normalize_optional(settings.project_management_agent_thinking_level.as_deref());
    settings.command_approval_model_config_id =
        normalize_optional(settings.command_approval_model_config_id.as_deref());
    settings.command_approval_thinking_level =
        normalize_optional(settings.command_approval_thinking_level.as_deref());
    settings.updated_at = Some(local_now_rfc3339());
    state.model_configs.settings = settings.clone();
    Ok(settings)
}

pub(crate) async fn sync_local_model_settings(
    http_client: &reqwest::Client,
    state: &LocalState,
) -> Result<LocalModelSettings> {
    let auth = state.auth.clone().ok_or_else(|| {
        anyhow!("Local Connector must be logged in before syncing model settings")
    })?;
    let owner_user_id = owner_user_id_from_auth(&auth)?;
    let local = &state.model_configs.settings;
    let payload = json!({
        "user_id": owner_user_id,
        "memory_summary_model_config_id": local
            .memory_summary_model_config_id
            .as_deref()
            .and_then(|id| server_model_id_for_local(state, id)),
        "memory_summary_thinking_level": local.memory_summary_thinking_level,
        "project_management_agent_model_config_id": local
            .project_management_agent_model_config_id
            .as_deref()
            .and_then(|id| server_model_id_for_local(state, id)),
        "project_management_agent_thinking_level": local.project_management_agent_thinking_level,
    });
    let _ = request_user_service_json::<Value, Value>(
        http_client,
        &auth,
        Method::PUT,
        "/api/model-configs/settings",
        Some(&payload),
    )
    .await?;
    Ok(local.clone())
}

pub(crate) fn resolve_local_model_runtime(
    state: &LocalState,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<LocalModelRuntimeResponse> {
    let auth = state
        .auth
        .as_ref()
        .ok_or_else(|| anyhow!("Local Connector client is not logged in"))?;
    let paired_owner = owner_user_id_from_auth(auth)?;
    if paired_owner != owner_user_id.trim() {
        return Err(anyhow!(
            "Local Connector is paired to a different user; cannot resolve this model runtime"
        ));
    }
    let model_config_id = model_config_id.trim();
    if model_config_id.is_empty() {
        return Err(anyhow!("model_config_id is required"));
    }
    let record = state
        .model_configs
        .configs
        .iter()
        .find(|item| {
            item.server_model_config_id.as_deref() == Some(model_config_id)
                || item.id.as_str() == model_config_id
        })
        .ok_or_else(|| {
            anyhow!("model config is not mapped in this Local Connector: {model_config_id}")
        })?;
    if !record.enabled {
        return Err(anyhow!("model config is disabled: {model_config_id}"));
    }
    let api_key = record
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("model config has no local API key: {model_config_id}"))?
        .to_string();
    let base_url = record
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_base_url_for_provider(record.provider.as_str()));
    Ok(LocalModelRuntimeResponse {
        id: record
            .server_model_config_id
            .clone()
            .unwrap_or_else(|| record.id.clone()),
        local_model_config_id: record.id.clone(),
        provider: runtime_provider_for_model(record.provider.as_str(), base_url.as_str()),
        base_url,
        api_key,
        model: record.model.clone(),
        thinking_level: record.thinking_level.clone(),
        supports_images: record.supports_images,
        supports_reasoning: record.supports_reasoning,
        supports_responses: record.supports_responses,
        temperature: record.temperature,
        max_output_tokens: record.max_output_tokens,
    })
}

pub(crate) async fn handle_model_runtime_request(value: Value, state: &LocalState) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("model_runtime_response", "", 400, err.to_string());
        }
    };
    let model_config_id = request
        .body
        .get("model_config_id")
        .and_then(Value::as_str)
        .or_else(|| request.path.as_deref().and_then(model_config_id_from_path))
        .unwrap_or_default();
    match resolve_local_model_runtime(
        state,
        request.owner_user_id.as_deref().unwrap_or_default(),
        model_config_id,
    ) {
        Ok(runtime) => RelayResponse {
            message_type: "model_runtime_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body: serde_json::to_value(runtime)
                .unwrap_or_else(|err| json!({ "error": err.to_string() })),
        }
        .to_value(),
        Err(err) => RelayResponse {
            message_type: "model_runtime_response".to_string(),
            request_id: request.request_id,
            status: 400,
            headers: BTreeMap::new(),
            body: json!({ "error": err.to_string() }),
        }
        .to_value(),
    }
}

fn model_config_id_from_path(path: &str) -> Option<&str> {
    path.trim_matches('/')
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn server_model_id_for_local(state: &LocalState, local_model_config_id: &str) -> Option<String> {
    state
        .model_configs
        .configs
        .iter()
        .find(|item| item.id == local_model_config_id)
        .and_then(|item| item.server_model_config_id.clone())
}

fn owner_user_id_from_auth(auth: &AuthState) -> Result<String> {
    auth.user
        .as_ref()
        .map(|user| user.id.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("current user is unavailable"))
}

async fn request_user_service_json<TBody, TResp>(
    http_client: &reqwest::Client,
    auth: &AuthState,
    method: Method,
    path: &str,
    body: Option<&TBody>,
) -> Result<TResp>
where
    TBody: serde::Serialize + ?Sized,
    TResp: DeserializeOwned,
{
    let endpoint = api_url(auth.user_service_base_url.as_str(), path);
    let mut request = http_client
        .request(method, endpoint.as_str())
        .bearer_auth(auth.access_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("request user_service {endpoint} failed"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(text.as_str())
        ));
    }
    if text.trim().is_empty() {
        return serde_json::from_value(Value::Null).context("decode empty user_service response");
    }
    serde_json::from_str::<TResp>(text.as_str())
        .with_context(|| format!("decode user_service response failed: {text}"))
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

fn required_text(value: Option<String>, field: &str) -> Result<String> {
    normalize_optional(value.as_deref())
        .ok_or_else(|| anyhow!("{field} is required and cannot be empty"))
}

fn optional_text_update(draft: Option<&str>, existing: Option<&str>) -> Option<String> {
    match draft {
        Some(value) => normalize_optional(Some(value)),
        None => existing.and_then(|value| normalize_optional(Some(value))),
    }
}

fn normalize_provider(value: Option<String>) -> String {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("gpt")
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "openai" | "gpt" => "gpt".to_string(),
        "deepseek" => "deepseek".to_string(),
        "kimi" | "kimik2" | "moonshot" => "kimi".to_string(),
        "minimax" => "minimax".to_string(),
        "openai_compatible" | "compatible" => "openai_compatible".to_string(),
        other => other.to_string(),
    }
}

fn default_base_url_for_provider(provider: &str) -> String {
    match normalize_provider(Some(provider.to_string())).as_str() {
        "deepseek" => "https://api.deepseek.com/v1".to_string(),
        "kimi" => "https://api.moonshot.cn/v1".to_string(),
        "minimax" => "https://api.minimax.chat/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

fn runtime_provider_for_model(provider: &str, base_url: &str) -> String {
    let provider = normalize_provider(Some(provider.to_string()));
    if provider == "gpt"
        && !base_url
            .trim()
            .to_ascii_lowercase()
            .contains("api.openai.com")
    {
        "openai_compatible".to_string()
    } else {
        provider
    }
}

async fn fetch_provider_models(
    http_client: &reqwest::Client,
    provider: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<LocalProviderModelRecord>, String> {
    let mut errors = Vec::new();
    for url in model_list_urls(provider, base_url) {
        match http_client
            .get(url.as_str())
            .bearer_auth(api_key)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    let message = format!(
                        "模型列表接口 {} 返回 {}: {}",
                        url,
                        status.as_u16(),
                        preview_text(text.as_str(), 800)
                    );
                    if matches!(
                        status,
                        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                    ) {
                        return Err(message);
                    }
                    errors.push(message);
                    continue;
                }
                let raw = serde_json::from_str::<Value>(text.as_str())
                    .map_err(|err| format!("解析模型列表失败: {err}"))?;
                return Ok(normalize_provider_models(provider, &raw));
            }
            Err(err) => {
                errors.push(format!("请求模型列表接口 {url} 失败: {err}"));
            }
        }
    }
    Err(if errors.is_empty() {
        "获取模型列表失败".to_string()
    } else {
        errors.join("；")
    })
}

fn model_list_urls(provider: &str, base_url: &str) -> Vec<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    let mut urls = Vec::new();
    push_unique_url(&mut urls, format!("{base_url}/models"));
    if base_url.ends_with("/v1") {
        let fallback = base_url.trim_end_matches("/v1");
        push_unique_url(&mut urls, format!("{fallback}/models"));
    }
    if normalize_provider(Some(provider.to_string())) == "deepseek" && !base_url.ends_with("/v1") {
        push_unique_url(&mut urls, format!("{base_url}/v1/models"));
    }
    urls
}

fn push_unique_url(urls: &mut Vec<String>, url: String) {
    if !urls.iter().any(|existing| existing == &url) {
        urls.push(url);
    }
}

fn normalize_provider_models(provider: &str, raw: &Value) -> Vec<LocalProviderModelRecord> {
    raw.get("data")
        .and_then(Value::as_array)
        .or_else(|| raw.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| normalize_provider_model_item(provider, item))
        .collect()
}

fn normalize_provider_model_item(provider: &str, item: &Value) -> Option<LocalProviderModelRecord> {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let provider = normalize_provider(Some(provider.to_string()));
    Some(LocalProviderModelRecord {
        id,
        owned_by: item
            .get("owned_by")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        context_length: read_provider_model_i64_field(
            item,
            &["context_length", "max_context_length", "max_tokens"],
        ),
        supports_images: read_provider_model_bool_field(
            item,
            &["supports_images", "supports_image_in", "vision", "image"],
        ),
        supports_reasoning: read_provider_model_bool_field(
            item,
            &["supports_reasoning", "reasoning"],
        ),
        supports_responses: read_provider_model_bool_field(item, &["supports_responses"])
            || provider == "gpt",
    })
}

fn read_provider_model_bool_field(item: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn read_provider_model_i64_field(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(Value::as_i64))
}

fn fallback_model_list(model: Option<&str>) -> Vec<LocalProviderModelRecord> {
    model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|id| {
            vec![LocalProviderModelRecord {
                id: id.to_string(),
                owned_by: None,
                context_length: None,
                supports_images: false,
                supports_reasoning: false,
                supports_responses: false,
            }]
        })
        .unwrap_or_default()
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let mut output = value.trim().chars().take(max_chars).collect::<String>();
    if value.trim().chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

impl LocalModelSettings {
    fn clear_model_id(&mut self, local_model_config_id: &str) {
        if self.memory_summary_model_config_id.as_deref() == Some(local_model_config_id) {
            self.memory_summary_model_config_id = None;
            self.memory_summary_thinking_level = None;
        }
        if self.project_management_agent_model_config_id.as_deref() == Some(local_model_config_id) {
            self.project_management_agent_model_config_id = None;
            self.project_management_agent_thinking_level = None;
        }
        if self.command_approval_model_config_id.as_deref() == Some(local_model_config_id) {
            self.command_approval_model_config_id = None;
            self.command_approval_thinking_level = None;
        }
        self.updated_at = Some(local_now_rfc3339());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_openai_style_model_catalog() {
        let raw = json!({
            "data": [
                {
                    "id": "gpt-4.1",
                    "owned_by": "openai",
                    "context_length": 128000,
                    "supports_images": true,
                    "supports_reasoning": false,
                    "supports_responses": true
                }
            ]
        });

        let models = normalize_provider_models("gpt", &raw);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-4.1");
        assert_eq!(models[0].owned_by.as_deref(), Some("openai"));
        assert_eq!(models[0].context_length, Some(128000));
        assert!(models[0].supports_images);
        assert!(models[0].supports_responses);
        assert!(!models[0].supports_reasoning);
    }

    #[test]
    fn fallback_model_list_keeps_current_selection() {
        let models = fallback_model_list(Some("deepseek-chat"));

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "deepseek-chat");
        assert!(!models[0].supports_images);
        assert!(!models[0].supports_reasoning);
    }

    #[test]
    fn model_list_urls_try_root_models_for_v1_base_url() {
        assert_eq!(
            model_list_urls("gpt", "https://newapi.example.com/v1"),
            vec![
                "https://newapi.example.com/v1/models".to_string(),
                "https://newapi.example.com/models".to_string(),
            ]
        );
    }

    #[test]
    fn model_list_urls_try_v1_models_for_deepseek_root_base_url() {
        assert_eq!(
            model_list_urls("deepseek", "https://api.deepseek.com"),
            vec![
                "https://api.deepseek.com/models".to_string(),
                "https://api.deepseek.com/v1/models".to_string(),
            ]
        );
    }

    #[test]
    fn optional_text_update_can_clear_existing_value() {
        assert_eq!(optional_text_update(Some(""), Some("task planning")), None);
        assert_eq!(
            optional_text_update(None, Some("task planning")).as_deref(),
            Some("task planning")
        );
    }
}
