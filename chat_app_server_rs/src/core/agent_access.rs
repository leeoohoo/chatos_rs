use crate::core::auth::AuthUser;
use crate::models::agent::Agent;
use crate::repositories::agents;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum AgentAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_agent(agent: &Agent, auth: &AuthUser) -> bool {
    agent.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_agent(
    agent_id: &str,
    auth: &AuthUser,
) -> Result<Agent, AgentAccessError> {
    match agents::get_agent_by_id(agent_id).await {
        Ok(Some(agent)) => {
            if is_owned_agent(&agent, auth) {
                Ok(agent)
            } else {
                Err(AgentAccessError::Forbidden)
            }
        }
        Ok(None) => Err(AgentAccessError::NotFound),
        Err(err) => Err(AgentAccessError::Internal(err)),
    }
}

pub fn map_agent_access_error(err: AgentAccessError) -> (StatusCode, Json<Value>) {
    match err {
        AgentAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Agent 不存在"})),
        ),
        AgentAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该智能体"})),
        ),
        AgentAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
