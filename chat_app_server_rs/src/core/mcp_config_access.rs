use crate::core::auth::AuthUser;
use crate::models::mcp_config::McpConfig;
use crate::repositories::mcp_configs;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum McpConfigAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_mcp_config(config: &McpConfig, auth: &AuthUser) -> bool {
    config.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_mcp_config(
    config_id: &str,
    auth: &AuthUser,
) -> Result<McpConfig, McpConfigAccessError> {
    match mcp_configs::get_mcp_config_by_id(config_id).await {
        Ok(Some(config)) => {
            if is_owned_mcp_config(&config, auth) {
                Ok(config)
            } else {
                Err(McpConfigAccessError::Forbidden)
            }
        }
        Ok(None) => Err(McpConfigAccessError::NotFound),
        Err(err) => Err(McpConfigAccessError::Internal(err)),
    }
}

pub fn map_mcp_config_access_error(err: McpConfigAccessError) -> (StatusCode, Json<Value>) {
    match err {
        McpConfigAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "MCP配置不存在"})),
        ),
        McpConfigAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该 MCP 配置"})),
        ),
        McpConfigAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
