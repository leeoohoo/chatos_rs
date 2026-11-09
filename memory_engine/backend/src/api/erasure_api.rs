// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use crate::repositories::erasure::{self, PrivilegedThreadErasureResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PrivilegedThreadErasureRequest {
    tenant_id: String,
    source_id: String,
    thread_id: String,
    reason_code: String,
    approval_id: String,
}

pub async fn erase_thread(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PrivilegedThreadErasureRequest>,
) -> Result<Json<PrivilegedThreadErasureResult>, (StatusCode, String)> {
    if req.approval_id.trim().len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "approval_id must contain at least 8 characters".to_string(),
        ));
    }
    erasure::erase_thread(
        &state.pool,
        req.tenant_id.trim(),
        req.source_id.trim(),
        req.thread_id.trim(),
        req.reason_code.trim(),
    )
    .await
    .map(Json)
    .map_err(|message| (StatusCode::BAD_REQUEST, message))
}
