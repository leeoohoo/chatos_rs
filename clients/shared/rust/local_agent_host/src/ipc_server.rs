// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentMessageStateRecord, AgentUiEventCursorQuery, ClientSettingRecord,
    ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, TaskRecord, ToolExecutionStateRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessageRole, GetRunDetailCommand, GetTaskGraphCommand, GetTaskRunDetailCommand,
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcReply, LocalAgentIpcRequest,
    LocalAgentIpcResponse, LocalAgentRun, LocalAgentRunDetail, LocalAgentRunTimelineEvent,
    LocalAgentTaskGraphNode, LocalAgentTaskGraphSnapshot, LocalAgentTaskProjection,
    LocalAgentTaskRunDetail, LocalAgentTaskRunSummary, LocalAgentTaskSnapshot, MainChatRunBinding,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::DurableTaskState;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

pub const DEFAULT_MAXIMUM_IPC_FRAME_BYTES: usize = 8 * 1024 * 1024;
const NATIVE_UI_CURSOR_SETTING_ID: &str = "local-agent-native-ui-cursor";

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentIpcServerError {
    #[error("IPC server maximum frame size must be positive")]
    InvalidMaximumFrameSize,
    #[error("IPC request frame contains {actual} bytes; maximum is {maximum}")]
    RequestFrameTooLarge { actual: usize, maximum: usize },
    #[error("IPC request frame is not valid JSON: {0}")]
    InvalidJson(serde_json::Error),
    #[error("IPC reply could not be encoded: {0}")]
    ReplyEncoding(serde_json::Error),
}

/// Executes commands that intentionally mutate Host or platform state. Query
/// commands are always resolved by `LocalAgentIpcServer` against its isolated
/// owner-scoped storage transaction.
#[async_trait]
pub trait LocalAgentIpcMutationExecutor: Send + Sync {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError>;
}

pub struct LocalAgentIpcServer {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    mutation_executor: Arc<dyn LocalAgentIpcMutationExecutor>,
    maximum_frame_bytes: usize,
}

impl LocalAgentIpcServer {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        mutation_executor: Arc<dyn LocalAgentIpcMutationExecutor>,
    ) -> Result<Self, LocalAgentIpcServerError> {
        Self::with_maximum_frame_bytes(
            storage,
            scope,
            mutation_executor,
            DEFAULT_MAXIMUM_IPC_FRAME_BYTES,
        )
    }

    pub fn with_maximum_frame_bytes(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        mutation_executor: Arc<dyn LocalAgentIpcMutationExecutor>,
        maximum_frame_bytes: usize,
    ) -> Result<Self, LocalAgentIpcServerError> {
        if maximum_frame_bytes == 0 {
            return Err(LocalAgentIpcServerError::InvalidMaximumFrameSize);
        }
        Ok(Self {
            storage,
            scope,
            mutation_executor,
            maximum_frame_bytes,
        })
    }

    /// Handles exactly one transport frame. Native Unix-domain-socket and
    /// named-pipe adapters share this boundary and therefore cannot bypass
    /// frame limits, protocol validation, owner isolation, or correlation.
    pub async fn handle_frame(&self, frame: &[u8]) -> Result<Vec<u8>, LocalAgentIpcServerError> {
        if frame.len() > self.maximum_frame_bytes {
            return Err(LocalAgentIpcServerError::RequestFrameTooLarge {
                actual: frame.len(),
                maximum: self.maximum_frame_bytes,
            });
        }
        let request: LocalAgentIpcRequest =
            serde_json::from_slice(frame).map_err(LocalAgentIpcServerError::InvalidJson)?;
        let reply = self.handle_request(request).await;
        let encoded =
            serde_json::to_vec(&reply).map_err(LocalAgentIpcServerError::ReplyEncoding)?;
        if encoded.len() <= self.maximum_frame_bytes {
            return Ok(encoded);
        }
        serde_json::to_vec(&LocalAgentIpcReply {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            request_id: reply.request_id,
            response: LocalAgentIpcResponse::Error(LocalAgentIpcError {
                code: "ipc_response_too_large".to_string(),
                message: "IPC response exceeds the configured frame limit; request a smaller page"
                    .to_string(),
                retryable: true,
            }),
        })
        .map_err(LocalAgentIpcServerError::ReplyEncoding)
    }

    pub async fn handle_request(&self, request: LocalAgentIpcRequest) -> LocalAgentIpcReply {
        let request_id = valid_reply_request_id(&request.request_id);
        let response = match request.validate() {
            Err(error) => LocalAgentIpcResponse::Error(LocalAgentIpcError {
                code: "invalid_ipc_request".to_string(),
                message: error.to_string(),
                retryable: false,
            }),
            Ok(()) if request.owner_user_id != self.scope.owner_user_id => {
                LocalAgentIpcResponse::Error(LocalAgentIpcError {
                    code: "owner_scope_mismatch".to_string(),
                    message: "IPC request owner does not match the authenticated Host scope"
                        .to_string(),
                    retryable: false,
                })
            }
            Ok(()) => self.execute_command(&request_id, request.command).await,
        };
        LocalAgentIpcReply {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            request_id,
            response,
        }
    }

    async fn execute_command(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> LocalAgentIpcResponse {
        let query_result = match command {
            LocalAgentCommand::GetRun { run_id } => {
                let mut operation = GetRunOperation {
                    scope: self.scope.clone(),
                    run_id,
                    response: None,
                };
                self.storage.transaction(&mut operation).await.map(|()| {
                    operation
                        .response
                        .unwrap_or(LocalAgentIpcResponse::Error(LocalAgentIpcError {
                            code: "run_not_found".to_string(),
                            message: "Local Agent Run was not found".to_string(),
                            retryable: false,
                        }))
                })
            }
            LocalAgentCommand::GetRunDetail(command) => {
                let mut operation = GetRunDetailOperation {
                    scope: self.scope.clone(),
                    command,
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| operation.response.ok_or(StorageError::NotFound))
            }
            LocalAgentCommand::ListRuns { cursor, limit } => {
                let mut operation = ListRunsOperation {
                    query: ListQuery {
                        scope: self.scope.clone(),
                        cursor,
                        limit,
                    },
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| {
                        operation.response.ok_or(StorageError::Transaction {
                            reason: "Run list transaction returned no page".to_string(),
                        })
                    })
            }
            LocalAgentCommand::GetTask { task_id } => {
                let mut operation = GetTaskOperation {
                    scope: self.scope.clone(),
                    task_id,
                    response: None,
                };
                self.storage.transaction(&mut operation).await.map(|()| {
                    operation
                        .response
                        .unwrap_or(LocalAgentIpcResponse::Error(LocalAgentIpcError {
                            code: "task_not_found".to_string(),
                            message: "Local Agent Task was not found".to_string(),
                            retryable: false,
                        }))
                })
            }
            LocalAgentCommand::ListTasks { cursor, limit } => {
                let mut operation = ListTasksOperation {
                    query: ListQuery {
                        scope: self.scope.clone(),
                        cursor,
                        limit,
                    },
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| {
                        operation.response.ok_or(StorageError::Transaction {
                            reason: "Task list transaction returned no page".to_string(),
                        })
                    })
            }
            LocalAgentCommand::GetTaskGraph(command) => {
                let mut operation = GetTaskGraphOperation {
                    scope: self.scope.clone(),
                    command,
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| operation.response.ok_or(StorageError::NotFound))
            }
            LocalAgentCommand::GetTaskRunDetail(command) => {
                let mut operation = GetTaskRunDetailOperation {
                    scope: self.scope.clone(),
                    command,
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| operation.response.ok_or(StorageError::NotFound))
            }
            LocalAgentCommand::GetMainChatRunBinding { run_id } => {
                let mut operation = GetMainChatRunBindingOperation {
                    scope: self.scope.clone(),
                    run_id,
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| operation.response.ok_or(StorageError::NotFound))
            }
            LocalAgentCommand::SubscribeRunEvents { after_seq, limit } => {
                let mut operation = ListUiEventsOperation {
                    query: AgentUiEventCursorQuery {
                        scope: self.scope.clone(),
                        after_seq,
                        limit,
                    },
                    response: None,
                };
                self.storage
                    .transaction(&mut operation)
                    .await
                    .and_then(|()| {
                        operation.response.ok_or(StorageError::Transaction {
                            reason: "UI event transaction returned no page".to_string(),
                        })
                    })
            }
            LocalAgentCommand::GetUiEventCursor => {
                let mut operation = NativeUiCursorOperation::read(self.scope.clone());
                self.storage.transaction(&mut operation).await.map(|()| {
                    LocalAgentIpcResponse::UiEventCursor {
                        event_seq: operation.result,
                    }
                })
            }
            LocalAgentCommand::AcknowledgeUiEvents { through_seq } => {
                let mut operation =
                    NativeUiCursorOperation::acknowledge(self.scope.clone(), through_seq);
                self.storage.transaction(&mut operation).await.map(|()| {
                    LocalAgentIpcResponse::UiEventCursor {
                        event_seq: operation.result,
                    }
                })
            }
            mutation => {
                return self
                    .mutation_executor
                    .execute_mutation(request_id, mutation)
                    .await
                    .unwrap_or_else(LocalAgentIpcResponse::Error);
            }
        };
        query_result.unwrap_or_else(storage_error_response)
    }
}

fn valid_reply_request_id(request_id: &str) -> String {
    if request_id.trim().is_empty() || request_id.len() > 512 {
        "invalid-request".to_string()
    } else {
        request_id.to_string()
    }
}

struct GetRunOperation {
    scope: RecordScope,
    run_id: String,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for GetRunOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.response = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.run_id.clone(),
            })
            .await?
            .map(|record| LocalAgentIpcResponse::Run(Box::new(record.run)));
        Ok(())
    }
}

struct ListRunsOperation {
    query: ListQuery,
    response: Option<LocalAgentIpcResponse>,
}

struct GetRunDetailOperation {
    scope: RecordScope,
    command: GetRunDetailCommand,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for GetRunDetailOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.command.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?
            .run;
        let (events, total, has_more) = paged_run_timeline(
            repositories,
            &self.scope,
            &run.run_id,
            self.command.event_offset,
            self.command.event_limit,
        )
        .await?;
        let detail = LocalAgentRunDetail {
            tools: run_tools(repositories, &self.scope, &run.run_id).await?,
            snapshot_event_sequence: latest_ui_event_sequence(repositories, &self.scope).await?,
            run,
            events,
            events_total: total,
            events_has_more: has_more,
        };
        detail.validate().map_err(protocol_projection_error)?;
        self.response = Some(LocalAgentIpcResponse::RunDetail(Box::new(detail)));
        Ok(())
    }
}

struct GetTaskOperation {
    scope: RecordScope,
    task_id: String,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for GetTaskOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.response = repositories
            .tasks()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.task_id.clone(),
            })
            .await?
            .map(task_snapshot)
            .transpose()?
            .map(|task| LocalAgentIpcResponse::Task(Box::new(task)));
        Ok(())
    }
}

struct ListTasksOperation {
    query: ListQuery,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for ListTasksOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let page = repositories.tasks().list(&self.query).await?;
        let tasks = page
            .records
            .into_iter()
            .map(task_snapshot)
            .collect::<StorageResult<Vec<_>>>()?;
        self.response = Some(LocalAgentIpcResponse::Tasks {
            tasks,
            next_cursor: page.next_cursor,
        });
        Ok(())
    }
}

fn task_snapshot(record: TaskRecord) -> StorageResult<LocalAgentTaskSnapshot> {
    let task_id = record.metadata.id.clone();
    let state = DurableTaskState::from_record(&record)?;
    let task = LocalAgentTaskSnapshot {
        task_id,
        revision: record.metadata.revision,
        source_thread_id: state.source_thread_id,
        source_turn_id: state.source_turn_id,
        project_id: state.project_id,
        initial_run_id: state.initial_run_id,
        current_run_id: state.current_run_id,
        run_ids: state.run_ids,
        objective: state.objective,
        acceptance_criteria: state.acceptance_criteria,
        status: record.status,
        model_config_id: state.model_config_id,
        model_config_revision: state.model_config_revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    task.validate().map_err(|error| StorageError::InvalidData {
        reason: format!("Task snapshot is invalid: {error}"),
    })?;
    Ok(task)
}

struct GetTaskGraphOperation {
    scope: RecordScope,
    command: GetTaskGraphCommand,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for GetTaskGraphOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut records = Vec::new();
        let mut cursor = None;
        loop {
            let page = repositories
                .tasks()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            for record in page.records {
                let state = DurableTaskState::from_record(&record)?;
                if state.source_thread_id == self.command.source_thread_id
                    && state.source_turn_id == self.command.source_turn_id
                {
                    records.push(record);
                }
            }
            let Some(next) = page.next_cursor else { break };
            ensure_cursor_advanced(cursor.as_deref(), &next, "Task Graph")?;
            cursor = Some(next);
        }

        let mut nodes = Vec::with_capacity(records.len());
        for record in records {
            let task = task_snapshot(record)?;
            let current_run = repositories
                .agent_runs()
                .get(&RecordQuery {
                    scope: self.scope.clone(),
                    id: task.current_run_id.clone(),
                })
                .await?
                .ok_or_else(|| StorageError::InvalidData {
                    reason: format!(
                        "Task {} current Run {} does not exist",
                        task.task_id, task.current_run_id
                    ),
                })?
                .run;
            nodes.push(LocalAgentTaskGraphNode {
                task: LocalAgentTaskProjection {
                    task,
                    current_run: task_run_summary(current_run),
                },
                depth: 0,
                is_root: true,
            });
        }
        nodes.sort_by(|left, right| {
            left.task
                .task
                .created_at
                .cmp(&right.task.task.created_at)
                .then_with(|| left.task.task.task_id.cmp(&right.task.task.task_id))
        });
        let graph = LocalAgentTaskGraphSnapshot {
            source_thread_id: self.command.source_thread_id.clone(),
            source_turn_id: self.command.source_turn_id.clone(),
            root_task_ids: nodes
                .iter()
                .map(|node| node.task.task.task_id.clone())
                .collect(),
            nodes,
            edges: Vec::new(),
        };
        graph.validate().map_err(protocol_projection_error)?;
        self.response = Some(LocalAgentIpcResponse::TaskGraph(Box::new(graph)));
        Ok(())
    }
}

struct GetTaskRunDetailOperation {
    scope: RecordScope,
    command: GetTaskRunDetailCommand,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for GetTaskRunDetailOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let task_record = repositories
            .tasks()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.command.task_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let task = task_snapshot(task_record)?;
        if !task.run_ids.contains(&self.command.run_id) {
            return Err(StorageError::NotFound);
        }
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.command.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?
            .run;
        if run.owner_entity_type != "task"
            || run.owner_entity_id != task.task_id
            || run.owner_user_id != self.scope.owner_user_id
        {
            return Err(StorageError::NotFound);
        }

        let (events, total, has_more) = paged_run_timeline(
            repositories,
            &self.scope,
            &run.run_id,
            self.command.event_offset,
            self.command.event_limit,
        )
        .await?;
        let detail = LocalAgentTaskRunDetail {
            task,
            run: task_run_summary(run),
            events,
            events_total: total,
            events_has_more: has_more,
        };
        detail.validate().map_err(protocol_projection_error)?;
        self.response = Some(LocalAgentIpcResponse::TaskRunDetail(Box::new(detail)));
        Ok(())
    }
}

async fn paged_run_timeline(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_id: &str,
    event_offset: u32,
    event_limit: u32,
) -> StorageResult<(Vec<LocalAgentRunTimelineEvent>, u32, bool)> {
    let mut events = run_timeline_events(repositories, scope, run_id).await?;
    events.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    let total = u32::try_from(events.len()).map_err(|_| StorageError::InvalidData {
        reason: "Run timeline event count exceeds the protocol limit".to_string(),
    })?;
    let offset = event_offset as usize;
    let limit = event_limit as usize;
    let page = events.into_iter().skip(offset).take(limit).collect();
    Ok((page, total, offset.saturating_add(limit) < total as usize))
}

async fn run_timeline_events(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_id: &str,
) -> StorageResult<Vec<LocalAgentRunTimelineEvent>> {
    let mut projected = Vec::new();

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
        projected.extend(
            page.records
                .into_iter()
                .filter(|record| record.event.run_id == run_id)
                .map(project_agent_event),
        );
        let Some(next) = page.next_cursor else { break };
        ensure_cursor_advanced(cursor.as_deref(), &next, "Agent event")?;
        cursor = Some(next);
    }

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
        projected.extend(
            page.records
                .into_iter()
                .filter(|record| record.message.run_id == run_id)
                .flat_map(project_agent_message),
        );
        let Some(next) = page.next_cursor else { break };
        ensure_cursor_advanced(cursor.as_deref(), &next, "Agent message")?;
        cursor = Some(next);
    }

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
        projected.extend(
            page.records
                .into_iter()
                .filter(|record| record.execution.run_id == run_id)
                .map(project_tool_execution),
        );
        let Some(next) = page.next_cursor else { break };
        ensure_cursor_advanced(cursor.as_deref(), &next, "Tool execution")?;
        cursor = Some(next);
    }
    Ok(projected)
}

async fn run_tools(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    run_id: &str,
) -> StorageResult<Vec<chatos_local_agent_protocol::ToolExecution>> {
    let mut values = Vec::new();
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
        values.extend(
            page.records
                .into_iter()
                .filter(|record| record.execution.run_id == run_id)
                .map(|record| record.execution),
        );
        let Some(next) = page.next_cursor else { break };
        ensure_cursor_advanced(cursor.as_deref(), &next, "Tool execution")?;
        cursor = Some(next);
    }
    values.sort_by(|left, right| left.invocation_id.cmp(&right.invocation_id));
    Ok(values)
}

async fn latest_ui_event_sequence(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
) -> StorageResult<u64> {
    // The durable native cursor already covers all older events. Starting at
    // that watermark keeps restart snapshots proportional to the unconsumed
    // tail instead of rescanning the account's full UI-event history once per
    // Run detail page.
    let mut sequence = read_native_ui_cursor(repositories, scope).await?;
    loop {
        let page = repositories
            .agent_ui_events()
            .list_after(&AgentUiEventCursorQuery {
                scope: scope.clone(),
                after_seq: sequence,
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        if page.next_seq < sequence || (page.has_more && page.next_seq == sequence) {
            return Err(StorageError::InvalidData {
                reason: "UI event snapshot pagination did not advance".to_string(),
            });
        }
        sequence = page.next_seq;
        if !page.has_more {
            return Ok(sequence);
        }
    }
}

fn project_agent_event(record: AgentEventStateRecord) -> LocalAgentRunTimelineEvent {
    let event_type = enum_wire_name(&record.event.event_type);
    let message = record
        .event
        .last_error
        .clone()
        .or_else(|| bounded_value_text(&record.event.bounded_payload));
    LocalAgentRunTimelineEvent {
        event_id: format!("run_event:{}", record.event.event_id),
        event_type,
        message,
        created_at: record.metadata.created_at,
    }
}

fn project_agent_message(record: AgentMessageStateRecord) -> Vec<LocalAgentRunTimelineEvent> {
    let role = enum_wire_name(&record.message.role);
    let mut events = Vec::new();
    if let Some(content) = record.message.content.filter(|value| !value.is_empty()) {
        events.push(LocalAgentRunTimelineEvent {
            event_id: format!("message:{}:content", record.message.record_id),
            event_type: format!("message_{role}_content"),
            message: Some(content),
            created_at: record.message.created_at,
        });
    }
    if let Some(reasoning) = record.message.reasoning.filter(|value| !value.is_empty()) {
        events.push(LocalAgentRunTimelineEvent {
            event_id: format!("message:{}:reasoning", record.message.record_id),
            event_type: format!("message_{role}_reasoning"),
            message: Some(reasoning),
            created_at: record.message.created_at,
        });
    }
    if events.is_empty() {
        if let Some(message) = record
            .message
            .structured_payload
            .as_ref()
            .and_then(bounded_value_text)
        {
            events.push(LocalAgentRunTimelineEvent {
                event_id: format!("message:{}:structured", record.message.record_id),
                event_type: format!("message_{role}_structured"),
                message: Some(message),
                created_at: record.message.created_at,
            });
        }
    }
    events
}

fn project_tool_execution(record: ToolExecutionStateRecord) -> LocalAgentRunTimelineEvent {
    let status = enum_wire_name(&record.execution.status);
    let result = record
        .execution
        .bounded_result
        .as_ref()
        .and_then(bounded_value_text);
    let message = Some(match result {
        Some(result) => format!("{}: {result}", record.execution.tool_name),
        None => record.execution.tool_name.clone(),
    });
    LocalAgentRunTimelineEvent {
        event_id: format!("tool:{}", record.execution.invocation_id),
        event_type: format!("tool_{status}"),
        message,
        created_at: record.metadata.created_at,
    }
}

fn task_run_summary(run: LocalAgentRun) -> LocalAgentTaskRunSummary {
    let outcome = run.terminal_outcome.as_ref();
    let result_summary =
        outcome.and_then(|value| text_field(value, &["summary", "result_summary", "message"]));
    let report_content = outcome.and_then(|value| {
        text_field(value, &["report_content", "report", "result"])
            .or_else(|| serde_json::to_string_pretty(value).ok())
    });
    let error_message = outcome.and_then(|value| {
        text_field(
            value,
            &["error_message", "failure_reason", "reason", "error"],
        )
    });
    LocalAgentTaskRunSummary {
        run,
        result_summary,
        report_content,
        error_message,
    }
}

fn text_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn bounded_value_text(value: &serde_json::Value) -> Option<String> {
    if value.is_null() {
        None
    } else if let Some(value) = value.as_str() {
        (!value.trim().is_empty()).then(|| value.to_string())
    } else {
        serde_json::to_string(value).ok()
    }
}

fn enum_wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn ensure_cursor_advanced(previous: Option<&str>, next: &str, domain: &str) -> StorageResult<()> {
    if previous == Some(next) {
        Err(StorageError::InvalidData {
            reason: format!("{domain} pagination cursor did not advance"),
        })
    } else {
        Ok(())
    }
}

fn protocol_projection_error(error: chatos_local_agent_protocol::ProtocolError) -> StorageError {
    StorageError::InvalidData {
        reason: format!("Local Agent projection is invalid: {error}"),
    }
}

#[async_trait]
impl StorageTransaction for ListRunsOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let page = repositories.agent_runs().list(&self.query).await?;
        self.response = Some(LocalAgentIpcResponse::Runs {
            runs: page.records.into_iter().map(|record| record.run).collect(),
            next_cursor: page.next_cursor,
        });
        Ok(())
    }
}

struct ListUiEventsOperation {
    query: AgentUiEventCursorQuery,
    response: Option<LocalAgentIpcResponse>,
}

struct GetMainChatRunBindingOperation {
    scope: RecordScope,
    run_id: String,
    response: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for GetMainChatRunBindingOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        if run.run.profile_key != "main_chat"
            || run.run.owner_entity_type != "conversation"
            || run.run.owner_user_id != self.scope.owner_user_id
        {
            return Err(StorageError::InvalidData {
                reason: "requested Run is not an owner-scoped Main Chat Run".to_string(),
            });
        }

        let mut cursor = None;
        let mut binding = None;
        loop {
            let page = repositories
                .agent_messages()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            for record in page.records.into_iter().filter(|record| {
                record.message.run_id == self.run_id
                    && record.message.role == AgentMessageRole::User
                    && record.message.message_source == "main_chat"
            }) {
                if binding.is_some() {
                    return Err(StorageError::InvalidData {
                        reason: "Main Chat Run has multiple initial user bindings".to_string(),
                    });
                }
                if record.message.thread_id != run.run.owner_entity_id {
                    return Err(StorageError::InvalidData {
                        reason: "Main Chat binding thread does not match the Run".to_string(),
                    });
                }
                binding = Some(MainChatRunBinding {
                    run_id: self.run_id.clone(),
                    thread_id: record.message.thread_id.clone(),
                    turn_id: record.message.turn_id.clone(),
                    message_id: record.message.record_id.clone(),
                    user_message: Box::new(record.message),
                });
            }
            match page.next_cursor {
                None => break,
                Some(next) if cursor.as_deref() == Some(next.as_str()) => {
                    return Err(StorageError::InvalidData {
                        reason: "Main Chat binding pagination cursor did not advance".to_string(),
                    });
                }
                Some(next) => cursor = Some(next),
            }
        }
        let binding = binding.ok_or(StorageError::NotFound)?;
        binding
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("Main Chat Run binding is invalid: {error}"),
            })?;
        self.response = Some(LocalAgentIpcResponse::MainChatRunBinding(binding));
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeUiCursorValue {
    event_seq: u64,
}

struct NativeUiCursorOperation {
    scope: RecordScope,
    acknowledge: Option<u64>,
    result: u64,
}

impl NativeUiCursorOperation {
    fn read(scope: RecordScope) -> Self {
        Self {
            scope,
            acknowledge: None,
            result: 0,
        }
    }

    fn acknowledge(scope: RecordScope, through_seq: u64) -> Self {
        Self {
            scope,
            acknowledge: Some(through_seq),
            result: 0,
        }
    }
}

#[async_trait]
impl StorageTransaction for NativeUiCursorOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let existing = repositories
            .settings()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: NATIVE_UI_CURSOR_SETTING_ID.to_string(),
            })
            .await?;
        let current = existing
            .as_ref()
            .map(native_ui_cursor)
            .transpose()?
            .unwrap_or(0);
        let Some(through_seq) = self.acknowledge else {
            self.result = current;
            return Ok(());
        };
        if through_seq <= current {
            self.result = current;
            return Ok(());
        }

        let page = repositories
            .agent_ui_events()
            .list_after(&AgentUiEventCursorQuery {
                scope: self.scope.clone(),
                after_seq: current,
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        if !page
            .records
            .iter()
            .any(|record| record.event.event_seq == through_seq)
        {
            return Err(StorageError::InvalidData {
                reason: "UI cursor cannot acknowledge an event that was not persisted".to_string(),
            });
        }

        let now = Utc::now();
        let expected_revision = existing.as_ref().map(|record| record.metadata.revision);
        let metadata = existing
            .as_ref()
            .map(|record| record.metadata.clone())
            .unwrap_or(RecordMetadata {
                id: NATIVE_UI_CURSOR_SETTING_ID.to_string(),
                scope: self.scope.clone(),
                origin_device_id: "native-ui".to_string(),
                revision: 0,
                created_at: now,
                updated_at: now,
            });
        repositories
            .settings()
            .put(PutRecord {
                record: ClientSettingRecord {
                    metadata,
                    key: NATIVE_UI_CURSOR_SETTING_ID.to_string(),
                    value: json!({ "event_seq": through_seq }),
                },
                expected_revision,
            })
            .await?;
        self.result = through_seq;
        Ok(())
    }
}

async fn read_native_ui_cursor(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
) -> StorageResult<u64> {
    repositories
        .settings()
        .get(&RecordQuery {
            scope: scope.clone(),
            id: NATIVE_UI_CURSOR_SETTING_ID.to_string(),
        })
        .await?
        .as_ref()
        .map(native_ui_cursor)
        .transpose()
        .map(|value| value.unwrap_or(0))
}

fn native_ui_cursor(record: &ClientSettingRecord) -> StorageResult<u64> {
    if record.metadata.id != NATIVE_UI_CURSOR_SETTING_ID
        || record.key != NATIVE_UI_CURSOR_SETTING_ID
    {
        return Err(StorageError::InvalidData {
            reason: "native UI cursor setting identity is invalid".to_string(),
        });
    }
    serde_json::from_value::<NativeUiCursorValue>(record.value.clone())
        .map(|value| value.event_seq)
        .map_err(|error| StorageError::InvalidData {
            reason: format!("native UI cursor setting is invalid: {error}"),
        })
}

#[async_trait]
impl StorageTransaction for ListUiEventsOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let page = repositories
            .agent_ui_events()
            .list_after(&self.query)
            .await?;
        self.response = Some(LocalAgentIpcResponse::Events {
            events: page
                .records
                .into_iter()
                .map(|record| record.event)
                .collect(),
            next_seq: page.next_seq,
            has_more: page.has_more,
        });
        Ok(())
    }
}

fn storage_error_response(error: StorageError) -> LocalAgentIpcResponse {
    let retryable = matches!(
        error,
        StorageError::Unavailable { .. } | StorageError::Transaction { .. }
    );
    LocalAgentIpcResponse::Error(LocalAgentIpcError {
        code: "client_storage_error".to_string(),
        message: error.to_string(),
        retryable,
    })
}
