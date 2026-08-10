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
use crate::models::project::PUBLIC_PROJECT_ID;
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
    #[serde(default)]
    plan_mode: bool,
}

async fn list_available_plugins(
    _auth: AuthUser,
    Query(query): Query<AvailablePluginsQuery>,
) -> (StatusCode, Json<Value>) {
    let project_id = match concrete_project_id(query.project_id.as_deref()) {
        Ok(project_id) => project_id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
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
        query.plan_mode,
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

fn concrete_project_id(value: Option<&str>) -> Result<&str, &'static str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "0" && *value != PUBLIC_PROJECT_ID)
        .ok_or("a concrete project_id is required for Plugin discovery")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_discovery_requires_a_concrete_project() {
        assert_eq!(concrete_project_id(Some(" project-1 ")), Ok("project-1"));
        assert!(concrete_project_id(None).is_err());
        assert!(concrete_project_id(Some("0")).is_err());
        assert!(concrete_project_id(Some(PUBLIC_PROJECT_ID)).is_err());
    }
}
