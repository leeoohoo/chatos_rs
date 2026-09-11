// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;

use crate::{
    AgentRecord, ClientSettingRecord, ConversationRecord, ListQuery, MediaStateRecord,
    PluginStateRecord, ProjectRecord, PutRecord, RecordPage, RecordQuery, StorageResult,
    TaskRecord,
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
define_domain_repository!(ConversationRepository, ConversationRecord);
define_domain_repository!(TaskRepository, TaskRecord);
define_domain_repository!(ProjectRepository, ProjectRecord);
define_domain_repository!(PluginStateRepository, PluginStateRecord);
define_domain_repository!(MediaStateRepository, MediaStateRecord);
define_domain_repository!(ClientSettingsRepository, ClientSettingRecord);
