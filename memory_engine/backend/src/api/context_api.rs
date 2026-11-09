// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, Json};

use super::{memory_auth::MemoryAuthContext, runtime_context, source_guard};
use crate::models::{ComposeContextRequest, ComposeContextResponse};
use crate::services::context;
use crate::state::AppState;

pub async fn compose_context(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(mut req): Json<ComposeContextRequest>,
) -> Result<Json<ComposeContextResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    let capability = state
        .config
        .model_capabilities
        .get(req.model_capability_revision.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "model_capability_revision is not published".to_string(),
            )
        })?;
    req.policy.max_memory_tokens = req.policy.max_memory_tokens.min(
        capability
            .context_window_tokens
            .saturating_sub(capability.output_reserve_tokens),
    );
    let partition =
        runtime_context::partition_from_project_subject(req.context_project_subject_id.as_deref())?;
    runtime_context::verify(
        &state,
        req.runtime_context_assertion.as_str(),
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.thread_id.as_str(),
        partition.as_str(),
        "compose",
    )
    .await?;
    let mut response = context::compose_context(&state.pool, req)
        .await
        .map_err(internal_error)?;
    response.meta.estimator_id = capability.protocol_overhead_estimator_id.clone();
    response.meta.tokenizer_id = capability.tokenizer_id.clone();
    state.runtime_stats.record_compose_context(&response);
    Ok(Json(response))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
