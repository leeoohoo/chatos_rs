// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::ai_model_config_access::{
    ensure_owned_ai_model_config, map_ai_model_config_access_error,
};
use crate::core::auth::AuthUser;
use crate::repositories::ai_model_configs;
use crate::services::user_service_api_client;

use super::super::{AiModelConfigRequest, UserQuery};
use super::model::{
    build_model_config, from_user_service_model_config, to_response_value,
    to_response_value_with_secret, to_user_service_create_request, to_user_service_update_request,
};
use super::provider_models::fallback_model_list;
use super::user_service_proxy::{
    configured_user_service_base_url, proxy_status_from_user_service_error,
    user_service_access_token_for_auth, user_service_timeout_ms,
};

pub(in crate::api::configs) async fn list_ai_model_configs(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    if query
        .user_id
        .as_deref()
        .is_some_and(|value| value != auth.user_id.as_str())
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "user_id 与登录用户不一致"})),
        );
    }

    if let Some(base_url) = configured_user_service_base_url() {
        let access_token = match user_service_access_token_for_auth(&auth) {
            Ok(token) => token,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "build user_service access token failed", "detail": err})),
                );
            }
        };
        return match user_service_api_client::list_model_configs(
            base_url.as_str(),
            access_token.as_str(),
            Some(auth.user_id.as_str()),
            user_service_timeout_ms(),
        )
        .await
        {
            Ok(items) => (
                StatusCode::OK,
                Json(Value::Array(
                    items
                        .into_iter()
                        .map(from_user_service_model_config)
                        .map(|item| to_response_value(&item))
                        .collect(),
                )),
            ),
            Err(err) => (
                proxy_status_from_user_service_error(err.as_str()),
                Json(json!({
                    "error": "load ai model configs via user_service failed",
                    "detail": err
                })),
            ),
        };
    }

    match ai_model_configs::list_ai_model_configs(Some(auth.user_id.as_str())).await {
        Ok(items) => {
            let out = items
                .into_iter()
                .map(|item| to_response_value(&item))
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(in crate::api::configs) async fn get_ai_model_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let include_secret = query
        .get("include_secret")
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);

    if let Some(base_url) = configured_user_service_base_url() {
        let access_token = match user_service_access_token_for_auth(&auth) {
            Ok(token) => token,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "build user_service access token failed", "detail": err})),
                );
            }
        };
        return match user_service_api_client::get_model_config(
            base_url.as_str(),
            access_token.as_str(),
            config_id.as_str(),
            include_secret,
            user_service_timeout_ms(),
        )
        .await
        {
            Ok(item) => (
                StatusCode::OK,
                Json(to_response_value_with_secret(
                    &from_user_service_model_config(item),
                    include_secret,
                )),
            ),
            Err(err) => (
                proxy_status_from_user_service_error(err.as_str()),
                Json(json!({
                    "error": "load ai model config via user_service failed",
                    "detail": err
                })),
            ),
        };
    }

    let profile = match ensure_owned_ai_model_config(config_id.as_str(), &auth).await {
        Ok(item) => item,
        Err(err) => return map_ai_model_config_access_error(err),
    };
    (
        StatusCode::OK,
        Json(to_response_value_with_secret(&profile, include_secret)),
    )
}

pub(in crate::api::configs) async fn create_ai_model_config(
    auth: AuthUser,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        let access_token = match user_service_access_token_for_auth(&auth) {
            Ok(token) => token,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "build user_service access token failed", "detail": err})),
                );
            }
        };
        return match user_service_api_client::create_model_config(
            base_url.as_str(),
            access_token.as_str(),
            &to_user_service_create_request(&auth, req),
            user_service_timeout_ms(),
        )
        .await
        {
            Ok(item) => (
                StatusCode::CREATED,
                Json(to_response_value(&from_user_service_model_config(item))),
            ),
            Err(err) => (
                proxy_status_from_user_service_error(err.as_str()),
                Json(json!({
                    "error": "create ai model config via user_service failed",
                    "detail": err
                })),
            ),
        };
    }

    let id = req
        .id
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let config = match build_model_config(auth.user_id.clone(), id, req, None, true) {
        Ok(config) => config,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match ai_model_configs::create_ai_model_config(&config).await {
        Ok(item) => (StatusCode::CREATED, Json(to_response_value(&item))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(in crate::api::configs) async fn update_ai_model_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        let access_token = match user_service_access_token_for_auth(&auth) {
            Ok(token) => token,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "build user_service access token failed", "detail": err})),
                );
            }
        };
        return match user_service_api_client::update_model_config(
            base_url.as_str(),
            access_token.as_str(),
            config_id.as_str(),
            &to_user_service_update_request(req),
            user_service_timeout_ms(),
        )
        .await
        {
            Ok(item) => (
                StatusCode::OK,
                Json(to_response_value(&from_user_service_model_config(item))),
            ),
            Err(err) => (
                proxy_status_from_user_service_error(err.as_str()),
                Json(json!({
                    "error": "update ai model config via user_service failed",
                    "detail": err
                })),
            ),
        };
    }

    let existing = match ensure_owned_ai_model_config(&config_id, &auth).await {
        Ok(item) => item,
        Err(err) => return map_ai_model_config_access_error(err),
    };
    let merged_req = AiModelConfigRequest {
        id: Some(existing.id.clone()),
        name: req.name.or_else(|| Some(existing.name.clone())),
        provider: req.provider.or_else(|| Some(existing.provider.clone())),
        prompt_vendor: req.prompt_vendor.or_else(|| existing.prompt_vendor.clone()),
        model: req.model.or_else(|| Some(existing.model.clone())),
        thinking_level: if req.thinking_level.is_some() {
            req.thinking_level
        } else {
            existing.thinking_level.clone()
        },
        task_usage_scenario: if req.task_usage_scenario.is_some() {
            req.task_usage_scenario
        } else {
            existing.task_usage_scenario.clone()
        },
        task_thinking_level: if req.task_thinking_level.is_some() {
            req.task_thinking_level
        } else {
            existing.task_thinking_level.clone()
        },
        temperature: if req.clear_temperature.unwrap_or(false) {
            None
        } else {
            req.temperature.or(existing.temperature)
        },
        clear_temperature: None,
        max_output_tokens: if req.clear_max_output_tokens.unwrap_or(false) {
            None
        } else {
            req.max_output_tokens.or(existing.max_output_tokens)
        },
        clear_max_output_tokens: None,
        api_key: if req.clear_api_key.unwrap_or(false) {
            None
        } else {
            req.api_key.or_else(|| existing.api_key.clone())
        },
        clear_api_key: None,
        base_url: req.base_url.or_else(|| existing.base_url.clone()),
        enabled: req.enabled.or(Some(existing.enabled)),
        supports_images: req.supports_images.or(Some(existing.supports_images)),
        supports_reasoning: req.supports_reasoning.or(Some(existing.supports_reasoning)),
        supports_responses: req.supports_responses.or(Some(existing.supports_responses)),
    };
    let config = match build_model_config(
        auth.user_id.clone(),
        existing.id.clone(),
        merged_req,
        existing.api_key.clone(),
        false,
    ) {
        Ok(mut config) => {
            config.created_at = existing.created_at;
            config.updated_at = crate::core::time::now_rfc3339();
            config
        }
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match ai_model_configs::update_ai_model_config(config_id.as_str(), &config).await {
        Ok(()) => (StatusCode::OK, Json(to_response_value(&config))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(in crate::api::configs) async fn refresh_ai_model_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(base_url) = configured_user_service_base_url() else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "user_service is not configured"})),
        );
    };
    let access_token = match user_service_access_token_for_auth(&auth) {
        Ok(token) => token,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "build user_service access token failed", "detail": err})),
            );
        }
    };
    match user_service_api_client::refresh_model_config(
        base_url.as_str(),
        access_token.as_str(),
        config_id.as_str(),
        &to_user_service_update_request(req),
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(item) => (
            StatusCode::OK,
            Json(to_response_value(&from_user_service_model_config(item))),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "refresh ai model config via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn delete_ai_model_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        let access_token = match user_service_access_token_for_auth(&auth) {
            Ok(token) => token,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "build user_service access token failed", "detail": err})),
                );
            }
        };
        return match user_service_api_client::delete_model_config(
            base_url.as_str(),
            access_token.as_str(),
            config_id.as_str(),
            user_service_timeout_ms(),
        )
        .await
        {
            Ok(()) => (
                StatusCode::OK,
                Json(json!({"message": "AI 模型配置删除成功"})),
            ),
            Err(err) => (
                proxy_status_from_user_service_error(err.as_str()),
                Json(json!({
                    "error": "delete ai model config via user_service failed",
                    "detail": err
                })),
            ),
        };
    }

    if let Err(err) = ensure_owned_ai_model_config(&config_id, &auth).await {
        return map_ai_model_config_access_error(err);
    }
    match ai_model_configs::delete_ai_model_config(config_id.as_str()).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"message": "AI 模型配置删除成功"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(in crate::api::configs) async fn list_ai_provider_models(
    auth: AuthUser,
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        let access_token = match user_service_access_token_for_auth(&auth) {
            Ok(token) => token,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "build user_service access token failed", "detail": err})),
                );
            }
        };
        let profile = match user_service_api_client::get_model_config(
            base_url.as_str(),
            access_token.as_str(),
            config_id.as_str(),
            true,
            user_service_timeout_ms(),
        )
        .await
        {
            Ok(item) => from_user_service_model_config(item),
            Err(err) => {
                return (
                    proxy_status_from_user_service_error(err.as_str()),
                    Json(json!({
                        "error": "load ai model config via user_service failed",
                        "detail": err
                    })),
                );
            }
        };

        return (
            StatusCode::OK,
            Json(json!({
                "provider_config_id": profile.id,
                "provider": profile.provider,
                "base_url": Value::Null,
                "source": "local_connector_managed",
                "fetched_at": null,
                "models": fallback_model_list(&profile),
                "error": "model credentials are managed by Local Connector Client"
            })),
        );
    }

    let profile = match ensure_owned_ai_model_config(&config_id, &auth).await {
        Ok(item) => item,
        Err(err) => return map_ai_model_config_access_error(err),
    };

    (
        StatusCode::OK,
        Json(json!({
            "provider_config_id": profile.id,
            "provider": profile.provider,
            "base_url": Value::Null,
            "source": "local_connector_managed",
            "fetched_at": null,
            "models": fallback_model_list(&profile),
            "error": "model credentials are managed by Local Connector Client"
        })),
    )
}
