// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_client_storage::{
    AgentRunStateRecord, AppendAgentUiEvent, ClientStorage, ListQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, ToolExecutionStateRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    LocalAgentUiEventPayload, MemorySyncStatus, MemorySyncUiStatus, ModelStreamUiEvent,
    UserInteractionQuestion, UserInteractionRequest,
};
use serde::Deserialize;

use crate::pagination::advance_cursor;

struct AppendModelStreamEvent {
    scope: RecordScope,
    origin_device_id: String,
    event: ModelStreamUiEvent,
}

#[async_trait::async_trait]
impl StorageTransaction for AppendModelStreamEvent {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .agent_ui_events()
            .append(AppendAgentUiEvent {
                scope: self.scope.clone(),
                origin_device_id: self.origin_device_id.clone(),
                payload: LocalAgentUiEventPayload::ModelStream(self.event.clone()),
            })
            .await?;
        Ok(())
    }
}

/// Persists one accepted gateway delta in the same authoritative client
/// storage used by the Run. These events are replayable UI state, not semantic
/// conversation messages and therefore never enter the Memory outbox.
pub async fn append_model_stream_event(
    storage: &dyn ClientStorage,
    scope: &RecordScope,
    origin_device_id: &str,
    event: ModelStreamUiEvent,
) -> StorageResult<()> {
    if event.run_id.trim().is_empty()
        || event.step_seq == 0
        || event.delta.is_empty()
        || origin_device_id.trim().is_empty()
    {
        return Err(StorageError::InvalidData {
            reason: "model stream UI event is invalid".to_string(),
        });
    }
    let mut operation = AppendModelStreamEvent {
        scope: scope.clone(),
        origin_device_id: origin_device_id.to_string(),
        event,
    };
    storage.transaction(&mut operation).await
}

pub(crate) async fn append_run_snapshot(
    repositories: &mut dyn TransactionRepositories,
    record: &AgentRunStateRecord,
) -> StorageResult<()> {
    repositories
        .agent_ui_events()
        .append(AppendAgentUiEvent {
            scope: record.metadata.scope.clone(),
            origin_device_id: record.metadata.origin_device_id.clone(),
            payload: LocalAgentUiEventPayload::RunSnapshot(Box::new(record.run.clone())),
        })
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingAskUserInteraction {
    #[serde(rename = "type")]
    interaction_type: String,
    interaction_id: String,
    question: UserInteractionQuestion,
}

pub(crate) async fn append_pending_user_interaction(
    repositories: &mut dyn TransactionRepositories,
    record: &AgentRunStateRecord,
) -> StorageResult<()> {
    let Some(pending) = record.run.pending_interaction.clone() else {
        return Ok(());
    };
    if pending.get("type").and_then(serde_json::Value::as_str) != Some("ask_user") {
        return Ok(());
    }
    let pending: PendingAskUserInteraction =
        serde_json::from_value(pending).map_err(|error| StorageError::InvalidData {
            reason: format!("pending Ask User interaction is invalid: {error}"),
        })?;
    if pending.interaction_type != "ask_user" || pending.question.prompt.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "pending Ask User interaction has an invalid type or empty prompt".to_string(),
        });
    }
    let request = UserInteractionRequest {
        interaction_id: pending.interaction_id,
        run_id: record.run.run_id.clone(),
        prompt: pending.question.prompt,
        options: pending.question.options,
        image_references: pending.question.image_references,
        details: pending.question.details,
    };
    request
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("pending Ask User interaction is invalid: {error}"),
        })?;
    repositories
        .agent_ui_events()
        .append(AppendAgentUiEvent {
            scope: record.metadata.scope.clone(),
            origin_device_id: record.metadata.origin_device_id.clone(),
            payload: LocalAgentUiEventPayload::UserInteraction(request),
        })
        .await?;
    Ok(())
}

pub(crate) async fn append_tool_snapshot(
    repositories: &mut dyn TransactionRepositories,
    record: &ToolExecutionStateRecord,
) -> StorageResult<()> {
    repositories
        .agent_ui_events()
        .append(AppendAgentUiEvent {
            scope: record.metadata.scope.clone(),
            origin_device_id: record.metadata.origin_device_id.clone(),
            payload: LocalAgentUiEventPayload::ToolSnapshot(Box::new(record.execution.clone())),
        })
        .await?;
    Ok(())
}

pub(crate) async fn append_memory_sync_statuses(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_origins: &BTreeMap<String, String>,
) -> StorageResult<()> {
    if run_origins.is_empty() {
        return Ok(());
    }
    if run_origins.iter().any(|(run_id, origin_device_id)| {
        run_id.trim().is_empty() || origin_device_id.trim().is_empty()
    }) {
        return Err(StorageError::InvalidData {
            reason: "Memory Sync UI event requires Run and device identities".to_string(),
        });
    }
    let mut counts = run_origins
        .keys()
        .cloned()
        .map(|run_id| (run_id, (0_u64, 0_u64)))
        .collect::<BTreeMap<_, _>>();
    let mut cursor = None;
    loop {
        let page = repositories
            .agent_messages()
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        for record in page.records {
            let Some((pending_count, failed_count)) = counts.get_mut(&record.message.run_id) else {
                continue;
            };
            match record.message.memory_sync_status {
                MemorySyncStatus::Pending => {
                    *pending_count =
                        pending_count
                            .checked_add(1)
                            .ok_or_else(|| StorageError::InvalidData {
                                reason: "Memory Sync pending count overflow".to_string(),
                            })?;
                }
                MemorySyncStatus::Failed => {
                    *failed_count =
                        failed_count
                            .checked_add(1)
                            .ok_or_else(|| StorageError::InvalidData {
                                reason: "Memory Sync failure count overflow".to_string(),
                            })?;
                }
                MemorySyncStatus::Synced => {}
            }
        }
        if !advance_cursor(&mut cursor, page.next_cursor)? {
            break;
        }
    }
    for (run_id, (pending_count, failed_count)) in counts {
        let origin_device_id = run_origins
            .get(&run_id)
            .expect("counts are initialized from run origins");
        repositories
            .agent_ui_events()
            .append(AppendAgentUiEvent {
                scope: scope.clone(),
                origin_device_id: origin_device_id.to_string(),
                payload: LocalAgentUiEventPayload::MemorySync(MemorySyncUiStatus {
                    run_id,
                    pending_count,
                    failed_count,
                    last_error_code: (failed_count > 0).then(|| "memory_sync_failed".to_string()),
                }),
            })
            .await?;
    }
    Ok(())
}
