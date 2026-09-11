// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, ClientStorage, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRunStatus,
};
use chrono::{DateTime, Utc};

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
        validate_transition(run.run.status, request.action)?;

        let event_id = format!(
            "{}:{}:{}:0",
            run.run.run_id,
            run.run.version,
            request.action.event_name()
        );
        let query = RecordQuery {
            scope: request.scope.clone(),
            id: event_id.clone(),
        };
        if let Some(existing) = repositories.agent_events().get(&query).await? {
            if existing.event.run_id == run.run.run_id
                && existing.event.expected_version == run.run.version
                && existing.event.event_type == request.action.event_type()
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
                run_id: run.run.run_id,
                event_type: request.action.event_type(),
                expected_version: run.run.version,
                available_at: request.now,
                status: LocalAgentEventStatus::Pending,
                attempt_count: 0,
                claimed_by_device_id: None,
                claim_token: None,
                claim_until: None,
                causation_id: request.causation_id,
                correlation_id: run.run.owner_entity_id,
                bounded_payload: serde_json::Value::Null,
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

fn validate_request(request: &RequestRunControl) -> StorageResult<()> {
    if request.scope.owner_user_id.trim().is_empty()
        || request.run_id.trim().is_empty()
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
