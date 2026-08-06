// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::require_internal_request_identity;
use crate::error::ApiError;
use crate::state::AppState;

const DLQ_SCAN_LIMIT: usize = 1_000;

#[derive(Debug, Deserialize)]
pub(super) struct ArchiveAsyncToolDeadLetterRequest {
    operation_id: String,
    invocation_id: String,
    reason: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ArchiveAsyncToolDeadLetterResponse {
    operation_id: String,
    invocation_id: String,
    dead_letter_archived: bool,
}

pub(super) async fn archive_async_tool_dead_letter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ArchiveAsyncToolDeadLetterRequest>,
) -> Result<Json<ArchiveAsyncToolDeadLetterResponse>, ApiError> {
    let identity =
        require_internal_request_identity(&state.config, &headers, "queue.dead_letter.archive")?;
    if identity.caller != "configuration-center" {
        return Err(ApiError::forbidden(
            "only Configuration Center may archive MCP dead letters",
        ));
    }
    let operation_id = validated_text(input.operation_id, "operation_id", 200)?;
    validate_operation_trace(identity.trace_id.as_deref(), operation_id.as_str())?;
    let invocation_id = validated_text(input.invocation_id, "invocation_id", 160)?;
    let reason = input.reason.trim().to_string();
    if !(8..=500).contains(&reason.len()) {
        return Err(ApiError::bad_request(
            "reason must contain between 8 and 500 characters",
        ));
    }

    let record = state
        .runtime_invocations
        .dead_letter_archive_candidate(invocation_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "MCP Invocation {invocation_id} is not an eligible confirmed dead-letter failure"
            ))
        })?;
    tracing::warn!(
        operation_id = operation_id.as_str(),
        invocation_id = invocation_id.as_str(),
        caller_service = record.caller_service.as_str(),
        reason = reason.as_str(),
        "Configuration Center requested MCP async tool dead-letter archival"
    );
    let dead_letter_archived = state
        .async_tool_dispatch
        .archive_dead_lettered_invocation(&record, DLQ_SCAN_LIMIT)
        .await
        .map_err(ApiError::internal)?;
    if !dead_letter_archived {
        return Err(ApiError::bad_request(format!(
            "no exact MCP async tool dead letter matched Invocation {invocation_id}"
        )));
    }
    Ok(Json(ArchiveAsyncToolDeadLetterResponse {
        operation_id,
        invocation_id,
        dead_letter_archived,
    }))
}

fn validate_operation_trace(trace_id: Option<&str>, operation_id: &str) -> Result<(), ApiError> {
    if trace_id != Some(operation_id) {
        return Err(ApiError::forbidden(
            "MCP dead-letter operation ID does not match the signed trace ID",
        ));
    }
    Ok(())
}

fn validated_text(value: String, field: &str, max_len: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_len {
        return Err(ApiError::bad_request(format!(
            "{field} is required and must contain at most {max_len} characters"
        )));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{validate_operation_trace, validated_text};

    #[test]
    fn archive_identity_is_trimmed_and_bounded() {
        assert_eq!(
            validated_text(" invocation-1 ".to_string(), "invocation_id", 160).unwrap(),
            "invocation-1"
        );
        assert!(validated_text(" ".to_string(), "invocation_id", 160).is_err());
    }

    #[test]
    fn archive_operation_must_match_signed_trace_id() {
        assert!(validate_operation_trace(Some("operation-1"), "operation-1").is_ok());
        assert!(validate_operation_trace(Some("operation-2"), "operation-1").is_err());
        assert!(validate_operation_trace(None, "operation-1").is_err());
    }
}
