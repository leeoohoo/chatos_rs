// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use tracing::{warn, Level};

use crate::repositories::{subject_memory_scopes, summaries, threads};
use crate::state::AppState;

const DLQ_SCAN_LIMIT: usize = 1_000;

#[derive(Debug, Deserialize)]
pub struct ReplayQueueDeadLetterRequest {
    pub operation_id: String,
    pub stream: String,
    pub tenant_id: String,
    pub source_id: String,
    pub item_id: String,
    pub version: i64,
    pub event_type: Option<String>,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ReplayQueueDeadLetterResponse {
    pub operation_id: String,
    pub stream: String,
    pub tenant_id: String,
    pub source_id: String,
    pub item_id: String,
    pub version: i64,
    pub event_type: Option<String>,
    pub event_enqueued: bool,
    pub dead_letter_archived: bool,
}

pub async fn replay_queue_dead_letter(
    State(state): State<Arc<AppState>>,
    identity: Option<Extension<chatos_service_runtime::InternalServiceTokenClaims>>,
    Json(input): Json<ReplayQueueDeadLetterRequest>,
) -> Result<Json<ReplayQueueDeadLetterResponse>, (StatusCode, String)> {
    let request = ValidatedReplayRequest::try_from(input)?;
    validate_operation_identity(identity, request.operation_id.as_str())?;
    validate_event_type(&request)?;
    warn!(
        operation_id = request.operation_id.as_str(),
        stream = request.stream.as_str(),
        tenant_id = request.tenant_id.as_str(),
        source_id = request.source_id.as_str(),
        item_id = request.item_id.as_str(),
        version = request.version,
        event_type = request.event_type.as_deref().unwrap_or(""),
        reason = request.reason.as_str(),
        "Configuration Center requested Memory Engine dead-letter replay"
    );

    let archived = match request.stream.as_str() {
        "summary" => replay_summary(&state, &request).await?,
        "rollup" => replay_rollup(&state, &request).await?,
        "subject_memory" => replay_subject_memory(&state, &request).await?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unsupported Memory Engine replay stream {}", request.stream),
            ));
        }
    };

    Ok(Json(ReplayQueueDeadLetterResponse {
        operation_id: request.operation_id,
        stream: request.stream,
        tenant_id: request.tenant_id,
        source_id: request.source_id,
        item_id: request.item_id,
        version: request.version,
        event_type: request.event_type,
        event_enqueued: true,
        dead_letter_archived: archived,
    }))
}

fn validate_operation_identity(
    identity: Option<Extension<chatos_service_runtime::InternalServiceTokenClaims>>,
    operation_id: &str,
) -> Result<(), (StatusCode, String)> {
    let Some(Extension(identity)) = identity else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "signed Configuration Center identity is required for Memory Engine queue replay"
                .to_string(),
        ));
    };
    if identity.caller != "configuration-center" {
        return Err((
            StatusCode::FORBIDDEN,
            "only Configuration Center may replay Memory Engine dead letters".to_string(),
        ));
    }
    if identity.trace_id != operation_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Memory Engine queue operation ID does not match the signed trace ID".to_string(),
        ));
    }
    Ok(())
}

async fn replay_summary(
    state: &AppState,
    request: &ValidatedReplayRequest,
) -> Result<bool, (StatusCode, String)> {
    require_absent_event_type(request)?;
    let event = threads::replay_dead_lettered_summary_dispatch(
        &state.pool,
        request.tenant_id.as_str(),
        request.source_id.as_str(),
        request.item_id.as_str(),
        request.version,
    )
    .await
    .map_err(internal_error)?
    .ok_or_else(|| ineligible(request))?;
    crate::summary_queue::publish_rearmed_summary_dispatch(state, &event)
        .await
        .map_err(internal_error)?;
    Ok(archive_result(
        request,
        crate::summary_queue::archive_summary_dead_letter(
            &state.config,
            request.tenant_id.as_str(),
            request.source_id.as_str(),
            request.item_id.as_str(),
            request.version,
            DLQ_SCAN_LIMIT,
        )
        .await,
    ))
}

async fn replay_rollup(
    state: &AppState,
    request: &ValidatedReplayRequest,
) -> Result<bool, (StatusCode, String)> {
    require_absent_event_type(request)?;
    let event = summaries::replay_dead_lettered_rollup_dispatch(
        &state.pool,
        request.tenant_id.as_str(),
        request.source_id.as_str(),
        request.item_id.as_str(),
        request.version,
    )
    .await
    .map_err(internal_error)?
    .ok_or_else(|| ineligible(request))?;
    crate::rollup_queue::publish_rearmed_rollup_dispatch(state, &event)
        .await
        .map_err(internal_error)?;
    Ok(archive_result(
        request,
        crate::rollup_queue::archive_rollup_dead_letter(
            &state.config,
            request.tenant_id.as_str(),
            request.source_id.as_str(),
            request.item_id.as_str(),
            request.version,
            DLQ_SCAN_LIMIT,
        )
        .await,
    ))
}

async fn replay_subject_memory(
    state: &AppState,
    request: &ValidatedReplayRequest,
) -> Result<bool, (StatusCode, String)> {
    match request.event_type.as_deref() {
        Some("source_available") => {
            let event = summaries::replay_dead_lettered_subject_memory_source_dispatch(
                &state.pool,
                request.tenant_id.as_str(),
                request.source_id.as_str(),
                request.item_id.as_str(),
                request.version,
            )
            .await
            .map_err(internal_error)?
            .ok_or_else(|| ineligible(request))?;
            crate::subject_memory_queue::publish_rearmed_source_dispatch(
                &state.config,
                &state.pool,
                &event,
            )
            .await
            .map_err(internal_error)?;
            Ok(archive_result(
                request,
                crate::subject_memory_queue::archive_subject_memory_source_dead_letter(
                    &state.config,
                    request.tenant_id.as_str(),
                    request.source_id.as_str(),
                    request.item_id.as_str(),
                    request.version,
                    DLQ_SCAN_LIMIT,
                )
                .await,
            ))
        }
        Some("scope_requested") => {
            let event = subject_memory_scopes::replay_dead_lettered_subject_memory_dispatch(
                &state.pool,
                request.tenant_id.as_str(),
                request.source_id.as_str(),
                request.item_id.as_str(),
                request.version,
            )
            .await
            .map_err(internal_error)?
            .ok_or_else(|| ineligible(request))?;
            crate::subject_memory_queue::publish_rearmed_scope_dispatch(
                &state.config,
                &state.pool,
                &event,
            )
            .await
            .map_err(internal_error)?;
            Ok(archive_result(
                request,
                crate::subject_memory_queue::archive_subject_memory_scope_dead_letter(
                    &state.config,
                    request.tenant_id.as_str(),
                    request.source_id.as_str(),
                    request.item_id.as_str(),
                    request.version,
                    DLQ_SCAN_LIMIT,
                )
                .await,
            ))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            "subject_memory replay requires event_type source_available or scope_requested"
                .to_string(),
        )),
    }
}

fn archive_result(request: &ValidatedReplayRequest, result: Result<bool, String>) -> bool {
    match result {
        Ok(archived) => archived,
        Err(error) => {
            tracing::event!(
                Level::WARN,
                operation_id = request.operation_id.as_str(),
                stream = request.stream.as_str(),
                item_id = request.item_id.as_str(),
                version = request.version,
                error = error.as_str(),
                "Memory Engine replay succeeded but old DLQ message archival failed"
            );
            false
        }
    }
}

fn require_absent_event_type(request: &ValidatedReplayRequest) -> Result<(), (StatusCode, String)> {
    if request.event_type.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{} replay does not accept event_type", request.stream),
        ));
    }
    Ok(())
}

fn ineligible(request: &ValidatedReplayRequest) -> (StatusCode, String) {
    (
        StatusCode::BAD_REQUEST,
        format!(
            "Memory Engine {}/{} version {} is not an eligible dead-lettered dispatch",
            request.stream, request.item_id, request.version
        ),
    )
}

fn internal_error(error: String) -> (StatusCode, String) {
    tracing::error!(error = error.as_str(), "Memory Engine queue replay failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Memory Engine queue replay failed; inspect service logs using the operation ID"
            .to_string(),
    )
}

fn validate_event_type(request: &ValidatedReplayRequest) -> Result<(), (StatusCode, String)> {
    match request.stream.as_str() {
        "subject_memory"
            if matches!(
                request.event_type.as_deref(),
                Some("source_available" | "scope_requested")
            ) =>
        {
            Ok(())
        }
        "subject_memory" => Err((
            StatusCode::BAD_REQUEST,
            "subject_memory replay requires event_type source_available or scope_requested"
                .to_string(),
        )),
        "summary" | "rollup" => require_absent_event_type(request),
        _ => Ok(()),
    }
}

struct ValidatedReplayRequest {
    operation_id: String,
    stream: String,
    tenant_id: String,
    source_id: String,
    item_id: String,
    version: i64,
    event_type: Option<String>,
    reason: String,
}

impl TryFrom<ReplayQueueDeadLetterRequest> for ValidatedReplayRequest {
    type Error = (StatusCode, String);

    fn try_from(input: ReplayQueueDeadLetterRequest) -> Result<Self, Self::Error> {
        let operation_id = required_bounded(input.operation_id, "operation_id", 100)?;
        let stream = required_bounded(input.stream, "stream", 50)?;
        let tenant_id = required_bounded(input.tenant_id, "tenant_id", 200)?;
        let source_id = required_bounded(input.source_id, "source_id", 200)?;
        let item_id = required_bounded(input.item_id, "item_id", 500)?;
        let reason = input.reason.trim().to_string();
        if input.version < 1 || reason.len() < 8 || reason.len() > 500 {
            return Err((
                StatusCode::BAD_REQUEST,
                "version must be positive and replay reason must contain 8..500 characters"
                    .to_string(),
            ));
        }
        let event_type = input
            .event_type
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(Self {
            operation_id,
            stream,
            tenant_id,
            source_id,
            item_id,
            version: input.version,
            event_type,
            reason,
        })
    }
}

fn required_bounded(
    value: String,
    field: &str,
    max_len: usize,
) -> Result<String, (StatusCode, String)> {
    let value = value.trim().to_string();
    if value.is_empty() || value.len() > max_len {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{field} is required and must contain at most {max_len} characters"),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn internal_identity(
        caller: &str,
        trace_id: &str,
    ) -> Extension<chatos_service_runtime::InternalServiceTokenClaims> {
        Extension(chatos_service_runtime::InternalServiceTokenClaims {
            iss: caller.to_string(),
            sub: caller.to_string(),
            caller: caller.to_string(),
            aud: "memory-engine".to_string(),
            scope: "memory.operator".to_string(),
            trace_id: trace_id.to_string(),
            owner_user_id: None,
            iat: 1,
            exp: 2,
        })
    }

    #[test]
    fn replay_request_requires_exact_identity_and_version() {
        let request = ValidatedReplayRequest::try_from(ReplayQueueDeadLetterRequest {
            operation_id: "operation-1".to_string(),
            stream: "summary".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            item_id: "thread-1".to_string(),
            version: 3,
            event_type: None,
            reason: "operator requested replay".to_string(),
        })
        .expect("valid replay request");
        assert_eq!(request.version, 3);
        assert_eq!(request.item_id, "thread-1");
    }

    #[test]
    fn replay_event_type_is_bound_to_stream() {
        let request = ValidatedReplayRequest {
            operation_id: "operation-1".to_string(),
            stream: "subject_memory".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            item_id: "scope-1".to_string(),
            version: 1,
            event_type: None,
            reason: "operator requested replay".to_string(),
        };
        assert!(validate_event_type(&request).is_err());

        let summary_request = ValidatedReplayRequest {
            stream: "summary".to_string(),
            event_type: Some("source_available".to_string()),
            ..request
        };
        assert!(validate_event_type(&summary_request).is_err());
    }

    #[test]
    fn replay_operation_must_match_signed_configuration_center_trace() {
        assert!(validate_operation_identity(
            Some(internal_identity("configuration-center", "operation-1")),
            "operation-1",
        )
        .is_ok());
        assert!(validate_operation_identity(
            Some(internal_identity("configuration-center", "operation-2")),
            "operation-1",
        )
        .is_err());
        assert!(validate_operation_identity(
            Some(internal_identity("task-runner", "operation-1")),
            "operation-1",
        )
        .is_err());
        assert!(validate_operation_identity(None, "operation-1").is_err());
    }
}
