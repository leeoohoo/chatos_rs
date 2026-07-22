// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

use crate::config::normalize_optional;
use crate::relay::{relay_error_response, RelayRequest};
use crate::{local_now_rfc3339, LocalState};
use chatos_plugin_management_sdk::normalize_agent_prompt_vendor;

use super::provider_catalog::{
    default_base_url_for_provider, fallback_model_list, fetch_provider_models,
    runtime_provider_for_model,
};
use super::types::{
    LocalModelCatalogResponse, LocalModelConfigDraft, LocalModelConfigPublic,
    LocalModelConfigRecord, LocalModelRuntimeResponse, LocalModelSettings,
};

mod support;

use self::support::{
    find_credential_replacement, is_supported_configured_provider, is_user_service_not_found,
    model_record_has_local_api_key, normalize_configured_provider, optional_text_update,
    owner_user_id_from_auth, repair_model_settings_with_credential_fallbacks,
    request_user_service_json, required_text, server_model_id_for_local,
};

pub(crate) fn list_local_model_configs(state: &LocalState) -> Vec<LocalModelConfigPublic> {
    state
        .model_configs
        .configs
        .iter()
        .filter(|item| is_supported_configured_provider(item.provider.as_str()))
        .map(LocalModelConfigRecord::public_value)
        .collect()
}

pub(crate) async fn preview_local_model_catalog(
    http_client: &reqwest::Client,
    state: &LocalState,
    draft: LocalModelConfigDraft,
) -> Result<LocalModelCatalogResponse> {
    let provider = normalize_configured_provider(draft.provider.clone())?;
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
    let provider = normalize_configured_provider(draft.provider)?;
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
    let prompt_vendor = normalize_optional(draft.prompt_vendor.as_deref())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.prompt_vendor.as_deref()))
        })
        .or_else(|| {
            normalize_agent_prompt_vendor(None, provider.as_str())
                .map(|vendor| vendor.as_str().to_string())
        });
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
        prompt_vendor,
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
    let provider = normalize_configured_provider(Some(current.provider.clone()))?;
    let server_model_config_id = current
        .server_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let payload = json!({
        "id": server_model_config_id.unwrap_or(current.id.as_str()),
        "owner_user_id": owner_user_id,
        "name": current.name,
        "provider": provider,
        "prompt_vendor": current.prompt_vendor,
        "model": current.model,
        "thinking_level": current.thinking_level,
        "task_usage_scenario": current.task_usage_scenario.clone().unwrap_or_default(),
        "task_thinking_level": current.task_thinking_level.clone().unwrap_or_default(),
        "temperature": current.temperature,
        "max_output_tokens": current.max_output_tokens,
        "api_key": current.api_key,
        "has_api_key": current.api_key.as_deref().map(str::trim).is_some_and(|value| !value.is_empty()),
        "base_url": current.base_url,
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
        method.clone(),
        path.as_str(),
        Some(&payload),
    )
    .await;
    let saved = match saved {
        Ok(saved) => saved,
        Err(err) if method == Method::PATCH && is_user_service_not_found(&err) => {
            request_user_service_json::<Value, Value>(
                http_client,
                &auth,
                Method::POST,
                "/api/model-configs",
                Some(&payload),
            )
            .await?
        }
        Err(err) => return Err(err),
    };
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

pub(crate) async fn reconcile_local_model_configs(
    http_client: &reqwest::Client,
    state: &mut LocalState,
) -> Result<usize> {
    let auth = state.auth.clone().ok_or_else(|| {
        anyhow!("Local Connector must be logged in before reconciling model configs")
    })?;
    let mut remote = request_user_service_json::<(), Vec<Value>>(
        http_client,
        &auth,
        Method::GET,
        "/api/model-configs",
        None,
    )
    .await?;
    if let Ok(remote_settings) = request_user_service_json::<(), Value>(
        http_client,
        &auth,
        Method::GET,
        "/api/model-configs/settings",
        None,
    )
    .await
    {
        if let Some(max_retries) = remote_settings
            .get("model_request_max_retries")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value <= 10)
        {
            state.model_configs.settings.model_request_max_retries = max_retries;
        }
    }
    let remote_ids = remote
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let missing_local_ids = state
        .model_configs
        .configs
        .iter()
        .filter(|item| {
            if !is_supported_configured_provider(item.provider.as_str()) {
                return false;
            }
            let server_id = item
                .server_model_config_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .unwrap_or(item.id.as_str());
            !remote_ids.contains(server_id)
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();

    for local_id in &missing_local_ids {
        sync_local_model_config(http_client, state, local_id.as_str()).await?;
    }

    if !missing_local_ids.is_empty() {
        remote = request_user_service_json::<(), Vec<Value>>(
            http_client,
            &auth,
            Method::GET,
            "/api/model-configs",
            None,
        )
        .await?;
    }

    let mut synchronized = missing_local_ids.len();
    let mut authoritative_ids = HashSet::new();
    for item in remote {
        let Some(server_id) = item
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        authoritative_ids.insert(server_id.to_string());
        let path = format!(
            "/api/model-configs/{}?include_secret=true",
            urlencoding::encode(server_id)
        );
        let remote = request_user_service_json::<(), Value>(
            http_client,
            &auth,
            Method::GET,
            path.as_str(),
            None,
        )
        .await?;
        upsert_server_model_config(state, &remote)?;
        synchronized += 1;
    }

    let removed_local_ids = state
        .model_configs
        .configs
        .iter()
        .filter(|item| is_supported_configured_provider(item.provider.as_str()))
        .filter(|item| {
            let server_id = item
                .server_model_config_id
                .as_deref()
                .unwrap_or(item.id.as_str());
            !authoritative_ids.contains(server_id)
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    if !removed_local_ids.is_empty() {
        state
            .model_configs
            .configs
            .retain(|item| !removed_local_ids.contains(&item.id));
        for local_id in &removed_local_ids {
            state.model_configs.settings.clear_model_id(local_id);
        }
        synchronized += removed_local_ids.len();
    }

    synchronized += repair_model_settings_with_credential_fallbacks(state);

    Ok(synchronized)
}

fn upsert_server_model_config(state: &mut LocalState, value: &Value) -> Result<()> {
    let server_id = required_json_text(value, "id")?;
    let provider = normalize_configured_provider(Some(required_json_text(value, "provider")?))?;
    let model = required_json_text(value, "model")?;
    let now = local_now_rfc3339();
    let existing_index = state.model_configs.configs.iter().position(|item| {
        item.server_model_config_id.as_deref() == Some(server_id.as_str()) || item.id == server_id
    });
    let existing = existing_index.and_then(|index| state.model_configs.configs.get(index).cloned());
    let record = LocalModelConfigRecord {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| server_id.clone()),
        server_model_config_id: Some(server_id),
        name: json_text(value, "name").unwrap_or_else(|| model.clone()),
        provider,
        prompt_vendor: json_text(value, "prompt_vendor"),
        model,
        base_url: json_text(value, "base_url"),
        api_key: json_text(value, "api_key"),
        enabled: json_bool(value, "enabled").unwrap_or(true),
        supports_images: json_bool(value, "supports_images").unwrap_or(false),
        supports_reasoning: json_bool(value, "supports_reasoning").unwrap_or(false),
        supports_responses: json_bool(value, "supports_responses").unwrap_or(true),
        thinking_level: json_text(value, "thinking_level"),
        task_usage_scenario: json_text(value, "task_usage_scenario"),
        task_thinking_level: json_text(value, "task_thinking_level"),
        temperature: value.get("temperature").and_then(Value::as_f64),
        max_output_tokens: value.get("max_output_tokens").and_then(Value::as_i64),
        created_at: json_text(value, "created_at")
            .or_else(|| existing.as_ref().map(|item| item.created_at.clone()))
            .unwrap_or_else(|| now.clone()),
        updated_at: json_text(value, "updated_at").unwrap_or(now),
    };
    if let Some(index) = existing_index {
        state.model_configs.configs[index] = record;
    } else {
        state.model_configs.configs.push(record);
    }
    Ok(())
}

fn required_json_text(value: &Value, field: &str) -> Result<String> {
    json_text(value, field).ok_or_else(|| anyhow!("server model config missing {field}"))
}

fn json_text(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn json_bool(value: &Value, field: &str) -> Option<bool> {
    value.get(field).and_then(Value::as_bool)
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
    if settings.model_request_max_retries > 10 {
        return Err(anyhow!(
            "model_request_max_retries must be between 0 and 10"
        ));
    }
    settings.memory_summary_model_config_id =
        normalize_optional(settings.memory_summary_model_config_id.as_deref());
    settings.memory_summary_thinking_level =
        normalize_optional(settings.memory_summary_thinking_level.as_deref());
    settings.project_management_agent_model_config_id =
        normalize_optional(settings.project_management_agent_model_config_id.as_deref());
    settings.project_management_agent_thinking_level =
        normalize_optional(settings.project_management_agent_thinking_level.as_deref());
    settings.environment_initialization_model_config_id = normalize_optional(
        settings
            .environment_initialization_model_config_id
            .as_deref(),
    );
    settings.environment_initialization_thinking_level = normalize_optional(
        settings
            .environment_initialization_thinking_level
            .as_deref(),
    );
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
        "model_request_max_retries": local.model_request_max_retries,
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
    let selected = state
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
    if !selected.enabled {
        return Err(anyhow!("model config is disabled: {model_config_id}"));
    }
    let record = if model_record_has_local_api_key(selected) {
        selected
    } else {
        find_credential_replacement(state, selected).unwrap_or(selected)
    };
    let provider = normalize_configured_provider(Some(record.provider.clone()))?;
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
        .unwrap_or_else(|| default_base_url_for_provider(provider.as_str()));
    let prompt_vendor = record.prompt_vendor.clone().or_else(|| {
        normalize_agent_prompt_vendor(None, provider.as_str())
            .map(|vendor| vendor.as_str().to_string())
    });
    Ok(LocalModelRuntimeResponse {
        id: record
            .server_model_config_id
            .clone()
            .unwrap_or_else(|| record.id.clone()),
        local_model_config_id: record.id.clone(),
        provider: runtime_provider_for_model(provider.as_str(), base_url.as_str()),
        prompt_vendor,
        base_url,
        api_key,
        model: record.model.clone(),
        thinking_level: record.thinking_level.clone(),
        supports_images: record.supports_images,
        supports_reasoning: record.supports_reasoning,
        supports_responses: record.supports_responses,
        temperature: record.temperature,
        max_output_tokens: record.max_output_tokens,
        model_request_max_retries: state.model_configs.settings.model_request_max_retries,
    })
}

pub(crate) async fn handle_model_runtime_request(value: Value, _state: &LocalState) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("model_runtime_response", "", 400, err.to_string());
        }
    };
    relay_error_response(
        "model_runtime_response",
        request.request_id.as_str(),
        403,
        "Local model credentials are device-only; remote model runtime requests are disabled"
            .to_string(),
    )
}

#[cfg(test)]
include!("service.test.rs");
