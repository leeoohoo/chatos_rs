// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chatos_local_agent_protocol::ModelRuntimeDescriptor;
use serde_json::{json, Value};
use tracing::warn;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::services::user_service_api_client::{self, UserServiceInternalModelRuntimeRecord};

type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn router() -> Router {
    Router::new().route(
        "/api/model-gateway/descriptors/{model_config_id}",
        get(get_model_runtime_descriptor),
    )
}

async fn get_model_runtime_descriptor(
    Path(model_config_id): Path<String>,
    auth: AuthUser,
) -> ApiResult<ModelRuntimeDescriptor> {
    let model_config_id = model_config_id.trim();
    if model_config_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "model_config_id is required",
        ));
    }

    let config = Config::try_get().map_err(|error| {
        warn!(error = %error, "model_gateway.descriptor.config_unavailable");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "model gateway configuration is unavailable",
        )
    })?;
    let internal_secret = config
        .user_service_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "model gateway service identity is unavailable",
            )
        })?;

    let runtime = user_service_api_client::get_internal_model_runtime_config(
        &config.user_service_internal_http_client,
        config.user_service_internal_base_url.as_str(),
        internal_secret,
        auth.user_id.as_str(),
        model_config_id,
    )
    .await
    .map_err(|error| map_runtime_service_error(model_config_id, &error))?;

    validate_runtime_identity(&runtime, auth.user_id.as_str(), model_config_id).map_err(
        |detail| {
            warn!(
                model_config_id,
                detail, "model_gateway.descriptor.identity_mismatch"
            );
            api_error(
                StatusCode::BAD_GATEWAY,
                "model runtime configuration identity is invalid",
            )
        },
    )?;

    let descriptor = descriptor_from_runtime(&runtime).map_err(|detail| {
        warn!(model_config_id, detail = %detail, "model_gateway.descriptor.invalid_config");
        api_error(StatusCode::UNPROCESSABLE_ENTITY, detail)
    })?;
    Ok(Json(descriptor))
}

fn validate_runtime_identity(
    runtime: &UserServiceInternalModelRuntimeRecord,
    expected_user_id: &str,
    expected_model_config_id: &str,
) -> Result<(), &'static str> {
    if runtime.owner_user_id != expected_user_id {
        return Err("runtime owner does not match authenticated user");
    }
    if runtime.id != expected_model_config_id {
        return Err("runtime model config id does not match requested id");
    }
    Ok(())
}

fn descriptor_from_runtime(
    runtime: &UserServiceInternalModelRuntimeRecord,
) -> Result<ModelRuntimeDescriptor, &'static str> {
    let protocol = runtime
        .protocol
        .ok_or("model configuration is missing protocol")?;
    let context_strategy = runtime
        .context_strategy
        .ok_or("model configuration is missing context_strategy")?;
    let context_window_tokens = runtime
        .context_window_tokens
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("model configuration is missing a positive context_window_tokens")?;
    let maximum_output_tokens = runtime
        .max_output_tokens
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("model configuration is missing a valid max_output_tokens")?;

    let descriptor = ModelRuntimeDescriptor {
        model_config_id: runtime.id.clone(),
        revision: runtime.revision,
        provider: runtime.provider.clone(),
        model: runtime.model.clone(),
        protocol,
        context_window_tokens,
        maximum_output_tokens,
        context_strategy,
        supports_streaming: runtime.supports_streaming,
        supports_native_compaction: runtime.supports_native_compaction,
        supports_input_token_count: runtime.supports_input_token_count,
    };
    descriptor
        .validate()
        .map_err(|_| "model configuration cannot produce a valid runtime descriptor")?;
    Ok(descriptor)
}

fn map_runtime_service_error(model_config_id: &str, error: &str) -> ApiError {
    let status = match user_service_api_client::response_status_from_error(error) {
        Some(400) => StatusCode::UNPROCESSABLE_ENTITY,
        Some(401) => StatusCode::UNAUTHORIZED,
        Some(403) => StatusCode::FORBIDDEN,
        Some(404) => StatusCode::NOT_FOUND,
        Some(409) => StatusCode::CONFLICT,
        _ => StatusCode::BAD_GATEWAY,
    };
    warn!(model_config_id, status = %status, error = %error, "model_gateway.descriptor.user_service_failed");
    let message = if status == StatusCode::NOT_FOUND {
        "model configuration was not found"
    } else if status == StatusCode::FORBIDDEN {
        "model configuration does not belong to the current user"
    } else if status == StatusCode::UNPROCESSABLE_ENTITY {
        "model configuration is not ready for local agent execution"
    } else {
        "model runtime configuration could not be loaded"
    };
    api_error(status, message)
}

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

#[cfg(test)]
mod tests {
    use super::{descriptor_from_runtime, router, validate_runtime_identity};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chatos_local_agent_protocol::{ContextStrategy, ModelProtocol};
    use tower::ServiceExt;

    use crate::services::user_service_api_client::UserServiceInternalModelRuntimeRecord;

    fn complete_runtime() -> UserServiceInternalModelRuntimeRecord {
        UserServiceInternalModelRuntimeRecord {
            id: "model-1".to_string(),
            revision: 7,
            owner_user_id: "user-1".to_string(),
            name: "Primary".to_string(),
            provider: "openai".to_string(),
            protocol: Some(ModelProtocol::Responses),
            context_strategy: Some(ContextStrategy::ProviderNative),
            prompt_vendor: Some("gpt".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "secret-key-must-not-leak".to_string(),
            model: "gpt-test".to_string(),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.2),
            context_window_tokens: Some(400_000),
            max_output_tokens: Some(32_000),
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
            supports_streaming: true,
            supports_native_compaction: true,
            supports_input_token_count: true,
        }
    }

    #[test]
    fn descriptor_contains_only_frozen_non_secret_runtime_data() {
        let descriptor = descriptor_from_runtime(&complete_runtime()).expect("descriptor");
        let value = serde_json::to_value(descriptor).expect("serialize descriptor");
        let serialized = value.to_string();
        assert_eq!(value["revision"], 7);
        assert_eq!(value["protocol"], "responses");
        assert_eq!(value["context_strategy"], "provider_native");
        assert!(!serialized.contains("secret-key-must-not-leak"));
        assert!(!serialized.contains("api.openai.com"));
    }

    #[test]
    fn descriptor_rejects_incomplete_model_metadata() {
        let mut runtime = complete_runtime();
        runtime.context_window_tokens = None;
        assert_eq!(
            descriptor_from_runtime(&runtime),
            Err("model configuration is missing a positive context_window_tokens")
        );
    }

    #[test]
    fn runtime_identity_must_match_authenticated_request() {
        let runtime = complete_runtime();
        assert!(validate_runtime_identity(&runtime, "user-1", "model-1").is_ok());
        assert!(validate_runtime_identity(&runtime, "other-user", "model-1").is_err());
        assert!(validate_runtime_identity(&runtime, "user-1", "other-model").is_err());
    }

    #[tokio::test]
    async fn descriptor_route_requires_authentication() {
        let response = router()
            .oneshot(
                Request::get("/api/model-gateway/descriptors/model-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
