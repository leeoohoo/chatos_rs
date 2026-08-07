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
    device_id: Option<String>,
    runtime_provider: String,
    #[serde(default)]
    plan_mode: bool,
}

async fn list_available_plugins(
    _auth: AuthUser,
    Query(query): Query<AvailablePluginsQuery>,
) -> (StatusCode, Json<Value>) {
    let runtime_provider = query.runtime_provider.trim().to_ascii_lowercase();
    if !matches!(runtime_provider.as_str(), "cloud" | "local_connector") {
        return error(
            StatusCode::BAD_REQUEST,
            "runtime_provider must be cloud or local_connector",
        );
    }
    let device_id = query
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if runtime_provider == "cloud" && device_id.is_some() {
        return error(
            StatusCode::BAD_REQUEST,
            "cloud Plugin discovery must not include a Local Connector device",
        );
    }
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
        runtime_provider.as_str(),
        device_id,
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
