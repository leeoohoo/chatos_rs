use axum::extract::State;
use axum::Json;

use crate::models::{HealthResponse, SystemConfigResponse};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::ApiResult;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "user_service_backend".to_string(),
        now: now_rfc3339(),
    })
}

pub async fn get_system_config(State(state): State<AppState>) -> ApiResult<SystemConfigResponse> {
    Ok(Json(SystemConfigResponse {
        service: "user_service_backend".to_string(),
        issuer: state.config.jwt_issuer.clone(),
        user_service_audience: state.config.user_service_audience.clone(),
        task_runner_audience: state.config.task_runner_audience.clone(),
        database_url: state.config.database_url.clone(),
        user_access_ttl_seconds: state.config.user_access_ttl_seconds,
        task_runner_access_ttl_seconds: state.config.task_runner_access_ttl_seconds,
    }))
}
