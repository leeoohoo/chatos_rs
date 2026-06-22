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
    Json(req): Json<SdkComposeContextRequest>,
) -> Result<Json<ComposeContextResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let direct = ComposeContextRequest {
        tenant_id: req.tenant_id,
        source_id: auth.source_id().to_string(),
        subject_id: req.subject_id,
        related_subject_ids: req.related_subject_ids,
        thread_id: req.thread_id,
        policy: req.policy,
    };
    context::compose_context(&state.pool, direct)
        .await
        .map(Json)
        .map_err(internal_error)
}
