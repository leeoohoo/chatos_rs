// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordQuery, RecordScope, StorageError, StorageResult, StorageTransaction,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType};
use chrono::{DateTime, Utc};

use crate::{reduce_claimed_event, ReducerPolicy, Reduction, StepEvidence};

#[derive(Debug, Clone)]
pub struct EventClaimRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub device_id: String,
    pub claim_token: String,
    pub now: DateTime<Utc>,
    pub claim_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventClaimResult {
    Acquired(Box<AgentEventStateRecord>),
    NotAvailable,
    AlreadyFinished,
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
    let mut operation = ClaimEventOperation {
        request,
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "event claim completed without a result".to_string(),
    })
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
