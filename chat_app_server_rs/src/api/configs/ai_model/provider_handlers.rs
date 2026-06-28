use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::user_service_api_client;

use super::super::{AiModelConfigRequest, UserQuery};
use super::model::{
    model_provider_response_value, to_user_service_create_provider_request,
    to_user_service_update_provider_request,
};
use super::user_service_proxy::{
    configured_user_service_base_url, proxy_status_from_user_service_error,
    user_service_access_token_for_auth, user_service_timeout_ms,
};

pub(in crate::api::configs) async fn list_ai_model_providers(
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
    match user_service_api_client::list_model_providers(
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
                    .map(|item| model_provider_response_value(item, false))
                    .collect(),
            )),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "load ai model providers via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn get_ai_model_provider(
    auth: AuthUser,
    Path(provider_id): Path<String>,
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
    match user_service_api_client::get_model_provider(
        base_url.as_str(),
        access_token.as_str(),
        provider_id.as_str(),
        include_secret,
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(item) => (
            StatusCode::OK,
            Json(model_provider_response_value(item, include_secret)),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "load ai model provider via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn create_ai_model_provider(
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
    match user_service_api_client::create_model_provider(
        base_url.as_str(),
        access_token.as_str(),
        &to_user_service_create_provider_request(&auth, req),
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(item) => (
            StatusCode::CREATED,
            Json(model_provider_response_value(item, false)),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "create ai model provider via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn update_ai_model_provider(
    auth: AuthUser,
    Path(provider_id): Path<String>,
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
    match user_service_api_client::update_model_provider(
        base_url.as_str(),
        access_token.as_str(),
        provider_id.as_str(),
        &to_user_service_update_provider_request(req),
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(item) => (
            StatusCode::OK,
            Json(model_provider_response_value(item, false)),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "update ai model provider via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn refresh_ai_model_provider(
    auth: AuthUser,
    Path(provider_id): Path<String>,
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
    match user_service_api_client::refresh_model_provider(
        base_url.as_str(),
        access_token.as_str(),
        provider_id.as_str(),
        &to_user_service_update_provider_request(req),
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(item) => (
            StatusCode::OK,
            Json(model_provider_response_value(item, false)),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "refresh ai model provider via user_service failed",
                "detail": err
            })),
        ),
    }
}

pub(in crate::api::configs) async fn delete_ai_model_provider(
    auth: AuthUser,
    Path(provider_id): Path<String>,
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
    match user_service_api_client::delete_model_provider(
        base_url.as_str(),
        access_token.as_str(),
        provider_id.as_str(),
        user_service_timeout_ms(),
    )
    .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"message": "AI provider deleted"})),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "delete ai model provider via user_service failed",
                "detail": err
            })),
        ),
    }
}
