// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;

use crate::{
    AgentEventStateRecord, AgentMessageStateRecord, AgentRecord, AgentRunStateRecord,
    ClientSettingRecord, ClipboardRecord, ConversationRecord, ListQuery, MediaStateRecord,
    NotepadRecord, PluginStateRecord, ProjectRecord, ProviderContextStateRecord, PutRecord,
    RecordPage, RecordQuery, StorageResult, StoryRecord, SyncOutboxStateRecord, TaskRecord,
    TerminalHistoryRecord, ToolExecutionStateRecord,
};

macro_rules! define_domain_repository {
    ($trait_name:ident, $record:ty) => {
        #[async_trait]
        pub trait $trait_name: Send {
            async fn get(&mut self, query: &RecordQuery) -> StorageResult<Option<$record>>;
            async fn list(&mut self, query: &ListQuery) -> StorageResult<RecordPage<$record>>;
            async fn put(&mut self, command: PutRecord<$record>) -> StorageResult<$record>;
            /// Restores a verified archive record without changing its
            /// revision or UTC timestamps. Existing records are rejected.
            async fn restore(&mut self, record: $record) -> StorageResult<$record>;
            async fn delete(
                &mut self,
                query: &RecordQuery,
                expected_revision: u64,
            ) -> StorageResult<()>;
        }
    };
}

define_domain_repository!(AgentRepository, AgentRecord);
define_domain_repository!(AgentRunStateRepository, AgentRunStateRecord);
define_domain_repository!(AgentEventStateRepository, AgentEventStateRecord);
define_domain_repository!(AgentMessageStateRepository, AgentMessageStateRecord);
define_domain_repository!(ProviderContextStateRepository, ProviderContextStateRecord);
define_domain_repository!(ToolExecutionStateRepository, ToolExecutionStateRecord);
define_domain_repository!(SyncOutboxStateRepository, SyncOutboxStateRecord);
define_domain_repository!(ConversationRepository, ConversationRecord);
define_domain_repository!(TaskRepository, TaskRecord);
define_domain_repository!(ProjectRepository, ProjectRecord);
define_domain_repository!(PluginStateRepository, PluginStateRecord);
define_domain_repository!(MediaStateRepository, MediaStateRecord);
define_domain_repository!(ClientSettingsRepository, ClientSettingRecord);
define_domain_repository!(ClipboardRepository, ClipboardRecord);
define_domain_repository!(StoryRepository, StoryRecord);
define_domain_repository!(NotepadRepository, NotepadRecord);
define_domain_repository!(TerminalHistoryRepository, TerminalHistoryRecord);
