// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::marker::PhantomData;

use async_trait::async_trait;
use chrono::Utc;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::canonical_json::{encode_canonical, verify_canonical, CanonicalRecord};
use crate::{
    AgentEventStateRecord, AgentEventStateRepository, AgentMessageStateRecord,
    AgentMessageStateRepository, AgentRecord, AgentRepository, AgentRunStateRecord,
    AgentRunStateRepository, ClientSettingRecord, ClientSettingsRepository, ClipboardRecord,
    ClipboardRepository, ConversationRecord, ConversationRepository, ListQuery, MediaStateRecord,
    MediaStateRepository, NotepadRecord, NotepadRepository, PluginStateRecord,
    PluginStateRepository, ProjectRecord, ProjectRepository, ProviderContextStateRecord,
    ProviderContextStateRepository, PutRecord, RecordMetadata, RecordPage, RecordQuery,
    StorageError, StorageResult, StoryRecord, StoryRepository, SyncOutboxStateRecord,
    SyncOutboxStateRepository, TaskRecord, TaskRepository, TerminalHistoryRecord,
    TerminalHistoryRepository, ToolExecutionStateRecord, ToolExecutionStateRepository,
    TransactionRepositories,
};

pub(crate) const SCHEMA_VERSION: u32 = 4;
pub(crate) const LEGACY_DOMAIN_TABLES: [&str; 11] = [
    "client_agents",
    "client_conversations",
    "client_tasks",
    "client_projects",
    "client_plugins",
    "client_media",
    "client_settings",
    "client_clipboard",
    "client_stories",
    "client_notepad",
    "client_terminal_history",
];
pub(crate) const RUNTIME_DOMAIN_TABLES: [&str; 2] = ["client_agent_runs", "client_agent_events"];
pub(crate) const AUXILIARY_RUNTIME_TABLES: [&str; 4] = [
    "client_agent_messages",
    "client_provider_context",
    "client_tool_executions",
    "client_sync_outbox",
];
pub(crate) const DOMAIN_TABLES: [&str; 17] = [
    "client_agents",
    "client_conversations",
    "client_tasks",
    "client_projects",
    "client_plugins",
    "client_media",
    "client_settings",
    "client_clipboard",
    "client_stories",
    "client_notepad",
    "client_terminal_history",
    "client_agent_runs",
    "client_agent_events",
    "client_agent_messages",
    "client_provider_context",
    "client_tool_executions",
    "client_sync_outbox",
];

pub(crate) struct StoredRow {
    pub id: String,
    pub payload: StoredPayload,
}

pub(crate) struct StoredPayload {
    pub record_json: String,
    pub record_digest: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[async_trait]
pub(crate) trait RecordStore: Send {
    async fn get_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<StoredPayload>>;

    async fn list_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> StorageResult<Vec<StoredRow>>;

    #[allow(clippy::too_many_arguments)]
    async fn insert_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
        revision: i64,
        created_at: &str,
        updated_at: &str,
        record_json: &str,
        record_digest: &str,
    ) -> StorageResult<bool>;

    #[allow(clippy::too_many_arguments)]
    async fn update_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
        expected_revision: i64,
        next_revision: i64,
        updated_at: &str,
        record_json: &str,
        record_digest: &str,
    ) -> StorageResult<bool>;

    async fn delete(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
        expected_revision: i64,
    ) -> StorageResult<bool>;

    async fn current_revision(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<i64>>;
}

pub(crate) struct RecordTransactionRepositories<'store> {
    store: &'store mut dyn RecordStore,
}

impl<'store> RecordTransactionRepositories<'store> {
    pub(crate) fn new(store: &'store mut dyn RecordStore) -> Self {
        Self { store }
    }
}

impl TransactionRepositories for RecordTransactionRepositories<'_> {
    fn agents(&mut self) -> Box<dyn AgentRepository + '_> {
        Box::new(JsonRecordRepository::<AgentRecord>::new(
            self.store,
            "client_agents",
        ))
    }

    fn agent_runs(&mut self) -> Box<dyn AgentRunStateRepository + '_> {
        Box::new(JsonRecordRepository::<AgentRunStateRecord>::new(
            self.store,
            "client_agent_runs",
        ))
    }

    fn agent_events(&mut self) -> Box<dyn AgentEventStateRepository + '_> {
        Box::new(JsonRecordRepository::<AgentEventStateRecord>::new(
            self.store,
            "client_agent_events",
        ))
    }

    fn agent_messages(&mut self) -> Box<dyn AgentMessageStateRepository + '_> {
        Box::new(JsonRecordRepository::<AgentMessageStateRecord>::new(
            self.store,
            "client_agent_messages",
        ))
    }

    fn provider_context(&mut self) -> Box<dyn ProviderContextStateRepository + '_> {
        Box::new(JsonRecordRepository::<ProviderContextStateRecord>::new(
            self.store,
            "client_provider_context",
        ))
    }

    fn tool_executions(&mut self) -> Box<dyn ToolExecutionStateRepository + '_> {
        Box::new(JsonRecordRepository::<ToolExecutionStateRecord>::new(
            self.store,
            "client_tool_executions",
        ))
    }

    fn sync_outbox(&mut self) -> Box<dyn SyncOutboxStateRepository + '_> {
        Box::new(JsonRecordRepository::<SyncOutboxStateRecord>::new(
            self.store,
            "client_sync_outbox",
        ))
    }

    fn conversations(&mut self) -> Box<dyn ConversationRepository + '_> {
        Box::new(JsonRecordRepository::<ConversationRecord>::new(
            self.store,
            "client_conversations",
        ))
    }

    fn tasks(&mut self) -> Box<dyn TaskRepository + '_> {
        Box::new(JsonRecordRepository::<TaskRecord>::new(
            self.store,
            "client_tasks",
        ))
    }

    fn projects(&mut self) -> Box<dyn ProjectRepository + '_> {
        Box::new(JsonRecordRepository::<ProjectRecord>::new(
            self.store,
            "client_projects",
        ))
    }

    fn plugins(&mut self) -> Box<dyn PluginStateRepository + '_> {
        Box::new(JsonRecordRepository::<PluginStateRecord>::new(
            self.store,
            "client_plugins",
        ))
    }

    fn media(&mut self) -> Box<dyn MediaStateRepository + '_> {
        Box::new(JsonRecordRepository::<MediaStateRecord>::new(
            self.store,
            "client_media",
        ))
    }

    fn settings(&mut self) -> Box<dyn ClientSettingsRepository + '_> {
        Box::new(JsonRecordRepository::<ClientSettingRecord>::new(
            self.store,
            "client_settings",
        ))
    }

    fn clipboard(&mut self) -> Box<dyn ClipboardRepository + '_> {
        Box::new(JsonRecordRepository::<ClipboardRecord>::new(
            self.store,
            "client_clipboard",
        ))
    }

    fn stories(&mut self) -> Box<dyn StoryRepository + '_> {
        Box::new(JsonRecordRepository::<StoryRecord>::new(
            self.store,
            "client_stories",
        ))
    }

    fn notepad(&mut self) -> Box<dyn NotepadRepository + '_> {
        Box::new(JsonRecordRepository::<NotepadRecord>::new(
            self.store,
            "client_notepad",
        ))
    }

    fn terminal_history(&mut self) -> Box<dyn TerminalHistoryRepository + '_> {
        Box::new(JsonRecordRepository::<TerminalHistoryRecord>::new(
            self.store,
            "client_terminal_history",
        ))
    }
}

pub(crate) trait RepositoryRecord: Serialize + DeserializeOwned + Send + Unpin {
    fn metadata(&self) -> &RecordMetadata;
    fn metadata_mut(&mut self) -> &mut RecordMetadata;
    fn validate(&self) -> StorageResult<()>;
    fn validate_persisted(&self) -> StorageResult<()> {
        self.validate()
    }
}

macro_rules! impl_repository_record {
    ($($record:ty),+ $(,)?) => {
        $(
            impl RepositoryRecord for $record {
                fn metadata(&self) -> &RecordMetadata { &self.metadata }
                fn metadata_mut(&mut self) -> &mut RecordMetadata { &mut self.metadata }
                fn validate(&self) -> StorageResult<()> { validate_record_identity(&self.metadata) }
            }
        )+
    };
}

impl_repository_record!(
    AgentRecord,
    ConversationRecord,
    TaskRecord,
    ProjectRecord,
    PluginStateRecord,
    MediaStateRecord,
    ClientSettingRecord,
    ClipboardRecord,
    StoryRecord,
    NotepadRecord,
    TerminalHistoryRecord,
);

impl RepositoryRecord for AgentRunStateRecord {
    fn metadata(&self) -> &RecordMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RecordMetadata {
        &mut self.metadata
    }

    fn validate(&self) -> StorageResult<()> {
        validate_record_identity(&self.metadata)?;
        self.run
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("invalid local Agent run: {error}"),
            })?;
        if self.metadata.id != self.run.run_id
            || self.metadata.scope.owner_user_id != self.run.owner_user_id
        {
            return Err(StorageError::InvalidData {
                reason: "Agent run storage identity does not match the protocol record".to_string(),
            });
        }
        Ok(())
    }

    fn validate_persisted(&self) -> StorageResult<()> {
        self.validate()?;
        if self.metadata.revision != self.run.version {
            return Err(StorageError::InvalidData {
                reason: "Agent run storage revision does not match the run version".to_string(),
            });
        }
        Ok(())
    }
}

impl RepositoryRecord for AgentEventStateRecord {
    fn metadata(&self) -> &RecordMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RecordMetadata {
        &mut self.metadata
    }

    fn validate(&self) -> StorageResult<()> {
        validate_record_identity(&self.metadata)?;
        self.event
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("invalid local Agent event: {error}"),
            })?;
        if self.metadata.id != self.event.event_id {
            return Err(StorageError::InvalidData {
                reason: "Agent event storage identity does not match the protocol record"
                    .to_string(),
            });
        }
        Ok(())
    }
}

macro_rules! impl_protocol_repository_record {
    ($record:ty, $field:ident, $id_field:ident, $label:literal) => {
        impl RepositoryRecord for $record {
            fn metadata(&self) -> &RecordMetadata {
                &self.metadata
            }

            fn metadata_mut(&mut self) -> &mut RecordMetadata {
                &mut self.metadata
            }

            fn validate(&self) -> StorageResult<()> {
                validate_record_identity(&self.metadata)?;
                self.$field
                    .validate()
                    .map_err(|error| StorageError::InvalidData {
                        reason: format!(concat!("invalid ", $label, ": {}"), error),
                    })?;
                if self.metadata.id != self.$field.$id_field {
                    return Err(StorageError::InvalidData {
                        reason: concat!(
                            $label,
                            " storage identity does not match the protocol record"
                        )
                        .to_string(),
                    });
                }
                Ok(())
            }
        }
    };
}

impl_protocol_repository_record!(AgentMessageStateRecord, message, record_id, "Agent message");
impl_protocol_repository_record!(
    ProviderContextStateRecord,
    item,
    item_id,
    "provider context"
);
impl_protocol_repository_record!(
    ToolExecutionStateRecord,
    execution,
    invocation_id,
    "tool execution"
);
impl_protocol_repository_record!(SyncOutboxStateRecord, item, outbox_id, "sync outbox");

pub(crate) struct JsonRecordRepository<'store, Record> {
    store: &'store mut dyn RecordStore,
    table: &'static str,
    record: PhantomData<Record>,
}

impl<'store, Record> JsonRecordRepository<'store, Record> {
    pub(crate) fn new(store: &'store mut dyn RecordStore, table: &'static str) -> Self {
        debug_assert!(DOMAIN_TABLES.contains(&table));
        Self {
            store,
            table,
            record: PhantomData,
        }
    }
}

impl<Record> JsonRecordRepository<'_, Record>
where
    Record: RepositoryRecord,
{
    async fn get_record(&mut self, query: &RecordQuery) -> StorageResult<Option<Record>> {
        self.store
            .get_json(self.table, &query.scope.owner_user_id, &query.id)
            .await?
            .map(|payload| {
                decode_record(self.table, &query.scope.owner_user_id, &query.id, payload)
            })
            .transpose()
    }

    async fn list_records(&mut self, query: &ListQuery) -> StorageResult<RecordPage<Record>> {
        query
            .validate()
            .map_err(|reason| StorageError::InvalidData {
                reason: reason.to_string(),
            })?;
        let rows = self
            .store
            .list_json(
                self.table,
                &query.scope.owner_user_id,
                query.cursor.as_deref(),
                query.limit,
            )
            .await?;
        let next_cursor = if rows.len() == query.limit as usize {
            rows.last().map(|row| row.id.clone())
        } else {
            None
        };
        let records = rows
            .into_iter()
            .map(|row| decode_record(self.table, &query.scope.owner_user_id, &row.id, row.payload))
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(RecordPage {
            records,
            next_cursor,
        })
    }

    async fn put_record(&mut self, mut command: PutRecord<Record>) -> StorageResult<Record> {
        command.record.validate()?;
        let owner_user_id = command.record.metadata().scope.owner_user_id.clone();
        let id = command.record.metadata().id.clone();
        match command.expected_revision {
            None => {
                let now = Utc::now();
                let metadata = command.record.metadata_mut();
                metadata.revision = 1;
                metadata.created_at = now;
                metadata.updated_at = now;
                command.record.validate_persisted()?;
                let encoded = encode_record(&command.record)?;
                let inserted = self
                    .store
                    .insert_json(
                        self.table,
                        &owner_user_id,
                        &id,
                        1,
                        &now.to_rfc3339(),
                        &now.to_rfc3339(),
                        &encoded.json,
                        &encoded.digest,
                    )
                    .await?;
                if !inserted {
                    return Err(StorageError::Conflict {
                        actual_revision: self
                            .current_revision(&owner_user_id, &id)
                            .await?
                            .unwrap_or(0),
                    });
                }
            }
            Some(expected_revision) => {
                let existing = self
                    .get_record(&RecordQuery {
                        scope: command.record.metadata().scope.clone(),
                        id: id.clone(),
                    })
                    .await?
                    .ok_or(StorageError::NotFound)?;
                command.record.metadata_mut().created_at = existing.metadata().created_at;
                command.record.metadata_mut().updated_at = Utc::now();
                command.record.metadata_mut().revision =
                    expected_revision
                        .checked_add(1)
                        .ok_or(StorageError::InvalidData {
                            reason: "record revision overflow".to_string(),
                        })?;
                let expected_revision_i64 = stored_revision(expected_revision)?;
                let next_revision_i64 = stored_revision(command.record.metadata().revision)?;
                command.record.validate_persisted()?;
                let encoded = encode_record(&command.record)?;
                let updated = self
                    .store
                    .update_json(
                        self.table,
                        &owner_user_id,
                        &id,
                        expected_revision_i64,
                        next_revision_i64,
                        &command.record.metadata().updated_at.to_rfc3339(),
                        &encoded.json,
                        &encoded.digest,
                    )
                    .await?;
                if !updated {
                    return Err(StorageError::Conflict {
                        actual_revision: self
                            .current_revision(&owner_user_id, &id)
                            .await?
                            .unwrap_or(0),
                    });
                }
            }
        }
        Ok(command.record)
    }

    async fn restore_record(&mut self, record: Record) -> StorageResult<Record> {
        record.validate_persisted()?;
        if record.metadata().revision == 0 {
            return Err(StorageError::InvalidData {
                reason: "restored record revision must be greater than zero".to_string(),
            });
        }
        if record.metadata().updated_at < record.metadata().created_at {
            return Err(StorageError::InvalidData {
                reason: "restored record updated_at precedes created_at".to_string(),
            });
        }
        let owner_user_id = record.metadata().scope.owner_user_id.clone();
        let id = record.metadata().id.clone();
        let revision = stored_revision(record.metadata().revision)?;
        let encoded = encode_record(&record)?;
        let inserted = self
            .store
            .insert_json(
                self.table,
                &owner_user_id,
                &id,
                revision,
                &record.metadata().created_at.to_rfc3339(),
                &record.metadata().updated_at.to_rfc3339(),
                &encoded.json,
                &encoded.digest,
            )
            .await?;
        if !inserted {
            return Err(StorageError::Conflict {
                actual_revision: self
                    .current_revision(&owner_user_id, &id)
                    .await?
                    .unwrap_or(0),
            });
        }
        Ok(record)
    }

    async fn delete_record(
        &mut self,
        query: &RecordQuery,
        expected_revision: u64,
    ) -> StorageResult<()> {
        if self
            .store
            .delete(
                self.table,
                &query.scope.owner_user_id,
                &query.id,
                stored_revision(expected_revision)?,
            )
            .await?
        {
            return Ok(());
        }
        match self
            .current_revision(&query.scope.owner_user_id, &query.id)
            .await?
        {
            Some(actual_revision) => Err(StorageError::Conflict { actual_revision }),
            None => Err(StorageError::NotFound),
        }
    }

    async fn current_revision(
        &mut self,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<u64>> {
        self.store
            .current_revision(self.table, owner_user_id, id)
            .await?
            .map(|value| {
                u64::try_from(value).map_err(|_| StorageError::InvalidData {
                    reason: format!("stored revision is negative for record {id}"),
                })
            })
            .transpose()
    }
}

macro_rules! impl_domain_repository {
    ($trait_name:ident, $record:ty) => {
        #[async_trait]
        impl $trait_name for JsonRecordRepository<'_, $record> {
            async fn get(&mut self, query: &RecordQuery) -> StorageResult<Option<$record>> {
                self.get_record(query).await
            }
            async fn list(&mut self, query: &ListQuery) -> StorageResult<RecordPage<$record>> {
                self.list_records(query).await
            }
            async fn put(&mut self, command: PutRecord<$record>) -> StorageResult<$record> {
                self.put_record(command).await
            }
            async fn restore(&mut self, record: $record) -> StorageResult<$record> {
                self.restore_record(record).await
            }
            async fn delete(
                &mut self,
                query: &RecordQuery,
                expected_revision: u64,
            ) -> StorageResult<()> {
                self.delete_record(query, expected_revision).await
            }
        }
    };
}

impl_domain_repository!(AgentRepository, AgentRecord);
impl_domain_repository!(AgentRunStateRepository, AgentRunStateRecord);
impl_domain_repository!(AgentEventStateRepository, AgentEventStateRecord);
impl_domain_repository!(AgentMessageStateRepository, AgentMessageStateRecord);
impl_domain_repository!(ProviderContextStateRepository, ProviderContextStateRecord);
impl_domain_repository!(ToolExecutionStateRepository, ToolExecutionStateRecord);
impl_domain_repository!(SyncOutboxStateRepository, SyncOutboxStateRecord);
impl_domain_repository!(ConversationRepository, ConversationRecord);
impl_domain_repository!(TaskRepository, TaskRecord);
impl_domain_repository!(ProjectRepository, ProjectRecord);
impl_domain_repository!(PluginStateRepository, PluginStateRecord);
impl_domain_repository!(MediaStateRepository, MediaStateRecord);
impl_domain_repository!(ClientSettingsRepository, ClientSettingRecord);
impl_domain_repository!(ClipboardRepository, ClipboardRecord);
impl_domain_repository!(StoryRepository, StoryRecord);
impl_domain_repository!(NotepadRepository, NotepadRecord);
impl_domain_repository!(TerminalHistoryRepository, TerminalHistoryRecord);

fn validate_record_identity(metadata: &RecordMetadata) -> StorageResult<()> {
    if metadata.id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "record id must not be empty".to_string(),
        });
    }
    if metadata.scope.owner_user_id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "owner_user_id must not be empty".to_string(),
        });
    }
    if metadata.origin_device_id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "origin_device_id must not be empty".to_string(),
        });
    }
    Ok(())
}

fn encode_record<Record: Serialize>(record: &Record) -> StorageResult<CanonicalRecord> {
    encode_canonical(record)
}

fn decode_record<Record: RepositoryRecord>(
    table: &'static str,
    owner_user_id: &str,
    id: &str,
    payload: StoredPayload,
) -> StorageResult<Record> {
    verify_canonical(table, id, &payload.record_json, &payload.record_digest)?;
    let record: Record =
        serde_json::from_str(&payload.record_json).map_err(|error| StorageError::InvalidData {
            reason: format!("stored record is invalid: {error}"),
        })?;
    record
        .validate_persisted()
        .map_err(|_| StorageError::RecordIntegrity {
            table,
            id: id.to_string(),
        })?;
    let metadata = record.metadata();
    if metadata.id != id
        || metadata.scope.owner_user_id != owner_user_id
        || stored_revision(metadata.revision)? != payload.revision
        || metadata.created_at.to_rfc3339() != payload.created_at
        || metadata.updated_at.to_rfc3339() != payload.updated_at
    {
        return Err(StorageError::RecordIntegrity {
            table,
            id: id.to_string(),
        });
    }
    Ok(record)
}

fn stored_revision(revision: u64) -> StorageResult<i64> {
    i64::try_from(revision).map_err(|_| StorageError::InvalidData {
        reason: "record revision exceeds the supported range".to_string(),
    })
}
