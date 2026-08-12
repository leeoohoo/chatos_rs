// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{FinalizeRuntimeRunRequest, FinalizeRuntimeRunResponse};

use crate::auth::require_internal_request_identity;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) async fn finalize_runtime_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FinalizeRuntimeRunRequest>,
) -> Result<Json<FinalizeRuntimeRunResponse>, ApiError> {
    let identity =
        require_internal_request_identity(&state.config, &headers, "runtime.runs.finalize")?;
    identity.require_signed_trace_id()?;
    if identity.caller != "task-runner" {
        return Err(ApiError::forbidden(
            "only Task Runner can finalize a runtime run",
        ));
    }
    let owner_user_id = required_text(request.owner_user_id.as_str(), "owner_user_id")?;
    let project_id = required_text(request.project_id.as_str(), "project_id")?;
    let run_id = required_text(request.run_id.as_str(), "run_id")?;
    let project_context = state
        .project_context_client
        .resolve(project_id, owner_user_id)
        .await
        .map_err(ApiError::bad_gateway)?;
    let generation = state
        .runtime_execution_scopes
        .finalize_run(
            owner_user_id,
            project_id,
            run_id,
            project_context.workspace_provider,
            request.status,
        )
        .await
        .map_err(ApiError::internal)?;
    let sessions = state
        .runtime_sessions
        .remove_run_sessions(owner_user_id, project_id, run_id)
        .await
        .map_err(ApiError::internal)?;
    for snapshot in sessions {
        state
            .runtime_invocations
            .close_session(snapshot.session_id.as_str())
            .await
            .map_err(ApiError::internal)?;
        state.providers.close_session(&snapshot).await;
        state
            .runtime_execution_scopes
            .detach_session(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_str(),
                run_id,
                snapshot.project_context.workspace_provider,
                snapshot.session_id.as_str(),
            )
            .await
            .map_err(ApiError::internal)?;
    }
    state
        .providers
        .finalize_run(
            &project_context,
            owner_user_id,
            project_id,
            run_id,
            generation,
            request.status,
        )
        .await
        .map_err(|error| ApiError::bad_gateway(error.message))?;
    Ok(Json(FinalizeRuntimeRunResponse {
        run_id: run_id.to_string(),
        finalized: true,
    }))
}

fn required_text<'a>(value: &'a str, field: &str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request(format!("{field} is required")));
    }
    Ok(value)
}
