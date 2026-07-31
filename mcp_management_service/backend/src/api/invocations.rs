// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::require_internal_request;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) async fn cancel_runtime_invocation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invocation_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let caller_service =
        require_internal_request(&state.config, &headers, "runtime.invocations.cancel")?;
    let invocation_id = invocation_id.trim();
    if invocation_id.is_empty() || invocation_id.len() > 160 {
        return Err(ApiError::bad_request("invocation id is invalid"));
    }
    let record = state
        .runtime_invocations
        .request_cancel_by_invocation(invocation_id, caller_service.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime invocation was not found or has expired"))?;
    Ok(Json(json!({
        "invocation_id": record.invocation_id,
        "status": super::mcp::cancel_response_status(&record),
    })))
}
