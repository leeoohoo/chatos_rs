// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{ResolveMcpRoutesRequest, ResolveMcpRoutesResponse};

use crate::auth::require_internal_request;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) async fn resolve_routes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ResolveMcpRoutesRequest>,
) -> Result<Json<ResolveMcpRoutesResponse>, ApiError> {
    require_internal_request(&state.config, &headers, "routes.resolve")?;
    if request.context.project_id.trim().is_empty() {
        return Err(ApiError::bad_request("project_id is required"));
    }
    if request.context.owner_user_id.trim().is_empty() {
        return Err(ApiError::bad_request("owner_user_id is required"));
    }
    if request.context.revision.trim().is_empty() {
        return Err(ApiError::bad_request(
            "project context revision is required",
        ));
    }
    Ok(Json(state.routing.resolve(request)))
}
