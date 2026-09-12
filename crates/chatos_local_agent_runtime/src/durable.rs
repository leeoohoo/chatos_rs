// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, ListQuery,
    ProviderContextStateRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, TaskRecord, ToolExecutionStateRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, ContextStrategy, FrozenSnapshotReference, LocalAgentEvent,
    LocalAgentEventStatus, LocalAgentEventType, LocalAgentRunStatus, MemorySyncStatus, MessageMode,
    ModelRuntimeDescriptor, ModelStepCompletion, ProviderContextItem, ToolExecutionStatus,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::digest::{canonical_json_digest, stable_digest_id};
use crate::memory_sync::{
    next_semantic_message_sequence, persist_semantic_message, RecordSemanticMessageRequest,
    RecordedSemanticMessage,
};
use crate::pagination::advance_cursor;
use crate::task_state::sync_task_from_run;
use crate::ui_events::{
    append_pending_user_interaction, append_run_snapshot, append_tool_snapshot,
};
use crate::{reduce_claimed_event, ReducerPolicy, Reduction, StepEvidence};

#[derive(Debug, Clone)]
pub struct CreateLocalAgentRunRequest {
    pub scope: RecordScope,
    pub run_id: String,
    pub profile_key: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub project_id: Option<String>,
    pub model_runtime_snapshot: ModelRuntimeDescriptor,
    pub prompt_revision: String,
    pub capability_snapshot_ref: String,
    pub origin_device_id: String,
    pub causation_id: String,
    pub deadline_at: Option<DateTime<Utc>>,
    pub initial_message: Option<InitialRunMessage>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitialRunMessage {
    pub record_id: String,
    pub turn_id: String,
    pub content: Option<String>,
    pub structured_payload: Option<serde_json::Value>,
    pub message_source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreatedLocalAgentRun {
    pub run_record: AgentRunStateRecord,
    pub start_event: AgentEventStateRecord,
    pub initial_message: Option<RecordedSemanticMessage>,
}

#[derive(Debug, Clone)]
pub struct CreateLocalAgentTaskRequest {
    pub run: CreateLocalAgentRunRequest,
    pub task_id: String,
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub project_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub prompt_snapshot: FrozenSnapshotReference,
    pub project_snapshot: FrozenSnapshotReference,
    pub capability_snapshot: FrozenSnapshotReference,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreatedLocalAgentTask {
    pub task_record: TaskRecord,
    pub run: CreatedLocalAgentRun,
}

pub async fn create_local_agent_run(
    storage: &dyn ClientStorage,
    request: CreateLocalAgentRunRequest,
) -> StorageResult<CreatedLocalAgentRun> {
    validate_create_run_request(&request)?;
    let mut operation = CreateLocalAgentRunOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "local Agent Run creation returned no result".to_string(),
    })
}

struct CreateLocalAgentRunOperation {
    request: Option<CreateLocalAgentRunRequest>,
    result: Option<CreatedLocalAgentRun>,
}

#[async_trait]
impl StorageTransaction for CreateLocalAgentRunOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "local Agent Run creation request was already consumed".to_string(),
        })?;
        self.result = Some(create_run_in_transaction(repositories, request).await?);
        Ok(())
    }
}

pub async fn create_local_agent_task(
    storage: &dyn ClientStorage,
    request: CreateLocalAgentTaskRequest,
) -> StorageResult<CreatedLocalAgentTask> {
    validate_create_task_request(&request)?;
    let mut operation = CreateLocalAgentTaskOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "local Agent Task creation returned no result".to_string(),
    })
}

struct CreateLocalAgentTaskOperation {
    request: Option<CreateLocalAgentTaskRequest>,
    result: Option<CreatedLocalAgentTask>,
}

#[async_trait]
impl StorageTransaction for CreateLocalAgentTaskOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "local Agent Task creation request was already consumed".to_string(),
        })?;
        let task = requested_task_record(&request);
        let query = RecordQuery {
            scope: task.metadata.scope.clone(),
            id: task.metadata.id.clone(),
        };
        let existing = repositories.tasks().get(&query).await?;
        let task_record = match existing {
            Some(existing) if task_identity_matches(&existing, &task) => existing,
            Some(existing) => {
                return Err(StorageError::Conflict {
                    actual_revision: existing.metadata.revision,
                });
            }
            None => {
                repositories
                    .tasks()
                    .put(PutRecord {
                        record: task,
                        expected_revision: None,
                    })
                    .await?
            }
        };
        let run = create_run_in_transaction(repositories, request.run).await?;
        self.result = Some(CreatedLocalAgentTask { task_record, run });
        Ok(())
    }
}

async fn create_run_in_transaction(
    repositories: &mut dyn TransactionRepositories,
    request: CreateLocalAgentRunRequest,
) -> StorageResult<CreatedLocalAgentRun> {
    let correlation_id = request
        .initial_message
        .as_ref()
        .map(|message| message.turn_id.clone())
        .unwrap_or_else(|| request.owner_entity_id.clone());
    let start_event_id = stable_event_id(
        request.run_id.as_str(),
        1,
        LocalAgentEventType::RunStarted,
        0,
    );
    let run_query = RecordQuery {
        scope: request.scope.clone(),
        id: request.run_id.clone(),
    };
    let existing_run = {
        let mut runs = repositories.agent_runs();
        runs.get(&run_query).await?
    };
    if let Some(existing_run) = existing_run {
        validate_existing_created_run(&existing_run, &request)?;
        let start_event = repositories
            .agent_events()
            .get(&RecordQuery {
                scope: request.scope,
                id: start_event_id,
            })
            .await?
            .ok_or_else(|| StorageError::InvalidData {
                reason: "existing local Agent Run has no durable start event".to_string(),
            })?;
        if start_event.event.run_id != existing_run.run.run_id
            || start_event.event.event_type != LocalAgentEventType::RunStarted
            || start_event.event.expected_version != 1
            || start_event.event.causation_id != request.causation_id
            || start_event.event.correlation_id != correlation_id
        {
            return Err(StorageError::Conflict {
                actual_revision: start_event.metadata.revision,
            });
        }
        let initial_message = persist_initial_run_message(
            repositories,
            &existing_run,
            request.initial_message,
            &request.origin_device_id,
            request.now,
        )
        .await?;
        return Ok(CreatedLocalAgentRun {
            run_record: existing_run,
            start_event,
            initial_message,
        });
    }

    let run = chatos_local_agent_protocol::LocalAgentRun {
        run_id: request.run_id.clone(),
        profile_key: request.profile_key,
        owner_user_id: request.scope.owner_user_id.clone(),
        owner_entity_type: request.owner_entity_type,
        owner_entity_id: request.owner_entity_id.clone(),
        project_id: request.project_id,
        status: LocalAgentRunStatus::Queued,
        version: 1,
        step_seq: 0,
        iteration: 0,
        retry_count: 0,
        model_config_id: request.model_runtime_snapshot.model_config_id.clone(),
        model_config_revision: request.model_runtime_snapshot.revision,
        context_strategy: request.model_runtime_snapshot.context_strategy,
        model_runtime_snapshot: request.model_runtime_snapshot,
        prompt_revision: request.prompt_revision,
        capability_snapshot_ref: request.capability_snapshot_ref,
        pending_batch_id: None,
        pending_interaction: None,
        terminal_outcome: None,
        deadline_at: request.deadline_at,
        created_at: request.now,
        updated_at: request.now,
    };
    run.validate().map_err(|error| StorageError::InvalidData {
        reason: format!("new local Agent Run is invalid: {error}"),
    })?;
    let run_record = repositories
        .agent_runs()
        .put(PutRecord {
            record: AgentRunStateRecord {
                metadata: RecordMetadata {
                    id: request.run_id.clone(),
                    scope: request.scope.clone(),
                    origin_device_id: request.origin_device_id.clone(),
                    revision: 0,
                    created_at: request.now,
                    updated_at: request.now,
                },
                run,
            },
            expected_revision: None,
        })
        .await?;
    let start_event = repositories
        .agent_events()
        .put(PutRecord {
            record: AgentEventStateRecord {
                metadata: RecordMetadata {
                    id: start_event_id.clone(),
                    scope: request.scope.clone(),
                    origin_device_id: request.origin_device_id,
                    revision: 0,
                    created_at: request.now,
                    updated_at: request.now,
                },
                event: LocalAgentEvent {
                    event_id: start_event_id,
                    run_id: request.run_id,
                    event_type: LocalAgentEventType::RunStarted,
                    expected_version: 1,
                    available_at: request.now,
                    status: LocalAgentEventStatus::Pending,
                    attempt_count: 0,
                    claimed_by_device_id: None,
                    claim_token: None,
                    claim_until: None,
                    causation_id: request.causation_id,
                    correlation_id,
                    bounded_payload: serde_json::Value::Null,
                    last_error: None,
                },
            },
            expected_revision: None,
        })
        .await?;
    let initial_message = persist_initial_run_message(
        repositories,
        &run_record,
        request.initial_message,
        &run_record.metadata.origin_device_id,
        request.now,
    )
    .await?;
    append_run_snapshot(repositories, &run_record).await?;
    Ok(CreatedLocalAgentRun {
        run_record,
        start_event,
        initial_message,
    })
}

fn validate_create_task_request(request: &CreateLocalAgentTaskRequest) -> StorageResult<()> {
    validate_create_run_request(&request.run)?;
    for value in [
        request.task_id.as_str(),
        request.source_thread_id.as_str(),
        request.source_turn_id.as_str(),
        request.project_id.as_str(),
        request.objective.as_str(),
    ] {
        if value.trim().is_empty() {
            return Err(StorageError::InvalidData {
                reason: "local Agent Task creation fields must not be empty".to_string(),
            });
        }
    }
    if request.acceptance_criteria.is_empty()
        || request
            .acceptance_criteria
            .iter()
            .any(|criterion| criterion.trim().is_empty())
    {
        return Err(StorageError::InvalidData {
            reason: "local Agent Task acceptance criteria must not be empty".to_string(),
        });
    }
    if request.run.profile_key != "task_runner"
        || request.run.owner_entity_type != "task"
        || request.run.owner_entity_id != request.task_id
        || request.run.project_id.as_deref() != Some(request.project_id.as_str())
        || request.run.prompt_revision != request.prompt_snapshot.revision
        || request.run.capability_snapshot_ref != request.capability_snapshot.snapshot_id
    {
        return Err(StorageError::InvalidData {
            reason: "local Agent Task does not match its frozen Run identity".to_string(),
        });
    }
    let Some(initial_message) = request.run.initial_message.as_ref() else {
        return Err(StorageError::InvalidData {
            reason: "local Agent Task requires an initial semantic message".to_string(),
        });
    };
    if initial_message.turn_id != request.source_turn_id
        || initial_message.content.as_deref() != Some(request.objective.as_str())
        || initial_message.structured_payload.is_none()
    {
        return Err(StorageError::InvalidData {
            reason: "local Agent Task initial message does not match the frozen task input"
                .to_string(),
        });
    }
    Ok(())
}

fn requested_task_record(request: &CreateLocalAgentTaskRequest) -> TaskRecord {
    TaskRecord {
        metadata: RecordMetadata {
            id: request.task_id.clone(),
            scope: request.run.scope.clone(),
            origin_device_id: request.run.origin_device_id.clone(),
            revision: 0,
            created_at: request.run.now,
            updated_at: request.run.now,
        },
        conversation_id: Some(request.source_thread_id.clone()),
        status: "queued".to_string(),
        state: json!({
            "schema_version": 1,
            "run_id": request.run.run_id,
            "source_thread_id": request.source_thread_id,
            "source_turn_id": request.source_turn_id,
            "project_id": request.project_id,
            "objective": request.objective,
            "acceptance_criteria": request.acceptance_criteria,
            "model_config_id": request.run.model_runtime_snapshot.model_config_id,
            "model_config_revision": request.run.model_runtime_snapshot.revision,
            "prompt_snapshot": request.prompt_snapshot,
            "project_snapshot": request.project_snapshot,
            "capability_snapshot": request.capability_snapshot,
        }),
    }
}

fn task_identity_matches(existing: &TaskRecord, requested: &TaskRecord) -> bool {
    const IMMUTABLE_TASK_FIELDS: &[&str] = &[
        "schema_version",
        "run_id",
        "source_thread_id",
        "source_turn_id",
        "project_id",
        "objective",
        "acceptance_criteria",
        "model_config_id",
        "model_config_revision",
        "prompt_snapshot",
        "project_snapshot",
        "capability_snapshot",
    ];
    existing.metadata.id == requested.metadata.id
        && existing.metadata.scope == requested.metadata.scope
        && existing.conversation_id == requested.conversation_id
        && IMMUTABLE_TASK_FIELDS.iter().all(|field| {
            existing.state.get(*field).is_some()
                && existing.state.get(*field) == requested.state.get(*field)
        })
}

fn validate_create_run_request(request: &CreateLocalAgentRunRequest) -> StorageResult<()> {
    for value in [
        request.scope.owner_user_id.as_str(),
        request.run_id.as_str(),
        request.profile_key.as_str(),
        request.owner_entity_type.as_str(),
        request.owner_entity_id.as_str(),
        request.prompt_revision.as_str(),
        request.capability_snapshot_ref.as_str(),
        request.origin_device_id.as_str(),
        request.causation_id.as_str(),
    ] {
        if value.trim().is_empty() {
            return Err(StorageError::InvalidData {
                reason: "local Agent Run creation identifiers must not be empty".to_string(),
            });
        }
    }
    request
        .model_runtime_snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("model runtime snapshot is invalid: {error}"),
        })?;
    if let Some(message) = &request.initial_message {
        for value in [
            message.record_id.as_str(),
            message.turn_id.as_str(),
            message.message_source.as_str(),
        ] {
            if value.trim().is_empty() {
                return Err(StorageError::InvalidData {
                    reason: "initial Run message identifiers must not be empty".to_string(),
                });
            }
        }
        if message
            .content
            .as_deref()
            .is_none_or(|content| content.trim().is_empty())
            && message.structured_payload.is_none()
        {
            return Err(StorageError::InvalidData {
                reason: "initial Run message must contain content or structured payload"
                    .to_string(),
            });
        }
    }
    Ok(())
}

async fn persist_initial_run_message(
    repositories: &mut dyn TransactionRepositories,
    run_record: &AgentRunStateRecord,
    initial: Option<InitialRunMessage>,
    origin_device_id: &str,
    now: DateTime<Utc>,
) -> StorageResult<Option<RecordedSemanticMessage>> {
    let Some(initial) = initial else {
        return Ok(None);
    };
    let existing_identity = repositories
        .agent_messages()
        .get(&RecordQuery {
            scope: run_record.metadata.scope.clone(),
            id: initial.record_id.clone(),
        })
        .await?
        .map(|record| (record.message.sequence, record.message.created_at));
    let (sequence, created_at) = match existing_identity {
        Some(identity) => identity,
        None => (
            next_semantic_message_sequence(
                repositories,
                &run_record.metadata.scope,
                &run_record.run.owner_entity_id,
            )
            .await?,
            now,
        ),
    };
    persist_semantic_message(
        repositories,
        RecordSemanticMessageRequest {
            scope: run_record.metadata.scope.clone(),
            message: AgentMessage {
                record_id: initial.record_id,
                run_id: run_record.run.run_id.clone(),
                thread_id: run_record.run.owner_entity_id.clone(),
                turn_id: initial.turn_id,
                sequence,
                role: AgentMessageRole::User,
                content: initial.content,
                reasoning: None,
                structured_payload: initial.structured_payload,
                tool_call_id: None,
                response_id: None,
                message_mode: MessageMode::Semantic,
                message_source: initial.message_source,
                memory_sync_status: MemorySyncStatus::Pending,
                created_at,
            },
            origin_device_id: origin_device_id.to_string(),
            now,
        },
    )
    .await
    .map(Some)
}

fn validate_existing_created_run(
    existing: &AgentRunStateRecord,
    request: &CreateLocalAgentRunRequest,
) -> StorageResult<()> {
    let run = &existing.run;
    if run.run_id == request.run_id
        && run.profile_key == request.profile_key
        && run.owner_user_id == request.scope.owner_user_id
        && run.owner_entity_type == request.owner_entity_type
        && run.owner_entity_id == request.owner_entity_id
        && run.project_id == request.project_id
        && run.model_runtime_snapshot == request.model_runtime_snapshot
        && run.prompt_revision == request.prompt_revision
        && run.capability_snapshot_ref == request.capability_snapshot_ref
        && run.deadline_at == request.deadline_at
    {
        Ok(())
    } else {
        Err(StorageError::Conflict {
            actual_revision: existing.metadata.revision,
        })
    }
}

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
    sync_task_from_run(repositories, &run_record).await?;
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
                || record.execution.effect.can_replay_after_started()
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
        sync_task_from_run(repositories, &run_record).await?;
        append_run_snapshot(repositories, &run_record).await?;
        append_pending_user_interaction(repositories, &run_record).await?;

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
pub struct BeginModelStepExecutionRequest {
    pub scope: RecordScope,
    pub event_id: String,
    pub claim_token: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BeganModelStepExecution {
    pub run_record: AgentRunStateRecord,
    pub request_event: AgentEventStateRecord,
}

/// Durably marks one claimed model request as running without consuming its
/// event. The same event remains the recovery lease until a formal model
/// result and its semantic records are committed.
pub async fn begin_model_step_execution(
    storage: &dyn ClientStorage,
    request: BeginModelStepExecutionRequest,
) -> StorageResult<BeganModelStepExecution> {
    if request.event_id.trim().is_empty() || request.claim_token.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "model step event and claim identifiers must not be empty".to_string(),
        });
    }
    let mut operation = BeginModelStepExecutionOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "begin model step transaction returned no result".to_string(),
    })
}

struct BeginModelStepExecutionOperation {
    request: Option<BeginModelStepExecutionRequest>,
    result: Option<BeganModelStepExecution>,
}

#[async_trait]
impl StorageTransaction for BeginModelStepExecutionOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "begin model step request was already consumed".to_string(),
        })?;
        let mut event = require_claimed_model_request(
            repositories,
            &request.scope,
            &request.event_id,
            &request.claim_token,
            request.now,
        )
        .await?;
        let mut run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: request.scope,
                id: event.event.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if event.event.expected_version != run.run.version {
            return Err(StorageError::Conflict {
                actual_revision: run.metadata.revision,
            });
        }
        if run.run.status == LocalAgentRunStatus::ModelRunning {
            self.result = Some(BeganModelStepExecution {
                run_record: run,
                request_event: event,
            });
            return Ok(());
        }
        if run.run.status != LocalAgentRunStatus::ModelReady {
            return Err(StorageError::InvalidData {
                reason: "model request can begin only from model_ready".to_string(),
            });
        }

        let run_revision = run.metadata.revision;
        run.run.version = run
            .run
            .version
            .checked_add(1)
            .ok_or(StorageError::InvalidData {
                reason: "Run version overflow while beginning model step".to_string(),
            })?;
        run.run.step_seq = run
            .run
            .step_seq
            .checked_add(1)
            .ok_or(StorageError::InvalidData {
                reason: "Run step sequence overflow while beginning model step".to_string(),
            })?;
        run.run.status = LocalAgentRunStatus::ModelRunning;
        run.run.updated_at = request.now;
        run.run
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("model-running Run is invalid: {error}"),
            })?;
        let run = repositories
            .agent_runs()
            .put(PutRecord {
                record: run,
                expected_revision: Some(run_revision),
            })
            .await?;
        sync_task_from_run(repositories, &run).await?;
        append_run_snapshot(repositories, &run).await?;

        let event_revision = event.metadata.revision;
        event.event.expected_version = run.run.version;
        let event = repositories
            .agent_events()
            .put(PutRecord {
                record: event,
                expected_revision: Some(event_revision),
            })
            .await?;
        self.result = Some(BeganModelStepExecution {
            run_record: run,
            request_event: event,
        });
        Ok(())
    }
}

async fn require_claimed_model_request(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    event_id: &str,
    claim_token: &str,
    now: DateTime<Utc>,
) -> StorageResult<AgentEventStateRecord> {
    let event = repositories
        .agent_events()
        .get(&RecordQuery {
            scope: scope.clone(),
            id: event_id.to_string(),
        })
        .await?
        .ok_or(StorageError::NotFound)?;
    if event.event.event_type != LocalAgentEventType::ModelStepRequested
        || event.event.status != LocalAgentEventStatus::Claimed
        || event.event.claim_token.as_deref() != Some(claim_token)
        || event
            .event
            .claim_until
            .is_none_or(|deadline| deadline < now)
    {
        return Err(StorageError::Conflict {
            actual_revision: event.metadata.revision,
        });
    }
    Ok(event)
}

#[derive(Debug, Clone)]
pub struct RecordModelStepCompletionRequest {
    pub scope: RecordScope,
    pub request_event_id: String,
    pub claim_token: String,
    pub completion: ModelStepCompletion,
    pub assistant_message: Option<CompletedAssistantMessage>,
    pub provider_context_commit: Option<DurableProviderContextCommit>,
    pub origin_device_id: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DurableProviderContextCommit {
    pub generation: u64,
    pub retained_items: Vec<DurableProviderContextItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DurableProviderContextItem {
    pub sequence: u64,
    pub item_type: String,
    pub encrypted_payload: String,
    pub payload_digest: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DurableModelStepCompletionPayload {
    pub completion: ModelStepCompletion,
    pub assistant_message_record_id: Option<String>,
    pub tool_call_message_record_id: Option<String>,
    pub provider_context_commit_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedAssistantMessage {
    pub record_id: String,
    pub turn_id: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub structured_payload: Option<serde_json::Value>,
    pub response_id: Option<String>,
    pub message_source: String,
}

pub async fn record_model_step_completion(
    storage: &dyn ClientStorage,
    request: RecordModelStepCompletionRequest,
) -> StorageResult<AgentEventStateRecord> {
    if [
        request.request_event_id.as_str(),
        request.claim_token.as_str(),
        request.origin_device_id.as_str(),
    ]
    .into_iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(StorageError::InvalidData {
            reason: "model completion request identifiers must not be empty".to_string(),
        });
    }
    request
        .completion
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: error.to_string(),
        })?;
    validate_completed_assistant_message(request.assistant_message.as_ref())?;
    validate_provider_context_commit(request.provider_context_commit.as_ref())?;
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
        let mut model_request = repositories
            .agent_events()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: request.request_event_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if model_request.event.event_type != LocalAgentEventType::ModelStepRequested {
            return Err(StorageError::InvalidData {
                reason: "model completion source must be a model_step_requested event".to_string(),
            });
        }
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: model_request.event.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let model_step_version = model_request.event.expected_version;
        let provider_context_commit_digest = request
            .provider_context_commit
            .as_ref()
            .map(provider_context_commit_identity_digest)
            .transpose()?;
        let tool_call_message_record_id = matches!(
            &request.completion.result,
            chatos_local_agent_protocol::ModelStepResult::ToolCommand(_)
        )
        .then(|| {
            stable_digest_id(
                "model-tool-calls",
                &[
                    request.scope.owner_user_id.as_str(),
                    model_request.event.event_id.as_str(),
                ],
            )
        });
        let payload = serde_json::to_value(DurableModelStepCompletionPayload {
            completion: request.completion.clone(),
            assistant_message_record_id: request
                .assistant_message
                .as_ref()
                .map(|message| message.record_id.clone()),
            tool_call_message_record_id,
            provider_context_commit_digest,
        })
        .map_err(|error| StorageError::InvalidData {
            reason: format!("model completion could not be serialized: {error}"),
        })?;
        let event_id = stable_event_id(
            model_request.event.run_id.as_str(),
            model_step_version,
            LocalAgentEventType::ModelStepCompleted,
            0,
        );
        let event_query = RecordQuery {
            scope: request.scope.clone(),
            id: event_id.clone(),
        };
        if model_request.event.status == LocalAgentEventStatus::Applied {
            let existing = repositories
                .agent_events()
                .get(&event_query)
                .await?
                .ok_or_else(|| StorageError::InvalidData {
                    reason: "applied model request has no durable completion event".to_string(),
                })?;
            if existing.event.event_type == LocalAgentEventType::ModelStepCompleted
                && existing.event.expected_version == model_step_version
                && existing.event.causation_id == model_request.event.event_id
                && existing.event.bounded_payload == payload
            {
                self.result = Some(existing);
                return Ok(());
            }
            return Err(StorageError::Conflict {
                actual_revision: existing.metadata.revision,
            });
        }
        if model_request.event.status != LocalAgentEventStatus::Claimed
            || model_request.event.claim_token.as_deref() != Some(request.claim_token.as_str())
            || model_request
                .event
                .claim_until
                .is_none_or(|deadline| deadline < request.now)
            || model_request.event.expected_version != run.run.version
        {
            return Err(StorageError::Conflict {
                actual_revision: model_request.metadata.revision,
            });
        }
        if run.run.status != LocalAgentRunStatus::ModelRunning {
            return Err(StorageError::InvalidData {
                reason: "model completion requires a model_running run".to_string(),
            });
        }
        persist_completed_assistant_message(
            repositories,
            &run,
            request.assistant_message,
            &request.origin_device_id,
            request.now,
        )
        .await?;
        persist_model_tool_call_message(
            repositories,
            &run,
            &model_request,
            &request.completion,
            &request.origin_device_id,
            request.now,
        )
        .await?;
        persist_provider_context_commit(
            repositories,
            &run,
            request.provider_context_commit,
            &request.origin_device_id,
            request.now,
        )
        .await?;
        if let Some(existing) = repositories.agent_events().get(&event_query).await? {
            return Err(StorageError::Conflict {
                actual_revision: existing.metadata.revision,
            });
        }
        let model_request_revision = model_request.metadata.revision;
        model_request.event.status = LocalAgentEventStatus::Applied;
        model_request.event.claimed_by_device_id = None;
        model_request.event.claim_token = None;
        model_request.event.claim_until = None;
        repositories
            .agent_events()
            .put(PutRecord {
                record: model_request.clone(),
                expected_revision: Some(model_request_revision),
            })
            .await?;
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
                run_id: model_request.event.run_id,
                event_type: LocalAgentEventType::ModelStepCompleted,
                expected_version: run.run.version,
                available_at: request.now,
                status: LocalAgentEventStatus::Pending,
                attempt_count: 0,
                claimed_by_device_id: None,
                claim_token: None,
                claim_until: None,
                causation_id: model_request.event.event_id,
                correlation_id: model_request.event.correlation_id,
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

fn validate_completed_assistant_message(
    message: Option<&CompletedAssistantMessage>,
) -> StorageResult<()> {
    let Some(message) = message else {
        return Ok(());
    };
    for value in [
        message.record_id.as_str(),
        message.turn_id.as_str(),
        message.message_source.as_str(),
    ] {
        if value.trim().is_empty() {
            return Err(StorageError::InvalidData {
                reason: "completed assistant message identifiers must not be empty".to_string(),
            });
        }
    }
    if message
        .content
        .as_deref()
        .is_none_or(|content| content.trim().is_empty())
        && message
            .reasoning
            .as_deref()
            .is_none_or(|reasoning| reasoning.trim().is_empty())
        && message.structured_payload.is_none()
    {
        return Err(StorageError::InvalidData {
            reason: "completed assistant message must contain semantic output".to_string(),
        });
    }
    Ok(())
}

fn validate_provider_context_commit(
    commit: Option<&DurableProviderContextCommit>,
) -> StorageResult<()> {
    let Some(commit) = commit else {
        return Ok(());
    };
    if commit.generation == 0 {
        return Err(StorageError::InvalidData {
            reason: "provider context generation must be positive".to_string(),
        });
    }
    let mut previous_sequence = 0;
    for item in &commit.retained_items {
        if item.sequence <= previous_sequence
            || item.item_type.trim().is_empty()
            || item.encrypted_payload.is_empty()
            || item.payload_digest.trim().is_empty()
        {
            return Err(StorageError::InvalidData {
                reason: "provider context items must be strictly ordered and complete".to_string(),
            });
        }
        previous_sequence = item.sequence;
    }
    Ok(())
}

fn provider_context_commit_identity_digest(
    commit: &DurableProviderContextCommit,
) -> StorageResult<String> {
    canonical_json_digest(&json!({
        "generation": commit.generation,
        "retained_items": commit.retained_items.iter().map(|item| json!({
            "sequence": item.sequence,
            "item_type": item.item_type,
            "payload_digest": item.payload_digest,
            "created_at": item.created_at,
        })).collect::<Vec<_>>(),
    }))
}

async fn persist_provider_context_commit(
    repositories: &mut dyn TransactionRepositories,
    run_record: &AgentRunStateRecord,
    commit: Option<DurableProviderContextCommit>,
    origin_device_id: &str,
    now: DateTime<Utc>,
) -> StorageResult<()> {
    let Some(commit) = commit else {
        return Ok(());
    };
    if run_record.run.context_strategy != ContextStrategy::ProviderNative {
        return Err(StorageError::InvalidData {
            reason: "Memory Engine runs cannot persist provider-native context".to_string(),
        });
    }
    let generation = commit.generation;
    let generation_text = generation.to_string();
    let mut desired = Vec::with_capacity(commit.retained_items.len());
    for item in commit.retained_items {
        let sequence_text = item.sequence.to_string();
        let item_id = stable_digest_id(
            "provider-context",
            &[
                run_record.run.run_id.as_str(),
                generation_text.as_str(),
                sequence_text.as_str(),
                item.payload_digest.as_str(),
            ],
        );
        let record = ProviderContextStateRecord {
            metadata: RecordMetadata {
                id: item_id.clone(),
                scope: run_record.metadata.scope.clone(),
                origin_device_id: origin_device_id.to_string(),
                revision: 0,
                created_at: now,
                updated_at: now,
            },
            item: ProviderContextItem {
                item_id,
                run_id: run_record.run.run_id.clone(),
                generation,
                sequence: item.sequence,
                provider: run_record.run.model_runtime_snapshot.provider.clone(),
                item_type: item.item_type,
                encrypted_payload: item.encrypted_payload,
                payload_digest: item.payload_digest,
                created_at: item.created_at,
            },
        };
        record
            .item
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("provider context item is invalid: {error}"),
            })?;
        desired.push(record);
    }

    let mut cursor = None;
    let mut existing_records = Vec::new();
    loop {
        let page = repositories
            .provider_context()
            .list(&ListQuery {
                scope: run_record.metadata.scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        existing_records.extend(
            page.records
                .into_iter()
                .filter(|record| record.item.run_id == run_record.run.run_id),
        );
        if !advance_cursor(&mut cursor, page.next_cursor)? {
            break;
        }
    }
    let desired_ids = desired
        .iter()
        .map(|record| record.metadata.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for record in existing_records {
        if !desired_ids.contains(record.metadata.id.as_str()) {
            repositories
                .provider_context()
                .delete(
                    &RecordQuery {
                        scope: record.metadata.scope,
                        id: record.metadata.id,
                    },
                    record.metadata.revision,
                )
                .await?;
        }
    }
    for record in desired {
        let query = RecordQuery {
            scope: record.metadata.scope.clone(),
            id: record.metadata.id.clone(),
        };
        let existing = {
            let mut context = repositories.provider_context();
            context.get(&query).await?
        };
        if let Some(existing) = existing {
            if provider_context_items_match(&existing.item, &record.item) {
                continue;
            }
            return Err(StorageError::Conflict {
                actual_revision: existing.metadata.revision,
            });
        }
        repositories
            .provider_context()
            .put(PutRecord {
                record,
                expected_revision: None,
            })
            .await?;
    }
    Ok(())
}

fn provider_context_items_match(
    existing: &ProviderContextItem,
    expected: &ProviderContextItem,
) -> bool {
    existing.item_id == expected.item_id
        && existing.run_id == expected.run_id
        && existing.generation == expected.generation
        && existing.sequence == expected.sequence
        && existing.provider == expected.provider
        && existing.item_type == expected.item_type
        && existing.payload_digest == expected.payload_digest
        && existing.created_at == expected.created_at
}

async fn persist_model_tool_call_message(
    repositories: &mut dyn TransactionRepositories,
    run_record: &AgentRunStateRecord,
    request_event: &AgentEventStateRecord,
    completion: &ModelStepCompletion,
    origin_device_id: &str,
    now: DateTime<Utc>,
) -> StorageResult<Option<RecordedSemanticMessage>> {
    let chatos_local_agent_protocol::ModelStepResult::ToolCommand(command) = &completion.result
    else {
        return Ok(None);
    };
    let calls = command
        .get("calls")
        .and_then(serde_json::Value::as_array)
        .filter(|calls| !calls.is_empty())
        .ok_or_else(|| StorageError::InvalidData {
            reason: "tool command completion must contain at least one call".to_string(),
        })?;
    if calls.iter().any(|call| !call.is_object()) {
        return Err(StorageError::InvalidData {
            reason: "tool command calls must be JSON objects".to_string(),
        });
    }
    let record_id = stable_digest_id(
        "model-tool-calls",
        &[
            run_record.metadata.scope.owner_user_id.as_str(),
            request_event.event.event_id.as_str(),
        ],
    );
    let existing_identity = repositories
        .agent_messages()
        .get(&RecordQuery {
            scope: run_record.metadata.scope.clone(),
            id: record_id.clone(),
        })
        .await?
        .map(|record| (record.message.sequence, record.message.created_at));
    let (sequence, created_at) = match existing_identity {
        Some(identity) => identity,
        None => (
            next_semantic_message_sequence(
                repositories,
                &run_record.metadata.scope,
                &run_record.run.owner_entity_id,
            )
            .await?,
            now,
        ),
    };
    persist_semantic_message(
        repositories,
        RecordSemanticMessageRequest {
            scope: run_record.metadata.scope.clone(),
            message: AgentMessage {
                record_id,
                run_id: run_record.run.run_id.clone(),
                thread_id: run_record.run.owner_entity_id.clone(),
                turn_id: request_event.event.correlation_id.clone(),
                sequence,
                role: AgentMessageRole::Assistant,
                content: None,
                reasoning: None,
                structured_payload: Some(json!({
                    "type": "model_tool_calls",
                    "tool_calls": calls,
                })),
                tool_call_id: None,
                response_id: None,
                message_mode: MessageMode::Semantic,
                message_source: "model_tool_calls".to_string(),
                memory_sync_status: MemorySyncStatus::Pending,
                created_at,
            },
            origin_device_id: origin_device_id.to_string(),
            now,
        },
    )
    .await
    .map(Some)
}

async fn persist_completed_assistant_message(
    repositories: &mut dyn TransactionRepositories,
    run_record: &AgentRunStateRecord,
    message: Option<CompletedAssistantMessage>,
    origin_device_id: &str,
    now: DateTime<Utc>,
) -> StorageResult<Option<RecordedSemanticMessage>> {
    let Some(message) = message else {
        return Ok(None);
    };
    let existing_identity = {
        let mut messages = repositories.agent_messages();
        messages
            .get(&RecordQuery {
                scope: run_record.metadata.scope.clone(),
                id: message.record_id.clone(),
            })
            .await?
            .map(|record| (record.message.sequence, record.message.created_at))
    };
    let (sequence, created_at) = match existing_identity {
        Some(identity) => identity,
        None => (
            next_semantic_message_sequence(
                repositories,
                &run_record.metadata.scope,
                &run_record.run.owner_entity_id,
            )
            .await?,
            now,
        ),
    };
    persist_semantic_message(
        repositories,
        RecordSemanticMessageRequest {
            scope: run_record.metadata.scope.clone(),
            message: AgentMessage {
                record_id: message.record_id,
                run_id: run_record.run.run_id.clone(),
                thread_id: run_record.run.owner_entity_id.clone(),
                turn_id: message.turn_id,
                sequence,
                role: AgentMessageRole::Assistant,
                content: message.content,
                reasoning: message.reasoning,
                structured_payload: message.structured_payload,
                tool_call_id: None,
                response_id: message.response_id,
                message_mode: MessageMode::Semantic,
                message_source: message.message_source,
                memory_sync_status: MemorySyncStatus::Pending,
                created_at,
            },
            origin_device_id: origin_device_id.to_string(),
            now,
        },
    )
    .await
    .map(Some)
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
