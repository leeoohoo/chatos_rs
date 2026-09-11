// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;

use crate::{
    AgentEventStateRepository, AgentMessageStateRepository, AgentRepository,
    AgentRunStateRepository, ClientSettingsRepository, ClipboardRepository, ConversationRepository,
    MediaStateRepository, NotepadRepository, PluginStateRepository, ProjectRepository,
    ProviderContextStateRepository, StorageBackend, StorageResult, StoryRepository,
    SyncOutboxStateRepository, TaskRepository, TerminalHistoryRepository,
    ToolExecutionStateRepository,
};

/// Repository views bound to one backend transaction.
pub trait TransactionRepositories: Send {
    fn agents(&mut self) -> Box<dyn AgentRepository + '_>;
    fn agent_runs(&mut self) -> Box<dyn AgentRunStateRepository + '_>;
    fn agent_events(&mut self) -> Box<dyn AgentEventStateRepository + '_>;
    fn agent_messages(&mut self) -> Box<dyn AgentMessageStateRepository + '_>;
    fn provider_context(&mut self) -> Box<dyn ProviderContextStateRepository + '_>;
    fn tool_executions(&mut self) -> Box<dyn ToolExecutionStateRepository + '_>;
    fn sync_outbox(&mut self) -> Box<dyn SyncOutboxStateRepository + '_>;
    fn conversations(&mut self) -> Box<dyn ConversationRepository + '_>;
    fn tasks(&mut self) -> Box<dyn TaskRepository + '_>;
    fn projects(&mut self) -> Box<dyn ProjectRepository + '_>;
    fn plugins(&mut self) -> Box<dyn PluginStateRepository + '_>;
    fn media(&mut self) -> Box<dyn MediaStateRepository + '_>;
    fn settings(&mut self) -> Box<dyn ClientSettingsRepository + '_>;
    fn clipboard(&mut self) -> Box<dyn ClipboardRepository + '_>;
    fn stories(&mut self) -> Box<dyn StoryRepository + '_>;
    fn notepad(&mut self) -> Box<dyn NotepadRepository + '_>;
    fn terminal_history(&mut self) -> Box<dyn TerminalHistoryRepository + '_>;
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
