// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;

use crate::{
    AgentRepository, ClientSettingsRepository, ConversationRepository, MediaStateRepository,
    PluginStateRepository, ProjectRepository, StorageBackend, StorageResult, TaskRepository,
};

/// Repository views bound to one backend transaction.
pub trait TransactionRepositories: Send {
    fn agents(&mut self) -> &mut dyn AgentRepository;
    fn conversations(&mut self) -> &mut dyn ConversationRepository;
    fn tasks(&mut self) -> &mut dyn TaskRepository;
    fn projects(&mut self) -> &mut dyn ProjectRepository;
    fn plugins(&mut self) -> &mut dyn PluginStateRepository;
    fn media(&mut self) -> &mut dyn MediaStateRepository;
    fn settings(&mut self) -> &mut dyn ClientSettingsRepository;
}

/// A caller-owned operation executed atomically by the selected backend.
///
/// Results can be retained in fields on the operation. The backend commits
/// only when this method succeeds and rolls back on every error or panic.
#[async_trait]
pub trait StorageTransaction: Send {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()>;
}

#[async_trait]
pub trait ClientStorage: Send + Sync {
    fn backend(&self) -> StorageBackend;

    /// Executes exactly one atomic transaction. Implementations must not open
    /// or write to a second backend when this call fails.
    async fn transaction(&self, operation: &mut dyn StorageTransaction) -> StorageResult<()>;
}
