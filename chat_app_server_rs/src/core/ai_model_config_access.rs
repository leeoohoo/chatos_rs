use crate::core::auth::AuthUser;
use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::ai_model_configs;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum AiModelConfigAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_ai_model_config(config: &AiModelConfig, auth: &AuthUser) -> bool {
    config.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_ai_model_config(
    config_id: &str,
    auth: &AuthUser,
) -> Result<AiModelConfig, AiModelConfigAccessError> {
    match ai_model_configs::get_ai_model_config_by_id(config_id).await {
        Ok(Some(config)) => {
            if is_owned_ai_model_config(&config, auth) {
                Ok(config)
            } else {
                Err(AiModelConfigAccessError::Forbidden)
            }
        }
        Ok(None) => Err(AiModelConfigAccessError::NotFound),
        Err(err) => Err(AiModelConfigAccessError::Internal(err)),
    }
}

pub fn map_ai_model_config_access_error(
    err: AiModelConfigAccessError,
) -> (StatusCode, Json<Value>) {
    match err {
        AiModelConfigAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "AI 模型配置不存在"})),
        ),
        AiModelConfigAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该 AI 模型配置"})),
        ),
        AiModelConfigAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
