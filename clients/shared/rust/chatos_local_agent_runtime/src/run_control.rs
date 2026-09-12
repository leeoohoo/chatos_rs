// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordQuery, RecordScope, StorageError, StorageResult, StorageTransaction,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType,
    LocalAgentRunStatus, MemorySyncStatus, MessageMode, UserInteractionAnswer,
    UserInteractionQuestion,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::attachments::{persist_message_attachments, safe_attachment_manifest};
use crate::digest::stable_digest_id;
use crate::memory_sync::{
    next_semantic_message_sequence, persist_semantic_message, RecordSemanticMessageRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunControlAction {
    Pause,
    Resume,
    Cancel,
}

impl RunControlAction {
    const fn event_type(self) -> LocalAgentEventType {
        match self {
            Self::Pause => LocalAgentEventType::PauseRequested,
            Self::Resume => LocalAgentEventType::ResumeRequested,
            Self::Cancel => LocalAgentEventType::CancelRequested,
        }
    }

    const fn event_name(self) -> &'static str {
        match self {
            Self::Pause => "pause_requested",
            Self::Resume => "resume_requested",
            Self::Cancel => "cancel_requested",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestRunControl {
    pub scope: RecordScope,
    pub run_id: String,
    pub expected_version: u64,
    pub action: RunControlAction,
    pub origin_device_id: String,
    pub causation_id: String,
    pub now: DateTime<Utc>,
}

pub async fn request_run_control(
    storage: &dyn ClientStorage,
    request: RequestRunControl,
) -> StorageResult<AgentEventStateRecord> {
    validate_request(&request)?;
    let mut operation = RequestRunControlOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "Run control transaction returned no event".to_string(),
    })
}

struct RequestRunControlOperation {
    request: Option<RequestRunControl>,
    result: Option<AgentEventStateRecord>,
}

#[async_trait]
impl StorageTransaction for RequestRunControlOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "Run control request was already consumed".to_string(),
        })?;
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: request.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if run.run.version != request.expected_version {
            return Err(StorageError::Conflict {
                actual_revision: run.run.version,
            });
        }
        validate_transition(run.run.status, request.action)?;

        self.result = Some(
            put_run_event(
                repositories,
                &request.scope,
                &run,
                request.action.event_type(),
                request.action.event_name(),
                &request.origin_device_id,
                &request.causation_id,
                request.now,
                Value::Null,
            )
            .await?,
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AnswerRunInteraction {
    pub scope: RecordScope,
    pub run_id: String,
    pub interaction_id: String,
    pub answer: UserInteractionAnswer,
    pub origin_device_id: String,
    pub causation_id: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnsweredRunInteraction {
    pub message_record_id: String,
    pub resume_event: AgentEventStateRecord,
}

pub async fn answer_run_interaction(
    storage: &dyn ClientStorage,
    request: AnswerRunInteraction,
) -> StorageResult<AnsweredRunInteraction> {
    validate_answer_request(&request)?;
    let mut operation = AnswerRunInteractionOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "user interaction answer returned no result".to_string(),
    })
}

struct AnswerRunInteractionOperation {
    request: Option<AnswerRunInteraction>,
    result: Option<AnsweredRunInteraction>,
}

#[async_trait]
impl StorageTransaction for AnswerRunInteractionOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "user interaction answer request was already consumed".to_string(),
        })?;
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: request.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        require_matching_interaction(&run, &request.interaction_id, &request.answer)?;
        let thread_id = run.run.owner_entity_id.clone();
        let message_record_id = stable_digest_id(
            "interaction-answer",
            &[
                request.scope.owner_user_id.as_str(),
                run.run.run_id.as_str(),
                request.interaction_id.as_str(),
            ],
        );
        let existing_identity = repositories
            .agent_messages()
            .get(&RecordQuery {
                scope: request.scope.clone(),
                id: message_record_id.clone(),
            })
            .await?
            .map(|record| (record.message.sequence, record.message.created_at));
        let (sequence, message_created_at) = match existing_identity {
            Some(identity) => identity,
            None => (
                next_semantic_message_sequence(repositories, &request.scope, &thread_id).await?,
                request.now,
            ),
        };
        let attachment_manifest = safe_attachment_manifest(&request.answer.attachments)?;
        let structured_payload = json!({
            "type": "user_interaction_answer",
            "interaction_id": request.interaction_id.clone(),
            "selected_option_ids": &request.answer.selected_option_ids,
            "attachments": attachment_manifest,
        });
        let recorded = persist_semantic_message(
            repositories,
            RecordSemanticMessageRequest {
                scope: request.scope.clone(),
                message: AgentMessage {
                    record_id: message_record_id.clone(),
                    run_id: run.run.run_id.clone(),
                    thread_id,
                    turn_id: request.interaction_id.clone(),
                    sequence,
                    role: AgentMessageRole::User,
                    content: request.answer.text.clone(),
                    reasoning: None,
                    structured_payload: Some(structured_payload),
                    tool_call_id: None,
                    response_id: None,
                    message_mode: MessageMode::Semantic,
                    message_source: "user_interaction".to_string(),
                    memory_sync_status: MemorySyncStatus::Pending,
                    created_at: message_created_at,
                },
                origin_device_id: request.origin_device_id.clone(),
                now: request.now,
            },
        )
        .await?;
        persist_message_attachments(
            repositories,
            &request.scope,
            &run.run.run_id,
            &run.run.owner_entity_id,
            run.run.project_id.as_deref(),
            &message_record_id,
            &request.answer.attachments,
            &request.origin_device_id,
            request.now,
        )
        .await?;
        let resume_event = put_run_event(
            repositories,
            &request.scope,
            &run,
            LocalAgentEventType::ResumeRequested,
            "resume_requested",
            &request.origin_device_id,
            &request.causation_id,
            request.now,
            json!({
                "interaction_id": request.interaction_id,
                "answer_record_id": recorded.message.message.record_id,
            }),
        )
        .await?;
        self.result = Some(AnsweredRunInteraction {
            message_record_id,
            resume_event,
        });
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn put_run_event(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run: &AgentRunStateRecord,
    event_type: LocalAgentEventType,
    event_name: &str,
    origin_device_id: &str,
    causation_id: &str,
    now: DateTime<Utc>,
    bounded_payload: Value,
) -> StorageResult<AgentEventStateRecord> {
    let event_id = format!("{}:{}:{event_name}:0", run.run.run_id, run.run.version);
    let query = RecordQuery {
        scope: scope.clone(),
        id: event_id.clone(),
    };
    if let Some(existing) = repositories.agent_events().get(&query).await? {
        if existing.event.run_id == run.run.run_id
            && existing.event.expected_version == run.run.version
            && existing.event.event_type == event_type
            && existing.event.bounded_payload == bounded_payload
        {
            return Ok(existing);
        }
        return Err(StorageError::Conflict {
            actual_revision: existing.metadata.revision,
        });
    }
    repositories
        .agent_events()
        .put(PutRecord {
            record: AgentEventStateRecord {
                metadata: RecordMetadata {
                    id: event_id.clone(),
                    scope: scope.clone(),
                    origin_device_id: origin_device_id.to_string(),
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                event: LocalAgentEvent {
                    event_id,
                    run_id: run.run.run_id.clone(),
                    event_type,
                    expected_version: run.run.version,
                    available_at: now,
                    status: LocalAgentEventStatus::Pending,
                    attempt_count: 0,
                    claimed_by_device_id: None,
                    claim_token: None,
                    claim_until: None,
                    causation_id: causation_id.to_string(),
                    correlation_id: run.run.owner_entity_id.clone(),
                    bounded_payload,
                    last_error: None,
                },
            },
            expected_revision: None,
        })
        .await
}

fn require_matching_interaction(
    run: &AgentRunStateRecord,
    interaction_id: &str,
    answer: &UserInteractionAnswer,
) -> StorageResult<()> {
    if run.run.status != LocalAgentRunStatus::Paused {
        return Err(StorageError::InvalidData {
            reason: "only a paused Run can accept a user interaction answer".to_string(),
        });
    }
    let pending = run
        .run
        .pending_interaction
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| StorageError::InvalidData {
            reason: "paused Run has no pending interaction".to_string(),
        })?;
    if pending.get("type").and_then(Value::as_str) != Some("ask_user")
        || pending.get("interaction_id").and_then(Value::as_str) != Some(interaction_id)
    {
        return Err(StorageError::InvalidData {
            reason: "interaction ID does not match the pending Ask User request".to_string(),
        });
    }
    let question: UserInteractionQuestion =
        serde_json::from_value(pending.get("question").cloned().ok_or_else(|| {
            StorageError::InvalidData {
                reason: "pending Ask User request has no visual question".to_string(),
            }
        })?)
        .map_err(|error| StorageError::InvalidData {
            reason: format!("pending Ask User request is invalid: {error}"),
        })?;
    question
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("pending Ask User request is invalid: {error}"),
        })?;
    let mut selected = std::collections::HashSet::new();
    for option_id in &answer.selected_option_ids {
        if !selected.insert(option_id) {
            return Err(StorageError::InvalidData {
                reason: "user interaction answer contains duplicate option IDs".to_string(),
            });
        }
        if !question
            .options
            .iter()
            .any(|option| option.option_id == *option_id)
        {
            return Err(StorageError::InvalidData {
                reason: format!("selected option {option_id} is not part of the pending question"),
            });
        }
    }
    Ok(())
}

fn validate_answer_request(request: &AnswerRunInteraction) -> StorageResult<()> {
    if request.scope.owner_user_id.trim().is_empty()
        || request.run_id.trim().is_empty()
        || request.interaction_id.trim().is_empty()
        || request.origin_device_id.trim().is_empty()
        || request.causation_id.trim().is_empty()
    {
        return Err(StorageError::InvalidData {
            reason: "user interaction answer identifiers must not be empty".to_string(),
        });
    }
    request
        .answer
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("invalid user interaction answer: {error}"),
        })
}

fn validate_request(request: &RequestRunControl) -> StorageResult<()> {
    if request.scope.owner_user_id.trim().is_empty()
        || request.run_id.trim().is_empty()
        || request.expected_version == 0
        || request.origin_device_id.trim().is_empty()
        || request.causation_id.trim().is_empty()
    {
        return Err(StorageError::InvalidData {
            reason: "Run control identifiers must not be empty".to_string(),
        });
    }
    Ok(())
}

fn validate_transition(status: LocalAgentRunStatus, action: RunControlAction) -> StorageResult<()> {
    let valid = match action {
        RunControlAction::Pause => !status.is_terminal() && status != LocalAgentRunStatus::Paused,
        RunControlAction::Resume => matches!(
            status,
            LocalAgentRunStatus::Paused | LocalAgentRunStatus::NeedsReview
        ),
        RunControlAction::Cancel => !status.is_terminal(),
    };
    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidData {
            reason: format!("cannot request {action:?} while Run status is {status:?}"),
        })
    }
}
