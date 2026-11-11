// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::repositories::history_migrations::{self, HistoryMigrationReport};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RunHistoryRepairRequest {
    tenant_id: String,
    source_id: String,
    reason: String,
}

pub async fn run_m2a_history_repair(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<chatos_service_runtime::InternalServiceTokenClaims>,
    Json(request): Json<RunHistoryRepairRequest>,
) -> Result<Json<HistoryMigrationReport>, (StatusCode, String)> {
    if request.reason.trim().len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "reason must contain at least 8 characters".to_string(),
        ));
    }
    let operator_id = format!("{}:{}", identity.caller, identity.sub);
    history_migrations::run_m2a_history_repair(
        &state.pool,
        request.tenant_id.trim(),
        request.source_id.trim(),
        operator_id.as_str(),
        request.reason.trim(),
    )
    .await
    .map(Json)
    .map_err(|message| (StatusCode::BAD_REQUEST, message))
}
