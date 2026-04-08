use crate::core::auth::AuthUser;
use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::ai_model_configs;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum AiModelAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_ai_model(config: &AiModelConfig, auth: &AuthUser) -> bool {
    config.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_ai_model(
    config_id: &str,
    auth: &AuthUser,
) -> Result<AiModelConfig, AiModelAccessError> {
    match ai_model_configs::get_ai_model_config_by_id(config_id).await {
        Ok(Some(config)) => {
            if is_owned_ai_model(&config, auth) {
                Ok(config)
            } else {
                Err(AiModelAccessError::Forbidden)
            }
        }
        Ok(None) => Err(AiModelAccessError::NotFound),
        Err(err) => Err(AiModelAccessError::Internal(err)),
    }
}

pub fn map_ai_model_access_error(err: AiModelAccessError) -> (StatusCode, Json<Value>) {
    match err {
        AiModelAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "AI模型配置不存在"})),
        ),
        AiModelAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该 AI 模型配置"})),
        ),
        AiModelAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
