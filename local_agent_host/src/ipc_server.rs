// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentUiEventCursorQuery, ClientStorage, ListQuery, RecordQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcReply, LocalAgentIpcRequest,
    LocalAgentIpcResponse, LOCAL_AGENT_PROTOCOL_VERSION,
};

pub const DEFAULT_MAXIMUM_IPC_FRAME_BYTES: usize = 8 * 1024 * 1024;

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
