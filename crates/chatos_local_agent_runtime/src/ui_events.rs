// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_client_storage::{
    AgentRunStateRecord, AppendAgentUiEvent, ListQuery, RecordScope, StorageError, StorageResult,
    ToolExecutionStateRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{LocalAgentUiEventPayload, MemorySyncUiStatus, SyncOutboxStatus};

use crate::pagination::advance_cursor;

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

pub(crate) async fn append_memory_sync_status(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    origin_device_id: &str,
) -> StorageResult<()> {
    let mut cursor = None;
    let mut pending_count = 0_u64;
    let mut failed_count = 0_u64;
    loop {
        let page = repositories
            .sync_outbox()
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        for record in page.records {
            match record.item.status {
                SyncOutboxStatus::Pending | SyncOutboxStatus::InFlight => {
                    pending_count =
                        pending_count
                            .checked_add(1)
                            .ok_or_else(|| StorageError::InvalidData {
                                reason: "Memory Sync pending count overflow".to_string(),
                            })?;
                }
                SyncOutboxStatus::Failed => {
                    failed_count =
                        failed_count
                            .checked_add(1)
                            .ok_or_else(|| StorageError::InvalidData {
                                reason: "Memory Sync failure count overflow".to_string(),
                            })?;
                }
                SyncOutboxStatus::Succeeded => {}
            }
        }
        if !advance_cursor(&mut cursor, page.next_cursor)? {
            break;
        }
    }
    repositories
        .agent_ui_events()
        .append(AppendAgentUiEvent {
            scope: scope.clone(),
            origin_device_id: origin_device_id.to_string(),
            payload: LocalAgentUiEventPayload::MemorySync(MemorySyncUiStatus {
                run_id: None,
                pending_count,
                failed_count,
                last_error_code: (failed_count > 0).then(|| "memory_sync_failed".to_string()),
            }),
        })
        .await?;
    Ok(())
}
