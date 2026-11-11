// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use crate::models::CurrentUser;
use crate::state::AppState;

use super::ApiError;

const STORY_STUDIO_RUNTIME_KIND: &str = "story_studio";
const STORY_STUDIO_SOURCE_ID: &str = "chatos";

#[derive(Debug, Deserialize)]
pub(super) struct IssueRuntimeContextAssertionRequest {
    runtime_kind: String,
    project_id: Option<String>,
    runtime_id: String,
    operation: String,
    source_id: Option<String>,
    canonical_agent_subject_id: Option<String>,
    thread_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct IssueRuntimeContextAssertionResponse {
    runtime_context_assertion: String,
    source_id: String,
    canonical_agent_subject_id: String,
    thread_id: String,
    project_partition_key: String,
    runtime_identity_revision: String,
}

pub(super) async fn issue_runtime_context_assertion(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<IssueRuntimeContextAssertionRequest>,
) -> Result<Json<IssueRuntimeContextAssertionResponse>, ApiError> {
    issue_for_runtime(&state.config.runtime_context_assertion_issuer, &user, req)
}

fn issue_for_runtime(
    issuer: &memory_engine_sdk::RuntimeContextAssertionIssuer,
    user: &CurrentUser,
    req: IssueRuntimeContextAssertionRequest,
) -> Result<Json<IssueRuntimeContextAssertionResponse>, ApiError> {
    let runtime_kind = required_identity(&req.runtime_kind, "runtime_kind")?;
    let runtime_id = required_identity(&req.runtime_id, "runtime_id")?;
    let operation = required_identity(&req.operation, "operation")?;
    if !matches!(
        operation,
        "record_ingest" | "compose" | "summarize" | "repair"
    ) {
        return Err(ApiError::bad_request(
            "unsupported Memory runtime assertion operation",
        ));
    }
    let owner_user_id = user.effective_owner_user_id();
    let project_id = req
        .project_id
        .as_deref()
        .map(|value| required_identity(value, "project_id"))
        .transpose()?;
    let (source_id, canonical_agent_subject_id, thread_id) =
        if runtime_kind == STORY_STUDIO_RUNTIME_KIND {
            let project_id = project_id.ok_or_else(|| {
                ApiError::bad_request("Story Studio Memory runtime requires project_id")
            })?;
            (
                STORY_STUDIO_SOURCE_ID.to_string(),
                format!("agent:story-studio:{project_id}"),
                format!("client-agent:story:{project_id}:{runtime_id}"),
            )
        } else if runtime_kind == "local_agent" {
            // Native projects are authoritative in the local owner-scoped registry and
            // do not create cloud bindings. Their Memory partition belongs to the
            // authenticated owner's tenant, even if another owner uses the same project ID.
            let source_id = required_optional_identity(req.source_id.as_deref(), "source_id")?;
            let canonical_agent_subject_id = required_optional_identity(
                req.canonical_agent_subject_id.as_deref(),
                "canonical_agent_subject_id",
            )?;
            if !canonical_agent_subject_id.starts_with("agent:") {
                return Err(ApiError::bad_request(
                    "Local Agent Memory identity must use a canonical agent subject",
                ));
            }
            let thread_id = required_optional_identity(req.thread_id.as_deref(), "thread_id")?;
            if runtime_id != thread_id {
                return Err(ApiError::bad_request(
                    "Local Agent Memory runtime_id must equal thread_id",
                ));
            }
            (
                source_id.to_string(),
                canonical_agent_subject_id.to_string(),
                thread_id.to_string(),
            )
        } else {
            return Err(ApiError::bad_request("unsupported Memory runtime_kind"));
        };
    let project_partition_key = project_id
        .map(|project_id| format!("project:{project_id}"))
        .unwrap_or_else(|| "__none__".to_string());
    let assertion = issuer
        .issue(
            owner_user_id,
            owner_user_id,
            source_id.as_str(),
            canonical_agent_subject_id.as_str(),
            thread_id.as_str(),
            project_partition_key.as_str(),
            &[operation],
        )
        .map_err(ApiError::internal)?;

    Ok(Json(IssueRuntimeContextAssertionResponse {
        runtime_context_assertion: assertion,
        source_id,
        canonical_agent_subject_id,
        thread_id,
        project_partition_key,
        runtime_identity_revision: issuer.runtime_identity_revision().to_string(),
    }))
}

#[cfg(test)]
#[path = "runtime_context_assertions_tests.rs"]
mod tests;

fn required_optional_identity<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<&'a str, ApiError> {
    required_identity(
        value.ok_or_else(|| {
            ApiError::bad_request(format!("Memory runtime assertion {field} is required"))
        })?,
        field,
    )
}

fn required_identity<'a>(value: &'a str, field: &str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request(format!(
            "Memory runtime assertion {field} is invalid"
        )));
    }
    Ok(value)
}
