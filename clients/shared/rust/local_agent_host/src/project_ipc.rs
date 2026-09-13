// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ProjectRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    CreateProjectCommand, LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse,
    LocalProjectDraft, LocalProjectSnapshot, LocalProjectStatus, UpdateProjectCommand,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

const PROJECT_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredProjectState {
    schema_version: u32,
    description: String,
    workspace_id: String,
    relative_root: String,
    status: LocalProjectStatus,
}

pub struct LocalProjectIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalProjectIpcExecutor {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        device_id: impl Into<String>,
        next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
    ) -> Self {
        Self {
            storage,
            scope,
            device_id: device_id.into(),
            next,
        }
    }

    async fn create(
        &self,
        command: CreateProjectCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let now = Utc::now();
        let mut operation = PutProject {
            record: Some(project_record(
                &self.scope,
                &self.device_id,
                command.project_id,
                command.draft,
                LocalProjectStatus::Active,
                now,
            )?),
            expected_revision: None,
            result: None,
            reject_removed: false,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(project_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::Project)
            .ok_or_else(|| project_internal_error("project creation returned no record"))
    }

    async fn update(
        &self,
        command: UpdateProjectCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let now = Utc::now();
        let mut operation = PutProject {
            record: Some(project_record(
                &self.scope,
                &self.device_id,
                command.project_id,
                command.draft,
                command.status,
                now,
            )?),
            expected_revision: Some(command.expected_revision),
            result: None,
            reject_removed: true,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(project_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::Project)
            .ok_or_else(|| project_internal_error("project update returned no record"))
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalProjectIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::CreateProject(command) => self.create(command).await,
            LocalAgentCommand::UpdateProject(command) => self.update(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct PutProject {
    record: Option<ProjectRecord>,
    expected_revision: Option<u64>,
    result: Option<LocalProjectSnapshot>,
    reject_removed: bool,
}

#[async_trait]
impl StorageTransaction for PutProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let record = self.record.take().ok_or(StorageError::Transaction {
            reason: "project mutation was already consumed".to_string(),
        })?;
        if self.reject_removed {
            let current = repositories
                .projects()
                .get(&RecordQuery {
                    scope: record.metadata.scope.clone(),
                    id: record.metadata.id.clone(),
                })
                .await?
                .ok_or(StorageError::NotFound)?;
            if project_snapshot(current)?.status == LocalProjectStatus::Removed {
                return Err(StorageError::InvalidData {
                    reason: "removed projects cannot be updated".to_string(),
                });
            }
        }
        let stored = repositories
            .projects()
            .put(PutRecord {
                record,
                expected_revision: self.expected_revision,
            })
            .await?;
        self.result = Some(project_snapshot(stored)?);
        Ok(())
    }
}

fn project_record(
    scope: &RecordScope,
    device_id: &str,
    project_id: String,
    draft: LocalProjectDraft,
    status: LocalProjectStatus,
    now: chrono::DateTime<Utc>,
) -> Result<ProjectRecord, LocalAgentIpcError> {
    draft
        .validate()
        .map_err(|error| project_invalid_error(error.to_string()))?;
    let state = StoredProjectState {
        schema_version: PROJECT_STATE_SCHEMA_VERSION,
        description: draft.description,
        workspace_id: draft.workspace_id.clone(),
        relative_root: draft.relative_root,
        status,
    };
    Ok(ProjectRecord {
        metadata: RecordMetadata {
            id: project_id,
            scope: scope.clone(),
            origin_device_id: device_id.to_string(),
            revision: 0,
            created_at: now,
            updated_at: now,
        },
        name: draft.name,
        // The workspace grant ID is the only working-directory authority stored here.
        // Absolute paths remain inside the native connector filesystem boundary.
        root_reference: Some(draft.workspace_id),
        state: serde_json::to_value(state)
            .map_err(|error| project_internal_error(error.to_string()))?,
    })
}

pub(crate) fn project_snapshot(record: ProjectRecord) -> StorageResult<LocalProjectSnapshot> {
    let state: StoredProjectState =
        serde_json::from_value(record.state).map_err(|error| StorageError::InvalidData {
            reason: format!("stored project state is invalid: {error}"),
        })?;
    if state.schema_version != PROJECT_STATE_SCHEMA_VERSION
        || record.root_reference.as_deref() != Some(state.workspace_id.as_str())
    {
        return Err(StorageError::InvalidData {
            reason: "stored project authority does not match its workspace binding".to_string(),
        });
    }
    let snapshot = LocalProjectSnapshot {
        project_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalProjectDraft {
            name: record.name,
            description: state.description,
            workspace_id: state.workspace_id,
            relative_root: state.relative_root,
        },
        revision: record.metadata.revision,
        status: state.status,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored project projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn project_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match error {
        StorageError::Conflict { .. } => ("project_revision_conflict", false),
        StorageError::NotFound => ("project_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("project_invalid", false),
        _ => ("project_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn project_invalid_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "project_invalid".to_string(),
        message: message.into(),
        retryable: false,
    }
}

fn project_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "project_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
