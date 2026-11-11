// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::models::{ComposeContextRequest, ComposeContextResponse};
use crate::services::context;
use crate::state::AppState;

use super::auth::SdkAuthContext;
use super::internal_error;
use super::requests::SdkComposeContextRequest;

pub async fn compose_context(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(mut req): Json<SdkComposeContextRequest>,
) -> Result<Json<ComposeContextResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let capability = state
        .config
        .model_capabilities
        .get(req.model_capability_revision.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "model_capability_revision is not published".to_string(),
            )
        })?;
    req.policy.max_memory_tokens = req.policy.max_memory_tokens.min(
        capability
            .context_window_tokens
            .saturating_sub(capability.output_reserve_tokens),
    );
    let partition = crate::api::runtime_context::partition_from_project_subject(
        req.context_project_subject_id.as_deref(),
    )?;
    crate::api::runtime_context::verify(
        &state,
        req.runtime_context_assertion.as_str(),
        req.tenant_id.as_str(),
        auth.source_id(),
        req.thread_id.as_str(),
        partition.as_str(),
        "compose",
    )
    .await?;
    let direct = ComposeContextRequest {
        tenant_id: req.tenant_id,
        source_id: auth.source_id().to_string(),
        subject_id: req.subject_id,
        context_project_subject_id: req.context_project_subject_id,
        thread_id: req.thread_id,
        query_text: req.query_text,
        current_turn_id: req.current_turn_id,
        model_capability_revision: req.model_capability_revision,
        runtime_context_assertion: req.runtime_context_assertion,
        policy: req.policy,
    };
    let mut response = context::compose_context(&state.pool, direct)
        .await
        .map_err(internal_error)?;
    response.meta.estimator_id = capability.protocol_overhead_estimator_id.clone();
    response.meta.tokenizer_id = capability.tokenizer_id.clone();
    state.runtime_stats.record_compose_context(&response);
    Ok(Json(response))
}
