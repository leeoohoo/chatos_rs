// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::user_service_api_client;

use super::super::{AiModelSettingsRequest, UserQuery};
use super::model::model_settings_response_value;
use super::user_service_proxy::{
    configured_user_service_base_url, proxy_status_from_user_service_error,
    user_service_access_token_for_auth, user_service_timeout_ms,
};

pub(in crate::api::configs) async fn get_ai_model_settings(
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
    match user_service_api_client::get_model_settings(
        base_url.as_str(),
        access_token.as_str(),
        Some(auth.user_id.as_str()),
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(settings) => (
            StatusCode::OK,
            Json(model_settings_response_value(settings)),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "load ai model settings via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn put_ai_model_settings(
    auth: AuthUser,
    Json(req): Json<AiModelSettingsRequest>,
) -> (StatusCode, Json<Value>) {
    if req
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
    let payload = user_service_api_client::UpdateUserServiceModelSettingsRequest {
        user_id: Some(auth.user_id.clone()),
        memory_summary_model_config_id: normalize_settings_update_value(
            req.memory_summary_model_config_id,
        ),
        memory_summary_thinking_level: normalize_settings_update_value(
            req.memory_summary_thinking_level,
        ),
        project_management_agent_model_config_id: normalize_settings_update_value(
            req.project_management_agent_model_config_id,
        ),
        project_management_agent_thinking_level: normalize_settings_update_value(
            req.project_management_agent_thinking_level,
        ),
    };
    match user_service_api_client::update_model_settings(
        base_url.as_str(),
        access_token.as_str(),
        &payload,
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(settings) => (
            StatusCode::OK,
            Json(model_settings_response_value(settings)),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "update ai model settings via user_service failed",
                "detail": err
            })),
        ),
    }
}

fn normalize_settings_update_value(value: Option<Option<String>>) -> Option<Option<String>> {
    value.map(|inner| {
        inner
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    })
}
