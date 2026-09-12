// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, ToolExecutionStateRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, LocalAgentEventStatus, LocalAgentEventType,
    LocalAgentRunStatus, MemorySyncStatus, MessageMode, ToolApprovalDecision, ToolEffect,
    ToolExecution, ToolExecutionStatus,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::digest::{canonical_json_digest, stable_digest_id};
use crate::memory_sync::{
    next_semantic_message_sequence, persist_semantic_message, RecordSemanticMessageRequest,
};
use crate::ui_events::append_tool_snapshot;

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedToolCall {
    pub invocation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub effect: ToolEffect,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedToolBatch {
    pub event_id: String,
    pub run_id: String,
    pub batch_id: String,
    pub source_turn_id: String,
    pub project_id: Option<String>,
    pub capability_snapshot_ref: String,
    pub calls: Vec<PreparedToolCall>,
}

#[derive(Debug, Clone)]
pub struct PrepareToolBatchRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub claim_token: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DeferToolBatchForApprovalRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub claim_token: String,
    pub batch: PreparedToolBatch,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolApprovalDeferralResult {
    Deferred(AgentEventStateRecord),
    Ready(AgentEventStateRecord),
}

/// Releases a claimed tool event into a durable, non-runnable wait state.
/// The approval transaction makes it runnable again only after every
/// side-effecting invocation in the frozen batch has a decision.
pub async fn defer_tool_batch_for_approval(
    storage: &dyn ClientStorage,
    request: DeferToolBatchForApprovalRequest,
) -> StorageResult<ToolApprovalDeferralResult> {
    let mut operation = DeferToolBatchForApprovalOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "tool approval deferral returned no result".to_string(),
    })
}

struct DeferToolBatchForApprovalOperation {
    request: Option<DeferToolBatchForApprovalRequest>,
    result: Option<ToolApprovalDeferralResult>,
}

#[async_trait]
impl StorageTransaction for DeferToolBatchForApprovalOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "tool approval deferral request was already consumed".to_string(),
        })?;
        if request.batch.event_id != request.event_id {
            return invalid_data("tool approval deferral batch does not match its event");
        }
        let mut event = repositories
            .agent_events()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: request.event_id,
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if event.event.event_type != LocalAgentEventType::ToolBatchRequested
            || event.event.status != LocalAgentEventStatus::Claimed
            || event.event.claim_token.as_deref() != Some(request.claim_token.as_str())
            || event.event.run_id != request.batch.run_id
        {
            return invalid_data("tool approval deferral does not own the claimed batch event");
        }
        let mut awaiting = false;
        for call in &request.batch.calls {
            let execution = repositories
                .tool_executions()
                .get(&RecordQuery {
                    scope: request.scope.clone(),
                    id: call.invocation_id.clone(),
                })
                .await?
                .ok_or(StorageError::NotFound)?;
            if execution.execution.run_id != request.batch.run_id
                || execution.execution.batch_id != request.batch.batch_id
            {
                return invalid_data("tool approval execution is outside the frozen batch");
            }
            awaiting |= execution.execution.status == ToolExecutionStatus::AwaitingApproval;
        }
        if !awaiting {
            self.result = Some(ToolApprovalDeferralResult::Ready(event));
            return Ok(());
        }

        let revision = event.metadata.revision;
        event.event.status = LocalAgentEventStatus::Pending;
        event.event.claimed_by_device_id = None;
        event.event.claim_token = None;
        event.event.claim_until = None;
        event.event.available_at = request
            .now
            .checked_add_signed(chrono::Duration::days(3_650))
            .ok_or_else(|| StorageError::InvalidData {
                reason: "tool approval wait deadline overflowed".to_string(),
            })?;
        let event = repositories
            .agent_events()
            .put(PutRecord {
                record: event,
                expected_revision: Some(revision),
            })
            .await?;
        self.result = Some(ToolApprovalDeferralResult::Deferred(event));
        Ok(())
    }
}

/// Freezes every invocation before any local implementation is called. A
/// repeated request returns the same records only when the exact arguments,
/// tool effect, Run and batch still match.
pub async fn prepare_tool_batch(
    storage: &dyn ClientStorage,
    request: PrepareToolBatchRequest,
) -> StorageResult<PreparedToolBatch> {
    let mut operation = PrepareToolBatchOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "tool batch preparation returned no result".to_string(),
    })
}

struct PrepareToolBatchOperation {
    request: Option<PrepareToolBatchRequest>,
    result: Option<PreparedToolBatch>,
}

#[async_trait]
impl StorageTransaction for PrepareToolBatchOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "tool batch preparation request was already consumed".to_string(),
        })?;
        let event = require_claimed_tool_event(repositories, &request).await?;
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: event.event.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if run.run.status != LocalAgentRunStatus::WaitingToolResult
            || run.run.version != event.event.expected_version
        {
            return invalid_data("tool batch event does not match the active Run state");
        }
        let batch_id = run
            .run
            .pending_batch_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| StorageError::InvalidData {
                reason: "waiting tool Run has no pending batch".to_string(),
            })?;
        let envelope = parse_batch_envelope(&event, &run.run.capability_snapshot_ref)?;
        if envelope.project_id != run.run.project_id {
            return invalid_data("tool batch project scope does not match the frozen Run");
        }

        let mut calls = Vec::with_capacity(envelope.calls.len());
        let mut call_ids = HashSet::new();
        for call in envelope.calls {
            if !call_ids.insert(call.tool_call_id.clone()) {
                return invalid_data("tool batch contains duplicate call IDs");
            }
            let invocation_id = stable_invocation_id(
                run.run.run_id.as_str(),
                batch_id,
                call.tool_call_id.as_str(),
            );
            let arguments_digest = canonical_json_digest(&call.arguments)?;
            let execution = ToolExecution {
                invocation_id: invocation_id.clone(),
                run_id: run.run.run_id.clone(),
                batch_id: batch_id.to_string(),
                tool_call_id: call.tool_call_id.clone(),
                tool_name: call.tool_name.clone(),
                effect: call.effect,
                arguments_digest,
                status: if call.effect.requires_approval() {
                    ToolExecutionStatus::AwaitingApproval
                } else {
                    ToolExecutionStatus::Requested
                },
                bounded_result: None,
                approval_decided_at: None,
                approval_reason: None,
                started_at: None,
                completed_at: None,
            };
            execution
                .validate()
                .map_err(|error| StorageError::InvalidData {
                    reason: format!("invalid frozen tool execution: {error}"),
                })?;
            let query = RecordQuery {
                scope: request.scope.clone(),
                id: invocation_id.clone(),
            };
            let existing = {
                let mut tool_executions = repositories.tool_executions();
                tool_executions.get(&query).await?
            };
            if let Some(existing) = existing {
                validate_existing_execution(&existing.execution, &execution)?;
            } else {
                let stored = repositories
                    .tool_executions()
                    .put(PutRecord {
                        record: ToolExecutionStateRecord {
                            metadata: RecordMetadata {
                                id: invocation_id.clone(),
                                scope: request.scope.clone(),
                                origin_device_id: event.metadata.origin_device_id.clone(),
                                revision: 0,
                                created_at: request.now,
                                updated_at: request.now,
                            },
                            execution,
                        },
                        expected_revision: None,
                    })
                    .await?;
                append_tool_snapshot(repositories, &stored).await?;
            }
            calls.push(PreparedToolCall {
                invocation_id,
                tool_call_id: call.tool_call_id,
                tool_name: call.tool_name,
                effect: call.effect,
                arguments: call.arguments,
            });
        }
        self.result = Some(PreparedToolBatch {
            event_id: event.event.event_id,
            run_id: run.run.run_id,
            batch_id: batch_id.to_string(),
            source_turn_id: event.event.correlation_id,
            project_id: envelope.project_id,
            capability_snapshot_ref: envelope.capability_snapshot_ref,
            calls,
        });
        Ok(())
    }
}

async fn require_claimed_tool_event(
    repositories: &mut dyn TransactionRepositories,
    request: &PrepareToolBatchRequest,
) -> StorageResult<AgentEventStateRecord> {
    let event = repositories
        .agent_events()
        .get(&RecordQuery {
            scope: request.scope.clone(),
            id: request.event_id.clone(),
        })
        .await?
        .ok_or(StorageError::NotFound)?;
    if event.event.event_type != LocalAgentEventType::ToolBatchRequested
        || event.event.status != LocalAgentEventStatus::Claimed
        || event.event.claim_token.as_deref() != Some(request.claim_token.as_str())
        || event
            .event
            .claim_until
            .is_none_or(|deadline| deadline < request.now)
    {
        return Err(StorageError::Conflict {
            actual_revision: event.metadata.revision,
        });
    }
    Ok(event)
}

struct BatchEnvelope {
    project_id: Option<String>,
    capability_snapshot_ref: String,
    calls: Vec<EnvelopeCall>,
}

struct EnvelopeCall {
    tool_call_id: String,
    tool_name: String,
    effect: ToolEffect,
    arguments: Value,
}

fn parse_batch_envelope(
    event: &AgentEventStateRecord,
    expected_capability_snapshot_ref: &str,
) -> StorageResult<BatchEnvelope> {
    let payload =
        event
            .event
            .bounded_payload
            .as_object()
            .ok_or_else(|| StorageError::InvalidData {
                reason: "tool batch payload must be an object".to_string(),
            })?;
    let project_id = match payload.get("project_id") {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.clone()),
        Some(Value::Null) => None,
        _ => return invalid_data("tool batch requires an explicit project_id scope"),
    };
    let capability_snapshot_ref = payload
        .get("capability_snapshot_ref")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| StorageError::InvalidData {
            reason: "tool batch requires a capability snapshot reference".to_string(),
        })?;
    if capability_snapshot_ref != expected_capability_snapshot_ref {
        return invalid_data("tool batch capability snapshot does not match the frozen Run");
    }
    let raw_calls = payload
        .get("calls")
        .and_then(Value::as_array)
        .filter(|calls| !calls.is_empty())
        .ok_or_else(|| StorageError::InvalidData {
            reason: "tool batch must contain at least one call".to_string(),
        })?;
    let mut calls = Vec::with_capacity(raw_calls.len());
    for raw in raw_calls {
        let raw = raw.as_object().ok_or_else(|| StorageError::InvalidData {
            reason: "tool batch calls must be objects".to_string(),
        })?;
        let tool_call_id = required_field(raw.get("call_id"), "tool call ID")?;
        let tool_name = required_field(raw.get("name"), "tool name")?;
        let effect = serde_json::from_value(raw.get("effect").cloned().ok_or_else(|| {
            StorageError::InvalidData {
                reason: format!("tool call {tool_call_id} has no effect classification"),
            }
        })?)
        .map_err(|error| StorageError::InvalidData {
            reason: format!("tool call {tool_call_id} effect is invalid: {error}"),
        })?;
        let arguments = raw
            .get("arguments")
            .cloned()
            .filter(Value::is_object)
            .ok_or_else(|| StorageError::InvalidData {
                reason: format!("tool call {tool_call_id} arguments must be an object"),
            })?;
        calls.push(EnvelopeCall {
            tool_call_id,
            tool_name,
            effect,
            arguments,
        });
    }
    Ok(BatchEnvelope {
        project_id,
        capability_snapshot_ref: capability_snapshot_ref.to_string(),
        calls,
    })
}

fn required_field(value: Option<&Value>, label: &str) -> StorageResult<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| StorageError::InvalidData {
            reason: format!("{label} must not be empty"),
        })
}

fn validate_existing_execution(
    existing: &ToolExecution,
    expected: &ToolExecution,
) -> StorageResult<()> {
    if existing.invocation_id != expected.invocation_id
        || existing.run_id != expected.run_id
        || existing.batch_id != expected.batch_id
        || existing.tool_call_id != expected.tool_call_id
        || existing.tool_name != expected.tool_name
        || existing.effect != expected.effect
        || existing.arguments_digest != expected.arguments_digest
    {
        return invalid_data("stable invocation ID is bound to different tool input");
    }
    existing
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored tool execution is invalid: {error}"),
        })
}

#[derive(Debug, Clone)]
pub struct BeginToolExecutionRequest {
    pub scope: RecordScope,
    pub invocation_id: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BeginToolExecutionResult {
    Execute(ToolExecutionStateRecord),
    AlreadyCompleted(ToolExecutionStateRecord),
    AwaitingApproval(ToolExecutionStateRecord),
    NeedsReview(ToolExecutionStateRecord),
}

/// Records `started` before returning `Execute`. Re-entering a started read is
/// safe and returns `Execute`; re-entering any irreversible call records an
/// unknown outcome and requires review instead of replaying it.
pub async fn begin_tool_execution(
    storage: &dyn ClientStorage,
    request: BeginToolExecutionRequest,
) -> StorageResult<BeginToolExecutionResult> {
    let mut operation = BeginToolExecutionOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "begin tool execution returned no result".to_string(),
    })
}

struct BeginToolExecutionOperation {
    request: Option<BeginToolExecutionRequest>,
    result: Option<BeginToolExecutionResult>,
}

#[async_trait]
impl StorageTransaction for BeginToolExecutionOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "begin tool execution request was already consumed".to_string(),
        })?;
        let query = RecordQuery {
            scope: request.scope,
            id: request.invocation_id,
        };
        let mut record = repositories
            .tool_executions()
            .get(&query)
            .await?
            .ok_or(StorageError::NotFound)?;
        record
            .execution
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("stored tool execution is invalid: {error}"),
            })?;
        self.result = Some(match record.execution.status {
            ToolExecutionStatus::Requested | ToolExecutionStatus::Approved => {
                let revision = record.metadata.revision;
                record.execution.status = ToolExecutionStatus::Started;
                record.execution.started_at = Some(request.now);
                let record = repositories
                    .tool_executions()
                    .put(PutRecord {
                        record,
                        expected_revision: Some(revision),
                    })
                    .await?;
                append_tool_snapshot(repositories, &record).await?;
                BeginToolExecutionResult::Execute(record)
            }
            ToolExecutionStatus::Started if record.execution.effect.can_replay_after_started() => {
                BeginToolExecutionResult::Execute(record)
            }
            ToolExecutionStatus::Started => {
                let revision = record.metadata.revision;
                record.execution.status = ToolExecutionStatus::OutcomeUnknown;
                let record = repositories
                    .tool_executions()
                    .put(PutRecord {
                        record,
                        expected_revision: Some(revision),
                    })
                    .await?;
                append_tool_snapshot(repositories, &record).await?;
                persist_tool_semantic_message(repositories, &record, request.now).await?;
                BeginToolExecutionResult::NeedsReview(record)
            }
            ToolExecutionStatus::Succeeded | ToolExecutionStatus::Failed => {
                BeginToolExecutionResult::AlreadyCompleted(record)
            }
            ToolExecutionStatus::Rejected => BeginToolExecutionResult::AlreadyCompleted(record),
            ToolExecutionStatus::AwaitingApproval => {
                BeginToolExecutionResult::AwaitingApproval(record)
            }
            ToolExecutionStatus::OutcomeUnknown => BeginToolExecutionResult::NeedsReview(record),
        });
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DecideToolApprovalRequest {
    pub scope: RecordScope,
    pub run_id: String,
    pub invocation_id: String,
    pub decision: ToolApprovalDecision,
    pub reason: Option<String>,
    pub now: DateTime<Utc>,
}

/// Commits a per-invocation approval decision before any tool I/O can begin.
/// Repeating the exact decision is idempotent; a conflicting decision is
/// rejected and can never overwrite a started or terminal execution.
pub async fn decide_tool_approval(
    storage: &dyn ClientStorage,
    request: DecideToolApprovalRequest,
) -> StorageResult<ToolExecutionStateRecord> {
    let mut operation = DecideToolApprovalOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "tool approval decision returned no result".to_string(),
    })
}

struct DecideToolApprovalOperation {
    request: Option<DecideToolApprovalRequest>,
    result: Option<ToolExecutionStateRecord>,
}

#[async_trait]
impl StorageTransaction for DecideToolApprovalOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "tool approval request was already consumed".to_string(),
        })?;
        if request
            .reason
            .as_deref()
            .is_some_and(|reason| reason.trim().is_empty())
        {
            return invalid_data("tool approval reason must not be blank");
        }
        let mut record = repositories
            .tool_executions()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: request.invocation_id,
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        record
            .execution
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("stored tool execution is invalid: {error}"),
            })?;
        if record.execution.run_id != request.run_id {
            return invalid_data("tool approval run does not match the durable execution");
        }
        if !record.execution.effect.requires_approval() {
            return invalid_data("read-only tools do not accept approval decisions");
        }
        let target = match request.decision {
            ToolApprovalDecision::Approve => ToolExecutionStatus::Approved,
            ToolApprovalDecision::Reject => ToolExecutionStatus::Rejected,
        };
        if record.execution.status != ToolExecutionStatus::AwaitingApproval {
            let same_decision = match request.decision {
                ToolApprovalDecision::Approve => matches!(
                    record.execution.status,
                    ToolExecutionStatus::Approved
                        | ToolExecutionStatus::Started
                        | ToolExecutionStatus::Succeeded
                        | ToolExecutionStatus::Failed
                        | ToolExecutionStatus::OutcomeUnknown
                ),
                ToolApprovalDecision::Reject => {
                    record.execution.status == ToolExecutionStatus::Rejected
                }
            };
            if same_decision && record.execution.approval_reason == request.reason {
                self.result = Some(record);
                return Ok(());
            }
            return invalid_data("tool approval decision conflicts with durable execution state");
        }

        let revision = record.metadata.revision;
        record.execution.status = target;
        record.execution.approval_decided_at = Some(request.now);
        record.execution.approval_reason = request.reason.clone();
        if request.decision == ToolApprovalDecision::Reject {
            record.execution.bounded_result = Some(json!({
                "type": "tool_rejected",
                "reason": request.reason,
            }));
            record.execution.completed_at = Some(request.now);
        }
        record
            .execution
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("tool approval decision is invalid: {error}"),
            })?;
        let record = repositories
            .tool_executions()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        append_tool_snapshot(repositories, &record).await?;
        if request.decision == ToolApprovalDecision::Reject {
            persist_tool_semantic_message(repositories, &record, request.now).await?;
        }
        wake_tool_batch_after_approvals(
            repositories,
            &request.scope,
            &record.execution.run_id,
            &record.execution.batch_id,
            request.now,
        )
        .await?;
        self.result = Some(record);
        Ok(())
    }
}

async fn wake_tool_batch_after_approvals(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_id: &str,
    batch_id: &str,
    now: DateTime<Utc>,
) -> StorageResult<()> {
    let mut cursor = None;
    loop {
        let page = repositories
            .tool_executions()
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        if page.records.iter().any(|record| {
            record.execution.run_id == run_id
                && record.execution.batch_id == batch_id
                && record.execution.status == ToolExecutionStatus::AwaitingApproval
        }) {
            return Ok(());
        }
        let Some(next) = page.next_cursor else {
            break;
        };
        if cursor.as_deref() == Some(next.as_str()) {
            return invalid_data("tool execution pagination cursor did not advance");
        }
        cursor = Some(next);
    }

    let run = repositories
        .agent_runs()
        .get(&RecordQuery {
            scope: scope.clone(),
            id: run_id.to_string(),
        })
        .await?
        .ok_or(StorageError::NotFound)?;
    if run.run.status != LocalAgentRunStatus::WaitingToolResult
        || run.run.pending_batch_id.as_deref() != Some(batch_id)
    {
        return invalid_data("approved tool batch no longer matches the active Run");
    }

    let mut cursor = None;
    loop {
        let page = repositories
            .agent_events()
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        if let Some(mut event) = page.records.into_iter().find(|event| {
            event.event.run_id == run_id
                && event.event.event_type == LocalAgentEventType::ToolBatchRequested
                && event.event.expected_version == run.run.version
                && matches!(
                    event.event.status,
                    LocalAgentEventStatus::Pending | LocalAgentEventStatus::Claimed
                )
        }) {
            if event.event.status == LocalAgentEventStatus::Pending {
                let revision = event.metadata.revision;
                event.event.available_at = now;
                repositories
                    .agent_events()
                    .put(PutRecord {
                        record: event,
                        expected_revision: Some(revision),
                    })
                    .await?;
            }
            return Ok(());
        }
        let Some(next) = page.next_cursor else {
            return invalid_data("approved tool batch has no resumable event");
        };
        if cursor.as_deref() == Some(next.as_str()) {
            return invalid_data("tool event pagination cursor did not advance");
        }
        cursor = Some(next);
    }
}

#[derive(Debug, Clone)]
pub struct CompleteToolExecutionRequest {
    pub scope: RecordScope,
    pub invocation_id: String,
    pub status: ToolExecutionStatus,
    pub bounded_result: Value,
    pub now: DateTime<Utc>,
}

pub async fn complete_tool_execution(
    storage: &dyn ClientStorage,
    request: CompleteToolExecutionRequest,
) -> StorageResult<ToolExecutionStateRecord> {
    if !matches!(
        request.status,
        ToolExecutionStatus::Succeeded | ToolExecutionStatus::Failed
    ) {
        return invalid_data("tool completion status must be succeeded or failed");
    }
    let mut operation = CompleteToolExecutionOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "tool completion returned no result".to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct MarkToolOutcomeUnknownRequest {
    pub scope: RecordScope,
    pub invocation_id: String,
    pub now: DateTime<Utc>,
}

/// Used when the local transport fails after an irreversible invocation was
/// durably started and cannot prove whether the external side effect happened.
pub async fn mark_tool_outcome_unknown(
    storage: &dyn ClientStorage,
    request: MarkToolOutcomeUnknownRequest,
) -> StorageResult<ToolExecutionStateRecord> {
    let mut operation = MarkToolOutcomeUnknownOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "mark tool outcome unknown returned no result".to_string(),
    })
}

struct MarkToolOutcomeUnknownOperation {
    request: Option<MarkToolOutcomeUnknownRequest>,
    result: Option<ToolExecutionStateRecord>,
}

#[async_trait]
impl StorageTransaction for MarkToolOutcomeUnknownOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "mark tool outcome unknown request was already consumed".to_string(),
        })?;
        let query = RecordQuery {
            scope: request.scope,
            id: request.invocation_id,
        };
        let mut record = repositories
            .tool_executions()
            .get(&query)
            .await?
            .ok_or(StorageError::NotFound)?;
        if record.execution.status == ToolExecutionStatus::OutcomeUnknown {
            persist_tool_semantic_message(repositories, &record, request.now).await?;
            self.result = Some(record);
            return Ok(());
        }
        if record.execution.status != ToolExecutionStatus::Started
            || record.execution.effect.can_replay_after_started()
        {
            return invalid_data("only a started irreversible tool can have an unknown outcome");
        }
        let revision = record.metadata.revision;
        record.execution.status = ToolExecutionStatus::OutcomeUnknown;
        let record = repositories
            .tool_executions()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        append_tool_snapshot(repositories, &record).await?;
        persist_tool_semantic_message(repositories, &record, request.now).await?;
        self.result = Some(record);
        Ok(())
    }
}

struct CompleteToolExecutionOperation {
    request: Option<CompleteToolExecutionRequest>,
    result: Option<ToolExecutionStateRecord>,
}

#[async_trait]
impl StorageTransaction for CompleteToolExecutionOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "tool completion request was already consumed".to_string(),
        })?;
        let query = RecordQuery {
            scope: request.scope,
            id: request.invocation_id,
        };
        let mut record = repositories
            .tool_executions()
            .get(&query)
            .await?
            .ok_or(StorageError::NotFound)?;
        if matches!(
            record.execution.status,
            ToolExecutionStatus::Succeeded | ToolExecutionStatus::Failed
        ) {
            if record.execution.status == request.status
                && record.execution.bounded_result.as_ref() == Some(&request.bounded_result)
            {
                persist_tool_semantic_message(repositories, &record, request.now).await?;
                self.result = Some(record);
                return Ok(());
            }
            return invalid_data("completed tool execution cannot be overwritten");
        }
        if record.execution.status != ToolExecutionStatus::Started {
            return invalid_data("only a started tool execution can complete");
        }
        let revision = record.metadata.revision;
        record.execution.status = request.status;
        record.execution.bounded_result = Some(request.bounded_result);
        record.execution.completed_at = Some(request.now);
        record
            .execution
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("tool completion is invalid: {error}"),
            })?;
        let record = repositories
            .tool_executions()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        append_tool_snapshot(repositories, &record).await?;
        persist_tool_semantic_message(repositories, &record, request.now).await?;
        self.result = Some(record);
        Ok(())
    }
}

async fn persist_tool_semantic_message(
    repositories: &mut dyn TransactionRepositories,
    execution_record: &ToolExecutionStateRecord,
    now: DateTime<Utc>,
) -> StorageResult<()> {
    let execution = &execution_record.execution;
    if !matches!(
        execution.status,
        ToolExecutionStatus::Succeeded
            | ToolExecutionStatus::Failed
            | ToolExecutionStatus::Rejected
            | ToolExecutionStatus::OutcomeUnknown
    ) {
        return invalid_data("only a terminal tool outcome can become a semantic message");
    }
    let run = repositories
        .agent_runs()
        .get(&RecordQuery {
            scope: execution_record.metadata.scope.clone(),
            id: execution.run_id.clone(),
        })
        .await?
        .ok_or(StorageError::NotFound)?;
    let record_id = stable_digest_id(
        "tool-result",
        &[
            execution_record.metadata.scope.owner_user_id.as_str(),
            execution.invocation_id.as_str(),
        ],
    );
    let existing_identity = repositories
        .agent_messages()
        .get(&RecordQuery {
            scope: execution_record.metadata.scope.clone(),
            id: record_id.clone(),
        })
        .await?
        .map(|record| (record.message.sequence, record.message.created_at));
    let (sequence, created_at) = match existing_identity {
        Some(identity) => identity,
        None => (
            next_semantic_message_sequence(
                repositories,
                &execution_record.metadata.scope,
                &run.run.owner_entity_id,
            )
            .await?,
            now,
        ),
    };
    let content = match execution.status {
        ToolExecutionStatus::Succeeded
        | ToolExecutionStatus::Failed
        | ToolExecutionStatus::Rejected => execution
            .bounded_result
            .as_ref()
            .map(Value::to_string)
            .ok_or_else(|| StorageError::InvalidData {
                reason: "completed tool execution has no bounded result".to_string(),
            })?,
        ToolExecutionStatus::OutcomeUnknown => {
            "Tool outcome is unknown and requires human review.".to_string()
        }
        _ => unreachable!("terminal status was checked above"),
    };
    persist_semantic_message(
        repositories,
        RecordSemanticMessageRequest {
            scope: execution_record.metadata.scope.clone(),
            message: AgentMessage {
                record_id,
                run_id: execution.run_id.clone(),
                thread_id: run.run.owner_entity_id,
                turn_id: execution.batch_id.clone(),
                sequence,
                role: AgentMessageRole::Tool,
                content: Some(content),
                reasoning: None,
                structured_payload: Some(json!({
                    "type": "tool_execution_result",
                    "invocation_id": execution.invocation_id,
                    "tool_name": execution.tool_name,
                    "effect": execution.effect,
                    "status": execution.status,
                    "result": execution.bounded_result,
                })),
                tool_call_id: Some(execution.tool_call_id.clone()),
                response_id: None,
                message_mode: MessageMode::Semantic,
                message_source: "local_tool_runtime".to_string(),
                memory_sync_status: MemorySyncStatus::Pending,
                created_at,
            },
            origin_device_id: execution_record.metadata.origin_device_id.clone(),
            now,
        },
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolBatchExecutionState {
    pub records: Vec<ToolExecutionStateRecord>,
    pub all_completed: bool,
    pub awaiting_approval: bool,
    pub outcome_unknown: bool,
}

pub async fn inspect_tool_batch(
    storage: &dyn ClientStorage,
    scope: RecordScope,
    batch: &PreparedToolBatch,
) -> StorageResult<ToolBatchExecutionState> {
    let mut operation = InspectToolBatchOperation {
        scope,
        invocation_ids: batch
            .calls
            .iter()
            .map(|call| call.invocation_id.clone())
            .collect(),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "tool batch inspection returned no result".to_string(),
    })
}

struct InspectToolBatchOperation {
    scope: RecordScope,
    invocation_ids: Vec<String>,
    result: Option<ToolBatchExecutionState>,
}

#[async_trait]
impl StorageTransaction for InspectToolBatchOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut records = Vec::with_capacity(self.invocation_ids.len());
        for invocation_id in &self.invocation_ids {
            let record = repositories
                .tool_executions()
                .get(&RecordQuery {
                    scope: self.scope.clone(),
                    id: invocation_id.clone(),
                })
                .await?
                .ok_or(StorageError::NotFound)?;
            records.push(record);
        }
        self.result = Some(ToolBatchExecutionState {
            all_completed: records.iter().all(|record| {
                matches!(
                    record.execution.status,
                    ToolExecutionStatus::Succeeded
                        | ToolExecutionStatus::Failed
                        | ToolExecutionStatus::Rejected
                )
            }),
            awaiting_approval: records
                .iter()
                .any(|record| record.execution.status == ToolExecutionStatus::AwaitingApproval),
            outcome_unknown: records
                .iter()
                .any(|record| record.execution.status == ToolExecutionStatus::OutcomeUnknown),
            records,
        });
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalToolInvocation {
    pub invocation_id: String,
    pub run_id: String,
    pub batch_id: String,
    pub source_turn_id: String,
    pub project_id: Option<String>,
    pub capability_snapshot_ref: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub effect: ToolEffect,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalToolOutcome {
    pub status: ToolExecutionStatus,
    pub bounded_result: Value,
}

impl LocalToolOutcome {
    pub fn succeeded(bounded_result: Value) -> Self {
        Self {
            status: ToolExecutionStatus::Succeeded,
            bounded_result,
        }
    }

    pub fn failed(bounded_result: Value) -> Self {
        Self {
            status: ToolExecutionStatus::Failed,
            bounded_result,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if matches!(
            self.status,
            ToolExecutionStatus::Succeeded | ToolExecutionStatus::Failed
        ) {
            Ok(())
        } else {
            Err("local tool outcome must be succeeded or failed".to_string())
        }
    }
}

#[async_trait]
pub trait LocalToolRuntime: Send + Sync {
    /// Executes one already-started invocation. The runtime must return a
    /// bounded, sanitized result and must not alter the frozen project scope.
    async fn execute(
        &self,
        invocation: LocalToolInvocation,
        cancellation: CancellationToken,
    ) -> Result<LocalToolOutcome, String>;
}

pub fn build_local_tool_invocation(
    batch: &PreparedToolBatch,
    call: &PreparedToolCall,
) -> LocalToolInvocation {
    LocalToolInvocation {
        invocation_id: call.invocation_id.clone(),
        run_id: batch.run_id.clone(),
        batch_id: batch.batch_id.clone(),
        source_turn_id: batch.source_turn_id.clone(),
        project_id: batch.project_id.clone(),
        capability_snapshot_ref: batch.capability_snapshot_ref.clone(),
        tool_call_id: call.tool_call_id.clone(),
        tool_name: call.tool_name.clone(),
        effect: call.effect,
        arguments: call.arguments.clone(),
    }
}

pub fn validate_local_tool_outcome(outcome: &LocalToolOutcome) -> Result<(), String> {
    outcome.validate()
}

fn stable_invocation_id(run_id: &str, batch_id: &str, tool_call_id: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [run_id, batch_id, tool_call_id] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("tool:{:x}", hasher.finalize())
}

fn invalid_data<T>(reason: impl Into<String>) -> StorageResult<T> {
    Err(StorageError::InvalidData {
        reason: reason.into(),
    })
}
