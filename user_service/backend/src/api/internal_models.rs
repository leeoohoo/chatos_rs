// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::models::DEFAULT_MODEL_REQUEST_MAX_RETRIES;
use crate::state::AppState;
use crate::store::now_rfc3339;
use chatos_plugin_management_sdk::normalize_agent_prompt_vendor;

use super::internal_auth::{
    record_user_service_internal_resource_access, require_user_model_internal_request,
    UserServiceInternalResourceAudit, MODEL_RUNTIME_READ_SCOPE, MODEL_SETTINGS_READ_SCOPE,
};
use super::models::{is_supported_provider, model_config_has_backing_provider};
use super::{bad_request, forbidden, internal_error, not_found, ApiResult};

#[derive(Debug, Serialize)]
pub struct InternalModelRuntimeConfigResponse {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    pub prompt_vendor: Option<String>,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub model_capability_revision: String,
    pub context_window_tokens: i64,
    pub output_reserve_tokens: i64,
    pub protocol_overhead_estimator_id: String,
    pub tokenizer_id: Option<String>,
    pub responses_compaction_threshold: Option<i64>,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
}

#[derive(Debug, Serialize)]
pub struct InternalUserModelSettingsResponse {
    pub user_id: String,
    pub model_request_max_retries: i64,
    pub memory_summary_model_config_id: Option<String>,
    pub memory_summary_thinking_level: Option<String>,
    pub updated_at: String,
}

pub async fn get_user_model_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<InternalUserModelSettingsResponse> {
    let identity =
        require_user_model_internal_request(&state.config, &headers, MODEL_SETTINGS_READ_SCOPE)?;
    let user_id = user_id.trim().to_string();
    let audit_resource_id = if user_id.is_empty() {
        "unknown"
    } else {
        user_id.as_str()
    };
    let result = async {
        if user_id.is_empty() {
            return Err(bad_request("user_id is required"));
        }
        if state
            .store
            .find_user_by_id(user_id.as_str())
            .await
            .map_err(internal_error)?
            .is_none()
        {
            return Err(not_found("user not found"));
        }

        let settings = state
            .store
            .get_user_model_settings(user_id.as_str())
            .await
            .map_err(internal_error)?;
        Ok(Json(match settings {
            Some(settings) => InternalUserModelSettingsResponse {
                user_id: settings.user_id,
                model_request_max_retries: settings.model_request_max_retries,
                memory_summary_model_config_id: settings.memory_summary_model_config_id,
                memory_summary_thinking_level: settings.memory_summary_thinking_level,
                updated_at: settings.updated_at,
            },
            None => InternalUserModelSettingsResponse {
                user_id: user_id.clone(),
                model_request_max_retries: DEFAULT_MODEL_REQUEST_MAX_RETRIES,
                memory_summary_model_config_id: None,
                memory_summary_thinking_level: None,
                updated_at: now_rfc3339(),
            },
        }))
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!user_id.is_empty()).then_some(user_id.as_str()),
            project_id: None,
            resource_type: "user_model_settings",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

pub async fn get_user_model_runtime_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((user_id, model_config_id)): Path<(String, String)>,
) -> ApiResult<InternalModelRuntimeConfigResponse> {
    let identity =
        require_user_model_internal_request(&state.config, &headers, MODEL_RUNTIME_READ_SCOPE)?;
    let user_id = user_id.trim().to_string();
    let model_config_id = model_config_id.trim().to_string();
    let audit_resource_id = if model_config_id.is_empty() {
        "unknown"
    } else {
        model_config_id.as_str()
    };
    let result = async {
        if user_id.is_empty() {
            return Err(bad_request("user_id is required"));
        }
        if model_config_id.is_empty() {
            return Err(bad_request("model_config_id is required"));
        }
        let Some(model_config) = state
            .store
            .find_user_model_config_by_id(model_config_id.as_str())
            .await
            .map_err(internal_error)?
        else {
            return Err(not_found("model config not found"));
        };
        if model_config.owner_user_id != user_id {
            return Err(forbidden("model config does not belong to the target user"));
        }
        if !is_supported_provider(model_config.provider.as_str()) {
            return Err(not_found("model config not found"));
        }
        let providers = state
            .store
            .list_user_model_providers(Some(user_id.as_str()))
            .await
            .map_err(internal_error)?;
        if !model_config_has_backing_provider(&model_config, providers.as_slice()) {
            return Err(bad_request(
                "model config is not backed by an active model provider",
            ));
        }
        if !model_config.enabled {
            return Err(bad_request("model config is disabled"));
        }
        if model_config.model.trim().is_empty() {
            return Err(bad_request("model config requires a concrete model name"));
        }
        let api_key = model_config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| bad_request("cloud model config requires a stored API key"))?
            .to_string();
        let base_url = model_config
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| bad_request("cloud model config requires a stored base_url"))?
            .to_string();
        let prompt_vendor = model_config.prompt_vendor.clone().or_else(|| {
            normalize_agent_prompt_vendor(None, model_config.provider.as_str())
                .map(|vendor| vendor.as_str().to_string())
        });

        let model_capability_revision = model_config
            .model_capability_revision
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| bad_request("model capability is not published"))?;
        let context_window_tokens = model_config
            .context_window_tokens
            .ok_or_else(|| bad_request("model capability is not published"))?;
        let output_reserve_tokens = model_config
            .output_reserve_tokens
            .ok_or_else(|| bad_request("model capability is not published"))?;
        let protocol_overhead_estimator_id = model_config
            .protocol_overhead_estimator_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| bad_request("model capability is not published"))?;

        Ok(Json(InternalModelRuntimeConfigResponse {
            id: model_config.id,
            owner_user_id: model_config.owner_user_id,
            name: model_config.name,
            provider: model_config.provider,
            prompt_vendor,
            base_url,
            api_key,
            model: model_config.model,
            thinking_level: model_config.thinking_level,
            temperature: model_config.temperature,
            max_output_tokens: model_config.max_output_tokens,
            model_capability_revision,
            context_window_tokens,
            output_reserve_tokens,
            protocol_overhead_estimator_id,
            tokenizer_id: model_config.tokenizer_id,
            responses_compaction_threshold: model_config.responses_compaction_threshold,
            supports_images: model_config.supports_images,
            supports_reasoning: model_config.supports_reasoning,
            supports_responses: model_config.supports_responses,
        }))
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!user_id.is_empty()).then_some(user_id.as_str()),
            project_id: None,
            resource_type: "user_model_runtime_config",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

pub async fn list_user_model_runtime_configs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<Vec<InternalModelRuntimeConfigResponse>> {
    let identity =
        require_user_model_internal_request(&state.config, &headers, MODEL_RUNTIME_READ_SCOPE)?;
    let user_id = user_id.trim().to_string();
    let result = async {
        if user_id.is_empty() {
            return Err(bad_request("user_id is required"));
        }
        if state
            .store
            .find_user_by_id(user_id.as_str())
            .await
            .map_err(internal_error)?
            .is_none()
        {
            return Err(not_found("user not found"));
        }

        let configs = state
            .store
            .list_user_model_configs(Some(user_id.as_str()))
            .await
            .map_err(internal_error)?;
        let providers = state
            .store
            .list_user_model_providers(Some(user_id.as_str()))
            .await
            .map_err(internal_error)?;
        let mut items = Vec::new();
        for model_config in configs {
            if !model_config.enabled
                || model_config.model.trim().is_empty()
                || !is_supported_provider(model_config.provider.as_str())
                || !model_config_has_backing_provider(&model_config, providers.as_slice())
                || !has_complete_model_capability(&model_config)
            {
                continue;
            }
            let Some(api_key) = model_config
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let Some(base_url) = model_config
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let prompt_vendor = model_config.prompt_vendor.clone().or_else(|| {
                normalize_agent_prompt_vendor(None, model_config.provider.as_str())
                    .map(|vendor| vendor.as_str().to_string())
            });
            items.push(InternalModelRuntimeConfigResponse {
                id: model_config.id,
                owner_user_id: model_config.owner_user_id,
                name: model_config.name,
                provider: model_config.provider,
                prompt_vendor,
                base_url,
                api_key,
                model: model_config.model,
                thinking_level: model_config.thinking_level,
                temperature: model_config.temperature,
                max_output_tokens: model_config.max_output_tokens,
                model_capability_revision: model_config
                    .model_capability_revision
                    .expect("validated capability revision"),
                context_window_tokens: model_config
                    .context_window_tokens
                    .expect("validated context window"),
                output_reserve_tokens: model_config
                    .output_reserve_tokens
                    .expect("validated output reserve"),
                protocol_overhead_estimator_id: model_config
                    .protocol_overhead_estimator_id
                    .expect("validated overhead estimator"),
                tokenizer_id: model_config.tokenizer_id,
                responses_compaction_threshold: model_config.responses_compaction_threshold,
                supports_images: model_config.supports_images,
                supports_reasoning: model_config.supports_reasoning,
                supports_responses: model_config.supports_responses,
            });
        }
        Ok(Json(items))
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!user_id.is_empty()).then_some(user_id.as_str()),
            project_id: None,
            resource_type: "user_model_runtime_catalog",
            resource_id: "all",
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

fn has_complete_model_capability(model: &crate::models::UserModelConfigRecord) -> bool {
    model
        .model_capability_revision
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && model.context_window_tokens.is_some_and(|value| value >= 2_048)
        && model.output_reserve_tokens.is_some_and(|value| value >= 256)
        && model
            .context_window_tokens
            .zip(model.output_reserve_tokens)
            .is_some_and(|(window, reserve)| reserve < window)
        && model
            .protocol_overhead_estimator_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && model.max_output_tokens.is_none_or(|max_output| {
            model
                .output_reserve_tokens
                .is_some_and(|reserve| max_output > 0 && max_output <= reserve)
        })
}
