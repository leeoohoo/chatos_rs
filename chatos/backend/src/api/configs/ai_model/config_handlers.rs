// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::user_service_api_client;

use super::super::{AiModelConfigRequest, UserQuery};
use super::model::{
    from_user_service_model_config, to_response_value, to_response_value_with_secret,
    to_user_service_create_request, to_user_service_update_request,
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
    match user_service_api_client::list_model_configs(
        base_url.as_str(),
        access_token.as_str(),
        None,
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
    match user_service_api_client::get_model_config(
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
    }
}

pub(in crate::api::configs) async fn create_ai_model_config(
    auth: AuthUser,
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
    match user_service_api_client::create_model_config(
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
    }
}

pub(in crate::api::configs) async fn update_ai_model_config(
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
    match user_service_api_client::update_model_config(
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
    match user_service_api_client::delete_model_config(
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
    }
}

pub(in crate::api::configs) async fn list_ai_provider_models(
    auth: AuthUser,
    Path(config_id): Path<String>,
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
