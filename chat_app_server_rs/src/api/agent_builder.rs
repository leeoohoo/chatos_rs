use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::services::memory_server_client;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiCreateAgentRequest {
    user_id: Option<String>,
    requirement: Option<String>,
    name: Option<String>,
    category: Option<String>,
    description: Option<String>,
    role_definition: Option<String>,
    skill_ids: Option<Vec<String>>,
    skill_prompts: Option<Vec<String>>,
    enabled: Option<bool>,
    mcp_enabled: Option<bool>,
    enabled_mcp_ids: Option<Vec<String>>,
    project_id: Option<String>,
    project_root: Option<String>,
}

pub fn router() -> Router {
    Router::new().route("/api/agent-builder/ai-create", post(ai_create))
}

async fn ai_create(
    auth: AuthUser,
    Json(req): Json<AiCreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    let requirement = req
        .requirement
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if requirement.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "requirement is required"})),
        );
    }

    let mut payload = match serde_json::to_value(req) {
        Ok(Value::Object(map)) => map,
        Ok(_) => serde_json::Map::new(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid request body", "detail": err.to_string()})),
            )
        }
    };
    payload.insert("user_id".to_string(), Value::String(user_id));

    match memory_server_client::ai_create_memory_agent(&Value::Object(payload)).await {
        Ok(result) => (StatusCode::OK, Json(result)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "ai-create memory agent failed", "detail": err})),
        ),
    }
}
