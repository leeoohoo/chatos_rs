// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use reqwest::Method;
use serde_json::Value;
use std::collections::HashSet;

use crate::config::normalize_optional;
use crate::relay::{relay_error_response, RelayRequest};
use crate::{local_now_rfc3339, LocalState};
use chatos_plugin_management_sdk::normalize_agent_prompt_vendor;

use super::provider_catalog::{default_base_url_for_provider, runtime_provider_for_model};
use super::types::{
    LocalModelConfigPublic, LocalModelConfigRecord, LocalModelRuntimeResponse, LocalModelSettings,
};

mod support;

use self::support::{
    find_credential_replacement, is_supported_configured_provider, model_record_has_local_api_key,
    normalize_configured_provider, owner_user_id_from_auth,
    repair_model_settings_with_credential_fallbacks, request_user_service_json,
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

pub(crate) async fn reconcile_local_model_configs(
    http_client: &reqwest::Client,
    state: &mut LocalState,
) -> Result<usize> {
    let mut next = state.clone();
    let synchronized = reconcile_local_model_configs_inner(http_client, &mut next).await?;
    *state = next;
    Ok(synchronized)
}

async fn reconcile_local_model_configs_inner(
    http_client: &reqwest::Client,
    state: &mut LocalState,
) -> Result<usize> {
    let auth = state.auth.clone().ok_or_else(|| {
        anyhow!("Local Connector must be logged in before reconciling model configs")
    })?;
    let remote = request_user_service_json::<(), Vec<Value>>(
        http_client,
        &auth,
        Method::GET,
        "/api/model-configs",
        None,
    )
    .await?;
    let mut synchronized = 0;
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

    synchronized += remove_non_authoritative_model_configs(state, &authoritative_ids);

    synchronized += repair_model_settings_with_credential_fallbacks(state);

    Ok(synchronized)
}

fn remove_non_authoritative_model_configs(
    state: &mut LocalState,
    authoritative_ids: &HashSet<String>,
) -> usize {
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
    }
    removed_local_ids.len()
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

pub(crate) fn save_local_model_settings(
    state: &mut LocalState,
    mut settings: LocalModelSettings,
) -> Result<LocalModelSettings> {
    if settings.model_request_max_retries > 10 {
        return Err(anyhow!(
            "model_request_max_retries must be between 0 and 10"
        ));
    }
    settings.command_approval_model_config_id =
        normalize_optional(settings.command_approval_model_config_id.as_deref());
    settings.command_approval_thinking_level =
        normalize_optional(settings.command_approval_thinking_level.as_deref());
    settings.updated_at = Some(local_now_rfc3339());
    state.model_configs.settings = settings.clone();
    Ok(settings)
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
