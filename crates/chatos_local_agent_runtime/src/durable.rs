// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, ListQuery, PutRecord,
    RecordMetadata, RecordQuery, RecordScope, StorageError, StorageResult, StorageTransaction,
    ToolExecutionStateRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRunStatus,
    ModelStepCompletion, ToolExecutionStatus,
};
use chrono::{DateTime, Utc};
use serde_json::json;

use crate::pagination::advance_cursor;
use crate::ui_events::{append_run_snapshot, append_tool_snapshot};
use crate::{reduce_claimed_event, ReducerPolicy, Reduction, StepEvidence};

#[derive(Debug, Clone)]
pub struct EventClaimRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub device_id: String,
    pub claim_token: String,
    pub now: DateTime<Utc>,
    pub claim_until: DateTime<Utc>,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptLimitDisposition {
    RunFailed,
    NeedsReview,
    StaleEventDiscarded,
    TerminalEventDiscarded,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventClaimResult {
    Acquired(Box<AgentEventStateRecord>),
    NotAvailable,
    AlreadyFinished,
    AttemptsExhausted {
        disposition: AttemptLimitDisposition,
    },
}

pub async fn claim_event(
    storage: &dyn ClientStorage,
    request: EventClaimRequest,
) -> StorageResult<EventClaimResult> {
    if request.claim_until <= request.now {
        return Err(StorageError::InvalidData {
            reason: "event claim lease must end after the claim time".to_string(),
        });
    }
    if request.max_attempts == 0 {
        return Err(StorageError::InvalidData {
            reason: "event max attempts must be greater than zero".to_string(),
        });
    }
    let mut operation = ClaimEventOperation {
        request,
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "event claim completed without a result".to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct RenewEventClaimRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub device_id: String,
    pub claim_token: String,
    pub now: DateTime<Utc>,
    pub claim_until: DateTime<Utc>,
}

/// Extends only the exact active lease. A stale worker can never recover a
/// lease after another device or claim token has acquired the event.
pub async fn renew_event_claim(
    storage: &dyn ClientStorage,
    request: RenewEventClaimRequest,
) -> StorageResult<AgentEventStateRecord> {
    if request.claim_until <= request.now {
        return Err(StorageError::InvalidData {
            reason: "renewed event lease must end after the renewal time".to_string(),
        });
    }
    let mut operation = RenewEventClaimOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "event claim renewal returned no event".to_string(),
    })
}

struct RenewEventClaimOperation {
    request: Option<RenewEventClaimRequest>,
    result: Option<AgentEventStateRecord>,
}

#[async_trait]
impl StorageTransaction for RenewEventClaimOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "event claim renewal request was already consumed".to_string(),
        })?;
        let query = RecordQuery {
            scope: request.scope,
            id: request.event_id,
        };
        let mut record = repositories
            .agent_events()
            .get(&query)
            .await?
            .ok_or(StorageError::NotFound)?;
        if record.event.status != LocalAgentEventStatus::Claimed
            || record.event.claimed_by_device_id.as_deref() != Some(request.device_id.as_str())
            || record.event.claim_token.as_deref() != Some(request.claim_token.as_str())
            || record
                .event
                .claim_until
                .is_none_or(|deadline| deadline < request.now)
        {
            return Err(StorageError::Conflict {
                actual_revision: record.metadata.revision,
            });
        }
        let revision = record.metadata.revision;
        record.event.claim_until = Some(request.claim_until);
        self.result = Some(
            repositories
                .agent_events()
                .put(PutRecord {
                    record,
                    expected_revision: Some(revision),
                })
                .await?,
        );
        Ok(())
    }
}

struct ClaimEventOperation {
    request: EventClaimRequest,
    result: Option<EventClaimResult>,
}

#[async_trait]
impl StorageTransaction for ClaimEventOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = RecordQuery {
            scope: self.request.scope.clone(),
            id: self.request.event_id.clone(),
        };
        let Some(mut record) = repositories.agent_events().get(&query).await? else {
            return Err(StorageError::NotFound);
        };
        let claimable = match record.event.status {
            LocalAgentEventStatus::Pending => record.event.available_at <= self.request.now,
            LocalAgentEventStatus::Claimed => record
                .event
                .claim_until
                .is_some_and(|deadline| deadline <= self.request.now),
            LocalAgentEventStatus::Applied | LocalAgentEventStatus::Failed => {
                self.result = Some(EventClaimResult::AlreadyFinished);
                return Ok(());
            }
        };
        if !claimable {
            self.result = Some(EventClaimResult::NotAvailable);
            return Ok(());
        }
        if record.event.attempt_count >= self.request.max_attempts {
            self.result = Some(
                exhaust_event_attempts(
                    repositories,
                    &self.request.scope,
                    &mut record,
                    self.request.now,
                )
                .await?,
            );
            return Ok(());
        }
        let expected_revision = record.metadata.revision;
        record.event.status = LocalAgentEventStatus::Claimed;
        record.event.attempt_count =
            record
                .event
                .attempt_count
                .checked_add(1)
                .ok_or(StorageError::InvalidData {
                    reason: "event attempt counter overflow".to_string(),
                })?;
        record.event.claimed_by_device_id = Some(self.request.device_id.clone());
        record.event.claim_token = Some(self.request.claim_token.clone());
        record.event.claim_until = Some(self.request.claim_until);
        let claimed = repositories
            .agent_events()
            .put(PutRecord {
                record,
                expected_revision: Some(expected_revision),
            })
            .await?;
        self.result = Some(EventClaimResult::Acquired(Box::new(claimed)));
        Ok(())
    }
}

async fn exhaust_event_attempts(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    event_record: &mut AgentEventStateRecord,
    now: DateTime<Utc>,
) -> StorageResult<EventClaimResult> {
    let run_query = RecordQuery {
        scope: scope.clone(),
        id: event_record.event.run_id.clone(),
    };
    let mut run_record = repositories
        .agent_runs()
        .get(&run_query)
        .await?
        .ok_or(StorageError::NotFound)?;
    if run_record.run.status.is_terminal() {
        fail_event(repositories, event_record, "run is already terminal").await?;
        return Ok(EventClaimResult::AttemptsExhausted {
            disposition: AttemptLimitDisposition::TerminalEventDiscarded,
        });
    }
    if event_record.event.expected_version != run_record.run.version {
        fail_event(
            repositories,
            event_record,
            "event expected version no longer matches the run",
        )
        .await?;
        return Ok(EventClaimResult::AttemptsExhausted {
            disposition: AttemptLimitDisposition::StaleEventDiscarded,
        });
    }

    let unknown_tools =
        mark_unknown_irreversible_tools(repositories, scope, &event_record.event.run_id).await?;
    let run_revision = run_record.metadata.revision;
    run_record.run.version =
        run_record
            .run
            .version
            .checked_add(1)
            .ok_or(StorageError::InvalidData {
                reason: "run version overflow while exhausting event attempts".to_string(),
            })?;
    run_record.run.updated_at = now;
    run_record.run.pending_batch_id = None;
    let disposition = if unknown_tools.is_empty() {
        run_record.run.status = LocalAgentRunStatus::Failed;
        run_record.run.pending_interaction = None;
        run_record.run.terminal_outcome = Some(json!({
            "reason": "event_attempt_limit_exceeded",
            "event_id": event_record.event.event_id,
            "attempt_count": event_record.event.attempt_count,
        }));
        AttemptLimitDisposition::RunFailed
    } else {
        let unknown_count = unknown_tools.len();
        let invocation_ids = unknown_tools
            .iter()
            .take(100)
            .map(|record| record.execution.invocation_id.as_str())
            .collect::<Vec<_>>();
        run_record.run.status = LocalAgentRunStatus::NeedsReview;
        run_record.run.terminal_outcome = None;
        run_record.run.pending_interaction = Some(json!({
            "type": "review_unknown_tool_outcomes",
            "event_id": event_record.event.event_id,
            "unknown_count": unknown_count,
            "invocation_ids": invocation_ids,
        }));
        AttemptLimitDisposition::NeedsReview
    };
    let run_record = repositories
        .agent_runs()
        .put(PutRecord {
            record: run_record,
            expected_revision: Some(run_revision),
        })
        .await?;
    append_run_snapshot(repositories, &run_record).await?;
    fail_event(repositories, event_record, "event attempt limit exceeded").await?;

    if disposition == AttemptLimitDisposition::RunFailed {
        let terminal_id = stable_event_id(
            &run_record.run.run_id,
            run_record.run.version,
            LocalAgentEventType::RunTerminal,
            0,
        );
        repositories
            .agent_events()
            .put(PutRecord {
                record: AgentEventStateRecord {
                    metadata: RecordMetadata {
                        id: terminal_id.clone(),
                        scope: scope.clone(),
                        origin_device_id: event_record.metadata.origin_device_id.clone(),
                        revision: 0,
                        created_at: now,
                        updated_at: now,
                    },
                    event: LocalAgentEvent {
                        event_id: terminal_id,
                        run_id: run_record.run.run_id.clone(),
                        event_type: LocalAgentEventType::RunTerminal,
                        expected_version: run_record.run.version,
                        available_at: now,
                        status: LocalAgentEventStatus::Pending,
                        attempt_count: 0,
                        claimed_by_device_id: None,
                        claim_token: None,
                        claim_until: None,
                        causation_id: event_record.event.event_id.clone(),
                        correlation_id: event_record.event.correlation_id.clone(),
                        bounded_payload: json!({"reason": "event_attempt_limit_exceeded"}),
                        last_error: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
    }
    Ok(EventClaimResult::AttemptsExhausted { disposition })
}

async fn fail_event(
    repositories: &mut dyn TransactionRepositories,
    record: &mut AgentEventStateRecord,
    reason: &str,
) -> StorageResult<()> {
    let expected_revision = record.metadata.revision;
    record.event.status = LocalAgentEventStatus::Failed;
    record.event.claimed_by_device_id = None;
    record.event.claim_token = None;
    record.event.claim_until = None;
    record.event.last_error = Some(reason.to_string());
    repositories
        .agent_events()
        .put(PutRecord {
            record: record.clone(),
            expected_revision: Some(expected_revision),
        })
        .await?;
    Ok(())
}

async fn mark_unknown_irreversible_tools(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_id: &str,
) -> StorageResult<Vec<ToolExecutionStateRecord>> {
    let mut cursor = None;
    let mut unknown = Vec::new();
    loop {
        let page = repositories
            .tool_executions()
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        for mut record in page.records {
            if record.execution.run_id != run_id
                || !record.execution.effect.requires_durable_start()
                || !matches!(
                    record.execution.status,
                    ToolExecutionStatus::Started | ToolExecutionStatus::OutcomeUnknown
                )
            {
                continue;
            }
            if record.execution.status == ToolExecutionStatus::Started {
                let expected_revision = record.metadata.revision;
                record.execution.status = ToolExecutionStatus::OutcomeUnknown;
                record = repositories
                    .tool_executions()
                    .put(PutRecord {
                        record,
                        expected_revision: Some(expected_revision),
                    })
                    .await?;
                append_tool_snapshot(repositories, &record).await?;
            }
            unknown.push(record);
        }
        if !advance_cursor(&mut cursor, page.next_cursor)? {
            break;
        }
    }
    Ok(unknown)
}

pub struct ReduceAndCommitRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub claim_token: String,
    pub origin_device_id: String,
    pub evidence: StepEvidence,
    pub now: DateTime<Utc>,
    pub policy: ReducerPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommittedReduction {
    pub reduction: Reduction,
    pub run_record: AgentRunStateRecord,
    pub applied_event: AgentEventStateRecord,
    pub emitted_events: Vec<AgentEventStateRecord>,
}

pub async fn reduce_and_commit(
    storage: &dyn ClientStorage,
    request: ReduceAndCommitRequest,
) -> StorageResult<CommittedReduction> {
    let mut operation = ReduceAndCommitOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "event reduction completed without a committed result".to_string(),
    })
}

struct ReduceAndCommitOperation {
    request: Option<ReduceAndCommitRequest>,
    result: Option<CommittedReduction>,
}

#[async_trait]
impl StorageTransaction for ReduceAndCommitOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "reduction request was already consumed".to_string(),
        })?;
        let event_query = RecordQuery {
            scope: request.scope.clone(),
            id: request.event_id.clone(),
        };
        let mut event_record = repositories
            .agent_events()
            .get(&event_query)
            .await?
            .ok_or(StorageError::NotFound)?;
        if event_record.event.status != LocalAgentEventStatus::Claimed
            || event_record.event.claim_token.as_deref() != Some(request.claim_token.as_str())
            || event_record
                .event
                .claim_until
                .is_none_or(|deadline| deadline < request.now)
        {
            return Err(StorageError::Conflict {
                actual_revision: event_record.metadata.revision,
            });
        }
        let run_query = RecordQuery {
            scope: request.scope.clone(),
            id: event_record.event.run_id.clone(),
        };
        let mut run_record = repositories
            .agent_runs()
            .get(&run_query)
            .await?
            .ok_or(StorageError::NotFound)?;
        let reduction = reduce_claimed_event(
            &run_record.run,
            &event_record.event,
            request.evidence,
            request.now,
            request.policy,
        )
        .map_err(|error| StorageError::InvalidData {
            reason: format!("local Agent reduction failed: {error}"),
        })?;

        let run_revision = run_record.metadata.revision;
        run_record.run = reduction.run.clone();
        let run_record = repositories
            .agent_runs()
            .put(PutRecord {
                record: run_record,
                expected_revision: Some(run_revision),
            })
            .await?;
        append_run_snapshot(repositories, &run_record).await?;

        let event_revision = event_record.metadata.revision;
        event_record.event.status = LocalAgentEventStatus::Applied;
        event_record.event.claimed_by_device_id = None;
        event_record.event.claim_token = None;
        event_record.event.claim_until = None;
        let applied_event = repositories
            .agent_events()
            .put(PutRecord {
                record: event_record.clone(),
                expected_revision: Some(event_revision),
            })
            .await?;

        let mut emitted_records = Vec::with_capacity(reduction.emitted_events.len());
        for (index, emitted) in reduction.emitted_events.iter().enumerate() {
            let event_id = stable_event_id(
                &reduction.run.run_id,
                reduction.run.version,
                emitted.event_type,
                index,
            );
            let next = AgentEventStateRecord {
                metadata: RecordMetadata {
                    id: event_id.clone(),
                    scope: request.scope.clone(),
                    origin_device_id: request.origin_device_id.clone(),
                    revision: 0,
                    created_at: request.now,
                    updated_at: request.now,
                },
                event: LocalAgentEvent {
                    event_id,
                    run_id: reduction.run.run_id.clone(),
                    event_type: emitted.event_type,
                    expected_version: emitted.expected_version,
                    available_at: emitted.available_at,
                    status: LocalAgentEventStatus::Pending,
                    attempt_count: 0,
                    claimed_by_device_id: None,
                    claim_token: None,
                    claim_until: None,
                    causation_id: event_record.event.event_id.clone(),
                    correlation_id: event_record.event.correlation_id.clone(),
                    bounded_payload: emitted.bounded_payload.clone(),
                    last_error: None,
                },
            };
            emitted_records.push(
                repositories
                    .agent_events()
                    .put(PutRecord {
                        record: next,
                        expected_revision: None,
                    })
                    .await?,
            );
        }
        self.result = Some(CommittedReduction {
            reduction,
            run_record,
            applied_event,
            emitted_events: emitted_records,
        });
        Ok(())
    }
}

fn stable_event_id(
    run_id: &str,
    version: u64,
    event_type: LocalAgentEventType,
    index: usize,
) -> String {
    format!("{run_id}:{version}:{}:{index}", event_type_name(event_type))
}

#[derive(Debug, Clone)]
pub struct RecordModelStepCompletionRequest {
    pub scope: RecordScope,
    pub run_id: String,
    pub completion: ModelStepCompletion,
    pub origin_device_id: String,
    pub causation_id: String,
    pub correlation_id: String,
    pub now: DateTime<Utc>,
}

pub async fn record_model_step_completion(
    storage: &dyn ClientStorage,
    request: RecordModelStepCompletionRequest,
) -> StorageResult<AgentEventStateRecord> {
    request
        .completion
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: error.to_string(),
        })?;
    let mut operation = RecordModelStepCompletionOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "model completion transaction returned no event".to_string(),
    })
}

struct RecordModelStepCompletionOperation {
    request: Option<RecordModelStepCompletionRequest>,
    result: Option<AgentEventStateRecord>,
}

#[async_trait]
impl StorageTransaction for RecordModelStepCompletionOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "model completion request was already consumed".to_string(),
        })?;
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: request.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if run.run.status != LocalAgentRunStatus::ModelRunning {
            return Err(StorageError::InvalidData {
                reason: "model completion requires a model_running run".to_string(),
            });
        }
        let event_id = stable_event_id(
            request.run_id.as_str(),
            run.run.version,
            LocalAgentEventType::ModelStepCompleted,
            0,
        );
        let payload = serde_json::to_value(&request.completion).map_err(|error| {
            StorageError::InvalidData {
                reason: format!("model completion could not be serialized: {error}"),
            }
        })?;
        let query = RecordQuery {
            scope: request.scope.clone(),
            id: event_id.clone(),
        };
        if let Some(existing) = repositories.agent_events().get(&query).await? {
            if existing.event.expected_version == run.run.version
                && existing.event.bounded_payload == payload
            {
                self.result = Some(existing);
                return Ok(());
            }
            return Err(StorageError::Conflict {
                actual_revision: existing.metadata.revision,
            });
        }
        let record = AgentEventStateRecord {
            metadata: RecordMetadata {
                id: event_id.clone(),
                scope: request.scope,
                origin_device_id: request.origin_device_id,
                revision: 0,
                created_at: request.now,
                updated_at: request.now,
            },
            event: LocalAgentEvent {
                event_id,
                run_id: request.run_id,
                event_type: LocalAgentEventType::ModelStepCompleted,
                expected_version: run.run.version,
                available_at: request.now,
                status: LocalAgentEventStatus::Pending,
                attempt_count: 0,
                claimed_by_device_id: None,
                claim_token: None,
                claim_until: None,
                causation_id: request.causation_id,
                correlation_id: request.correlation_id,
                bounded_payload: payload,
                last_error: None,
            },
        };
        self.result = Some(
            repositories
                .agent_events()
                .put(PutRecord {
                    record,
                    expected_revision: None,
                })
                .await?,
        );
        Ok(())
    }
}

const fn event_type_name(event_type: LocalAgentEventType) -> &'static str {
    match event_type {
        LocalAgentEventType::RunStarted => "run_started",
        LocalAgentEventType::ModelStepRequested => "model_step_requested",
        LocalAgentEventType::ModelStepCompleted => "model_step_completed",
        LocalAgentEventType::ToolBatchRequested => "tool_batch_requested",
        LocalAgentEventType::ToolBatchCompleted => "tool_batch_completed",
        LocalAgentEventType::ContinuationRequested => "continuation_requested",
        LocalAgentEventType::RetryDue => "retry_due",
        LocalAgentEventType::PauseRequested => "pause_requested",
        LocalAgentEventType::ResumeRequested => "resume_requested",
        LocalAgentEventType::CancelRequested => "cancel_requested",
        LocalAgentEventType::MemorySyncDue => "memory_sync_due",
        LocalAgentEventType::RunTerminal => "run_terminal",
    }
}
