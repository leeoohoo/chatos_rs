// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::services::{access_token_scope, task_runner_api_client};

pub fn router() -> Router {
    Router::new().route(
        "/api/task-runner/available-plugins",
        get(list_available_plugins),
    )
}

#[derive(Debug, Deserialize)]
struct AvailablePluginsQuery {
    project_id: Option<String>,
    project_context: Option<String>,
}

async fn list_available_plugins(
    _auth: AuthUser,
    Query(query): Query<AvailablePluginsQuery>,
) -> (StatusCode, Json<Value>) {
    let project_id = query
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let project_context = match query.project_context.as_deref() {
        Some(value) => match serde_json::from_str::<
            chatos_mcp_management_sdk::ClientProjectContextSnapshot,
        >(value)
        {
            Ok(snapshot) => Some(snapshot),
            Err(error_message) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    format!("invalid project_context: {error_message}"),
                )
            }
        },
        None => None,
    };
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return error(
            StatusCode::UNAUTHORIZED,
            "current user access token is unavailable",
        );
    };
    let config = match Config::try_get() {
        Ok(value) => value,
        Err(error_message) => return error(StatusCode::SERVICE_UNAVAILABLE, error_message),
    };
    match task_runner_api_client::list_task_runner_available_plugins(
        config.task_runner_base_url.as_str(),
        access_token.as_str(),
        project_id,
        project_context.as_ref(),
    )
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(error_message) => error(StatusCode::BAD_GATEWAY, error_message),
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn plugin_discovery_supports_user_conversation_scope() {}
}
