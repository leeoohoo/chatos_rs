// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::state::AppState;

use super::internal_audit::MemoryInternalRequestAudit;
use super::internal_auth::{require_internal_request, OPERATOR_SCOPE};

pub async fn require_operator_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let claims = require_internal_request(
        &state.config,
        request.headers(),
        OPERATOR_SCOPE,
        &["chatos-backend", "task-runner", "configuration-center"],
    )?
    .ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "signed Memory Engine internal API token is required".to_string(),
        )
    })?;
    let audit = MemoryInternalRequestAudit::from_request(&request, &claims);
    request.extensions_mut().insert(claims);
    let response = next.run(request).await;
    audit.record(response.status());
    Ok(response)
}
