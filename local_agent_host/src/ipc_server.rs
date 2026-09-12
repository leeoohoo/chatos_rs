// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentUiEventCursorQuery, ClientSettingRecord, ClientStorage, ListQuery, PutRecord,
    RecordMetadata, RecordQuery, RecordScope, StorageError, StorageResult, StorageTransaction,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessageRole, LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcReply,
    LocalAgentIpcRequest, LocalAgentIpcResponse, MainChatRunBinding, LOCAL_AGENT_PROTOCOL_VERSION,
};
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
                    thread_id: record.message.thread_id,
                    turn_id: record.message.turn_id,
                    message_id: record.message.record_id,
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
