// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;

use crate::services::runtime_context_assertion::{self, ExpectedRuntimeContextScope};
use crate::state::AppState;

pub async fn verify(
    state: &AppState,
    token: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    operation: &str,
) -> Result<memory_engine_sdk::RuntimeContextAssertionClaims, (StatusCode, String)> {
    runtime_context_assertion::verify_and_consume(
        &state.pool,
        &state.config,
        token,
        ExpectedRuntimeContextScope {
            tenant_id,
            source_id,
            thread_id,
            project_partition_key,
            operation,
        },
    )
    .await
    .map_err(|message| (StatusCode::FORBIDDEN, message))
}

pub fn partition_from_project_subject(
    project_subject_id: Option<&str>,
) -> Result<String, (StatusCode, String)> {
    let Some(value) = project_subject_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok("__none__".to_string());
    };
    let project = value.strip_prefix("project:").unwrap_or(value);
    if project.is_empty() || project.chars().any(char::is_control) {
        return Err((
            StatusCode::BAD_REQUEST,
            "context_project_subject_id is invalid".to_string(),
        ));
    }
    Ok(format!("project:{project}"))
}
