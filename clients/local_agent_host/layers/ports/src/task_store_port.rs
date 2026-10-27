// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[async_trait]
pub trait LocalAgentTaskStore: Send + Sync {
    async fn resolve_task_followup(
        &self,
        command: &IdempotentCommand,
        review: &chatos_local_agent_protocol::ResolveTaskFollowupCommand,
        now_unix_ms: i64,
    ) -> Result<Value, ClientStorageError>;

    async fn create_task_graph(
        &self,
        command: &IdempotentCommand,
        graph: &CreateTaskGraphCommand,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn get_task_graph(
        &self,
        owner_user_id: &str,
        graph_id: &str,
    ) -> Result<Option<LocalTaskGraph>, ClientStorageError>;

    async fn list_task_graphs(
        &self,
        owner_user_id: &str,
        scope: LocalTaskGraphListScope,
        source_entity_type: Option<&str>,
        source_entity_id: Option<&str>,
        before_updated_at_unix_ms: Option<i64>,
        before_graph_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalTaskGraphPage, ClientStorageError>;

    async fn list_task_runs(
        &self,
        owner_user_id: &str,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentRunRecord>, ClientStorageError>;

    /// Returns at most one newest Run presentation for each requested Task. Implementations must
    /// neither sort nor return full Run input, checkpoint, continuation, Tool batch, or claim data.
    /// Terminal presentation text is bounded because this snapshot shares the multiplexed IPC
    /// transport with unrelated conversations.
    async fn latest_task_runs(
        &self,
        owner_user_id: &str,
        task_ids: &[String],
    ) -> Result<Vec<LocalMessageTaskGraphRunSummary>, ClientStorageError>;

    async fn list_tasks_for_conversation(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        status: Option<LocalTaskStatus>,
        keyword: Option<&str>,
        tag: Option<&str>,
        scheduled_only: Option<bool>,
        parent_task_id: Option<&str>,
        source_run_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<LocalTaskRecord>, ClientStorageError>;

    async fn get_task_for_conversation(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<Option<LocalTaskRecord>, ClientStorageError>;

    async fn start_next_task_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    async fn start_next_task_run_for_graph(
        &self,
        owner_user_id: &str,
        graph_id: &str,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn cancel_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        replacement_task_ids: &[String],
        run_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn retry_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: u64,
        retry_instruction: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn restart_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: u64,
        reason: &str,
        run_event_prefix: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;
}
