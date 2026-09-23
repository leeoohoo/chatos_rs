// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_mcp_service::MCP_ERROR_INTERNAL;

const TERMINAL_PROCESS_WAIT_TIMEOUT_MESSAGE: &str = "terminal process wait timed out";
const RESTART_INTERRUPTED_MESSAGE: &str =
    "MCP Management restarted before the provider returned a durable tool result";
const MISSING_BATCH_MESSAGE: &str =
    "MCP Management could not recover the durable tool batch for this invocation";
const EXPIRED_INVOCATION_MESSAGE: &str =
    "Runtime Invocation expired before a durable provider result was recorded";
// MCP model inputs may contain up to 2 MiB of decoded image bytes. Base64 adds
// roughly one third, and the surrounding JSON/MCP envelope adds more overhead.
// Four MiB preserves the supported visual payload while keeping model and
// broker payloads bounded independently of the database backend.
pub(super) const MAX_INLINE_MCP_RESULT_BYTES: usize = 4 * 1024 * 1024;

impl RuntimeInvocationStore {
    pub(crate) async fn finish_expired_claim(
        &self,
        claim: &ExpiredRuntimeInvocationClaim,
    ) -> Result<bool, String> {
        use chatos_mcp_service::MCP_ERROR_UNKNOWN_EXECUTION_STATE;

        let terminal_status = if claim.record.mutation_may_have_started
            && claim.record.started_at_unix_ms.is_some()
        {
            RuntimeInvocationStatus::UnknownExecutionState
        } else {
            RuntimeInvocationStatus::Cancelled
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let transitioned_record = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(claim.record.invocation_id.as_str()) else {
                    return Ok(false);
                };
                if record.status != RuntimeInvocationStatus::CancelRequested {
                    return Ok(false);
                }
                record.status = terminal_status;
                record.completed_at_unix_ms = Some(now_ms);
                record.terminal_result = None;
                record.terminal_error_code = (terminal_status
                    == RuntimeInvocationStatus::UnknownExecutionState)
                    .then_some(MCP_ERROR_UNKNOWN_EXECUTION_STATE);
                record.terminal_error_message = Some(EXPIRED_INVOCATION_MESSAGE.to_string());
                Some(record.clone())
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations \
                     WHERE invocation_id=$1 AND recovery_claim_token=$2 \
                       AND recovery_claim_until>now() FOR UPDATE",
                )
                .bind(claim.record.invocation_id.as_str())
                .bind(claim.claim_token.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| {
                    format!("load claimed expired Runtime Invocation failed: {error}")
                })?;
                let Some(value) = value else {
                    return Ok(false);
                };
                let mut record = decode_invocation(value)?;
                if record.status != RuntimeInvocationStatus::CancelRequested {
                    sqlx::query(
                        "UPDATE mcp_management_runtime_invocations \
                         SET recovery_claim_token=NULL,recovery_claim_until=NULL \
                         WHERE invocation_id=$1 AND recovery_claim_token=$2",
                    )
                    .bind(record.invocation_id.as_str())
                    .bind(claim.claim_token.as_str())
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?;
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(false);
                }
                record.status = terminal_status;
                record.completed_at_unix_ms = Some(now_ms);
                record.terminal_result = None;
                record.terminal_error_code = (terminal_status
                    == RuntimeInvocationStatus::UnknownExecutionState)
                    .then_some(MCP_ERROR_UNKNOWN_EXECUTION_STATE);
                record.terminal_error_message = Some(EXPIRED_INVOCATION_MESSAGE.to_string());
                persist_invocation(&mut *tx, &record).await?;
                sqlx::query(
                    "UPDATE mcp_management_runtime_invocations \
                     SET recovery_claim_token=NULL,recovery_claim_until=NULL \
                     WHERE invocation_id=$1 AND recovery_claim_token=$2",
                )
                .bind(record.invocation_id.as_str())
                .bind(claim.claim_token.as_str())
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                tx.commit().await.map_err(|error| error.to_string())?;
                Some(record)
            }
        };
        if let Some(record) = transitioned_record.as_ref() {
            if let Err(error) = self.quota.release(record).await {
                self.diagnostics
                    .quota_release_failures
                    .fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    invocation_id = record.invocation_id.as_str(),
                    error = error.as_str(),
                    "release expired Runtime Invocation quota reservation failed"
                );
            }
        }
        Ok(transitioned_record.is_some())
    }

    pub async fn recover_after_restart(
        &self,
        record: &RuntimeInvocationRecord,
        durable_batch_exists: bool,
    ) -> Result<bool, String> {
        use chatos_mcp_service::MCP_ERROR_UNKNOWN_EXECUTION_STATE;

        let missing_batch = !durable_batch_exists;
        match record.status {
            RuntimeInvocationStatus::Queued if !missing_batch => Ok(false),
            RuntimeInvocationStatus::WaitingForUser if !missing_batch => Ok(false),
            RuntimeInvocationStatus::Queued => {
                self.fail(
                    record.invocation_id.as_str(),
                    MCP_ERROR_INTERNAL,
                    MISSING_BATCH_MESSAGE,
                )
                .await
            }
            RuntimeInvocationStatus::Running | RuntimeInvocationStatus::WaitingForUser => {
                let message = if missing_batch {
                    MISSING_BATCH_MESSAGE
                } else {
                    RESTART_INTERRUPTED_MESSAGE
                };
                if record.mutation_may_have_started && record.started_at_unix_ms.is_some() {
                    self.transition_terminal(
                        record.invocation_id.as_str(),
                        &[record.status],
                        RuntimeInvocationStatus::UnknownExecutionState,
                        None,
                        Some(MCP_ERROR_UNKNOWN_EXECUTION_STATE),
                        Some(message.to_string()),
                    )
                    .await
                } else {
                    self.transition_terminal(
                        record.invocation_id.as_str(),
                        &[record.status],
                        RuntimeInvocationStatus::Failed,
                        None,
                        Some(MCP_ERROR_INTERNAL),
                        Some(message.to_string()),
                    )
                    .await
                }
            }
            RuntimeInvocationStatus::CancelRequested => {
                let unknown =
                    record.mutation_may_have_started && record.started_at_unix_ms.is_some();
                self.transition_terminal(
                    record.invocation_id.as_str(),
                    &[RuntimeInvocationStatus::CancelRequested],
                    if unknown {
                        RuntimeInvocationStatus::UnknownExecutionState
                    } else {
                        RuntimeInvocationStatus::Cancelled
                    },
                    None,
                    unknown.then_some(MCP_ERROR_UNKNOWN_EXECUTION_STATE),
                    Some(
                        if missing_batch {
                            MISSING_BATCH_MESSAGE
                        } else {
                            RESTART_INTERRUPTED_MESSAGE
                        }
                        .to_string(),
                    ),
                )
                .await
            }
            RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState => Ok(false),
        }
    }

    pub async fn mark_running(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Queued],
            RuntimeInvocationStatus::Running,
        )
        .await
    }

    pub async fn mark_waiting_for_user(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Running],
            RuntimeInvocationStatus::WaitingForUser,
        )
        .await
    }

    pub async fn complete(&self, invocation_id: &str, result: Value) -> Result<bool, String> {
        if terminal_process_wait_timed_out(&result) {
            return self
                .transition_terminal(
                    invocation_id,
                    &[
                        RuntimeInvocationStatus::Running,
                        RuntimeInvocationStatus::WaitingForUser,
                    ],
                    RuntimeInvocationStatus::Failed,
                    Some(result),
                    Some(MCP_ERROR_INTERNAL),
                    Some(TERMINAL_PROCESS_WAIT_TIMEOUT_MESSAGE.to_string()),
                )
                .await;
        }
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Running,
                RuntimeInvocationStatus::WaitingForUser,
            ],
            RuntimeInvocationStatus::Completed,
            Some(result),
            None,
            None,
        )
        .await
    }

    pub async fn fail(
        &self,
        invocation_id: &str,
        error_code: i32,
        error_message: impl Into<String>,
    ) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Queued,
                RuntimeInvocationStatus::Running,
                RuntimeInvocationStatus::WaitingForUser,
            ],
            RuntimeInvocationStatus::Failed,
            None,
            Some(error_code),
            Some(error_message.into()),
        )
        .await
    }

    pub async fn finish_cancellation(
        &self,
        invocation_id: &str,
        status: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        if !matches!(
            status,
            RuntimeInvocationStatus::Cancelled | RuntimeInvocationStatus::UnknownExecutionState
        ) {
            return Err("invalid terminal Runtime Invocation cancellation state".to_string());
        }
        self.transition_terminal(
            invocation_id,
            &[RuntimeInvocationStatus::CancelRequested],
            status,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn cancel_without_start(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Queued,
                RuntimeInvocationStatus::CancelRequested,
            ],
            RuntimeInvocationStatus::Cancelled,
            None,
            None,
            None,
        )
        .await
    }

    async fn transition_status(
        &self,
        invocation_id: &str,
        from: &[RuntimeInvocationStatus],
        to: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                if to == RuntimeInvocationStatus::Running && record.started_at_unix_ms.is_none() {
                    record.started_at_unix_ms = Some(chrono::Utc::now().timestamp_millis());
                }
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations WHERE invocation_id=$1 FOR UPDATE",
                )
                .bind(invocation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| format!("load Runtime Invocation for transition failed: {error}"))?;
                let Some(value) = value else {
                    return Ok(false);
                };
                let mut record = decode_invocation(value)?;
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                if to == RuntimeInvocationStatus::Running && record.started_at_unix_ms.is_none() {
                    record.started_at_unix_ms = Some(chrono::Utc::now().timestamp_millis());
                }
                persist_invocation(&mut *tx, &record).await?;
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(true)
            }
        }
    }

    pub(super) async fn transition_terminal(
        &self,
        invocation_id: &str,
        from: &[RuntimeInvocationStatus],
        to: RuntimeInvocationStatus,
        result: Option<Value>,
        error_code: Option<i32>,
        error_message: Option<String>,
    ) -> Result<bool, String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let file_modification_outcome =
            terminal_file_modification_outcome(to, result.as_ref(), error_message.as_deref());
        let result = sanitize_terminal_result(result)?;
        let transitioned_record = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                record.completed_at_unix_ms = Some(now_ms);
                record.terminal_result = result;
                record.terminal_error_code = error_code;
                record.terminal_error_message = error_message;
                record.file_modification_outcome =
                    if is_file_modification_tool(record.original_tool_name.as_str()) {
                        file_modification_outcome
                    } else {
                        None
                    };
                Some(record.clone())
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations WHERE invocation_id=$1 FOR UPDATE",
                )
                .bind(invocation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| format!("load Runtime Invocation for terminal transition failed: {error}"))?;
                let Some(value) = value else {
                    return Ok(false);
                };
                let mut record = decode_invocation(value)?;
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                record.completed_at_unix_ms = Some(now_ms);
                record.terminal_result = result;
                record.terminal_error_code = error_code;
                record.terminal_error_message = error_message;
                record.file_modification_outcome =
                    if is_file_modification_tool(record.original_tool_name.as_str()) {
                        file_modification_outcome
                    } else {
                        None
                    };
                persist_invocation(&mut *tx, &record).await?;
                tx.commit().await.map_err(|error| error.to_string())?;
                Some(record)
            }
        };
        if let Some(record) = transitioned_record.as_ref() {
            if let Err(error) = self.quota.release(record).await {
                self.diagnostics
                    .quota_release_failures
                    .fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    invocation_id = record.invocation_id.as_str(),
                    error = error.as_str(),
                    "release terminal Runtime Invocation quota reservation failed"
                );
            }
        }
        Ok(transitioned_record.is_some())
    }
}

fn terminal_process_wait_timed_out(result: &Value) -> bool {
    let payload = chatos_mcp_runtime::structured_result_payload(result);
    payload.get("timed_out").and_then(Value::as_bool) == Some(true)
        && payload.get("completed").and_then(Value::as_bool) == Some(false)
        && payload.get("wait_status").and_then(Value::as_str) == Some("timeout")
}

fn terminal_file_modification_outcome(
    status: RuntimeInvocationStatus,
    result: Option<&Value>,
    error_message: Option<&str>,
) -> Option<FileModificationOutcome> {
    match status {
        RuntimeInvocationStatus::Completed => result
            .and_then(file_modification_outcome_from_result)
            .or(Some(FileModificationOutcome::Changed)),
        RuntimeInvocationStatus::Failed => error_message.map(classify_file_modification_error),
        RuntimeInvocationStatus::Queued
        | RuntimeInvocationStatus::Running
        | RuntimeInvocationStatus::WaitingForUser
        | RuntimeInvocationStatus::CancelRequested
        | RuntimeInvocationStatus::Cancelled
        | RuntimeInvocationStatus::UnknownExecutionState => None,
    }
}

fn file_modification_outcome_from_result(result: &Value) -> Option<FileModificationOutcome> {
    let payload = chatos_mcp_runtime::structured_result_payload(result);
    if let Some(outcome) = payload.get("outcome").and_then(Value::as_str) {
        return match outcome {
            "changed" => Some(FileModificationOutcome::Changed),
            "already_applied" => Some(FileModificationOutcome::AlreadyApplied),
            "stale" | "stale_context" => Some(FileModificationOutcome::StaleContext),
            "expected_match" => Some(FileModificationOutcome::ExpectedMatch),
            "validation" => Some(FileModificationOutcome::Validation),
            "infrastructure" => Some(FileModificationOutcome::Infrastructure),
            _ => None,
        };
    }
    let result_payload = payload.get("result").unwrap_or(payload);
    if result_payload
        .get("already_applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Some(FileModificationOutcome::AlreadyApplied);
    }
    result_payload
        .get("changed")
        .and_then(Value::as_bool)
        .map(FileModificationOutcome::from_changed)
}

fn is_file_modification_tool(original_tool_name: &str) -> bool {
    matches!(
        original_tool_name,
        "stage_edit_batch" | "commit_edit_session"
    )
}

pub(super) fn sanitize_terminal_result(result: Option<Value>) -> Result<Option<Value>, String> {
    let Some(result) = result else {
        return Ok(None);
    };
    let encoded = serde_json::to_vec(&result).map_err(|error| error.to_string())?;
    if encoded.len() <= MAX_INLINE_MCP_RESULT_BYTES {
        return Ok(Some(result));
    }
    Ok(Some(serde_json::json!({
        "status": "result_truncated",
        "result_bytes": encoded.len(),
    })))
}
