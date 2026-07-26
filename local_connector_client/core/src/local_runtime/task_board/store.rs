// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp::{
    TaskClosureDecision, TaskDraft, TaskManagerStore, TaskStreamChunkCallback, TaskUpdatePatch,
};
use serde_json::{json, Value};

use crate::local_runtime::storage::LocalDatabase;
use crate::local_runtime::LocalAskUserPromptRegistry;

#[path = "store/review.rs"]
mod review;

#[derive(Clone)]
pub(super) struct LocalTaskManagerStore {
    database: LocalDatabase,
    owner_user_id: String,
    ask_user_prompts: LocalAskUserPromptRegistry,
    task_session_id: Option<String>,
}

impl LocalTaskManagerStore {
    pub(super) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        ask_user_prompts: LocalAskUserPromptRegistry,
        task_session_id: Option<String>,
    ) -> Self {
        Self {
            database,
            owner_user_id: owner_user_id.into(),
            ask_user_prompts,
            task_session_id,
        }
    }

    fn task_session_id(&self) -> Option<&str> {
        self.task_session_id.as_deref()
    }
}

#[async_trait]
impl TaskManagerStore for LocalTaskManagerStore {
    async fn create_tasks_for_turn(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<TaskDraft>,
    ) -> Result<Vec<Value>, String> {
        records_to_values(if let Some(task_session_id) = self.task_session_id() {
            self.database
                .create_local_task_manager_session_tasks(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    conversation_turn_id,
                    task_session_id,
                    draft_tasks,
                )
                .await
                .map_err(|error| error.to_string())?
        } else {
            self.database
                .create_local_task_board_tasks(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    conversation_turn_id,
                    draft_tasks,
                )
                .await
                .map_err(|error| error.to_string())?
        })
    }

    async fn review_and_create_tasks(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<TaskDraft>,
        timeout_ms: u64,
        on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String> {
        review::review_and_create_tasks(
            self,
            conversation_id,
            conversation_turn_id,
            draft_tasks,
            timeout_ms,
            on_stream_chunk,
        )
        .await
    }

    async fn list_tasks_for_context(
        &self,
        conversation_id: &str,
        conversation_turn_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        records_to_values(if let Some(task_session_id) = self.task_session_id() {
            self.database
                .list_local_task_manager_session_tasks(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_session_id,
                    include_done,
                    limit,
                )
                .await
                .map_err(|error| error.to_string())?
        } else {
            self.database
                .list_local_task_board_tasks(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    conversation_turn_id,
                    include_done,
                    limit,
                )
                .await
                .map_err(|error| error.to_string())?
        })
    }

    async fn update_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: TaskUpdatePatch,
    ) -> Result<Value, String> {
        serde_json::to_value(if let Some(task_session_id) = self.task_session_id() {
            self.database
                .update_local_task_manager_session_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_session_id,
                    task_id,
                    patch,
                )
                .await
                .map_err(|error| error.to_string())?
        } else {
            self.database
                .update_local_task_board_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_id,
                    patch,
                )
                .await
                .map_err(|error| error.to_string())?
        })
        .map_err(|error| error.to_string())
    }

    async fn update_task_for_turn(
        &self,
        conversation_id: &str,
        _conversation_turn_id: &str,
        task_id: &str,
        patch: TaskUpdatePatch,
    ) -> Result<Value, String> {
        self.update_task_by_id(conversation_id, task_id, patch)
            .await
    }

    async fn complete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: Option<TaskUpdatePatch>,
    ) -> Result<Value, String> {
        serde_json::to_value(if let Some(task_session_id) = self.task_session_id() {
            self.database
                .complete_local_task_manager_session_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_session_id,
                    task_id,
                    patch.unwrap_or_default(),
                )
                .await
                .map_err(|error| error.to_string())?
        } else {
            self.database
                .complete_local_task_board_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_id,
                    patch.unwrap_or_default(),
                )
                .await
                .map_err(|error| error.to_string())?
        })
        .map_err(|error| error.to_string())
    }

    async fn complete_task_for_turn(
        &self,
        conversation_id: &str,
        _conversation_turn_id: &str,
        task_id: &str,
        patch: Option<TaskUpdatePatch>,
    ) -> Result<Value, String> {
        self.complete_task_by_id(conversation_id, task_id, patch)
            .await
    }

    async fn delete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        if let Some(task_session_id) = self.task_session_id() {
            self.database
                .delete_local_task_manager_session_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_session_id,
                    task_id,
                )
                .await
                .map_err(|error| error.to_string())
        } else {
            self.database
                .delete_local_task_board_task(self.owner_user_id.as_str(), conversation_id, task_id)
                .await
                .map_err(|error| error.to_string())
        }
    }

    async fn delete_task_for_turn(
        &self,
        conversation_id: &str,
        _conversation_turn_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        self.delete_task_by_id(conversation_id, task_id).await
    }

    async fn reconcile_tasks_for_turn(
        &self,
        conversation_id: &str,
        _conversation_turn_id: &str,
        decisions: Vec<TaskClosureDecision>,
    ) -> Result<Value, String> {
        let task_session_id = self.task_session_id().ok_or_else(|| {
            "task lifecycle reconciliation is unavailable in this host".to_string()
        })?;
        self.database
            .reconcile_local_task_manager_session(
                self.owner_user_id.as_str(),
                conversation_id,
                task_session_id,
                decisions,
            )
            .await
            .map_err(|error| error.to_string())
    }

    async fn finalize_session_for_turn(
        &self,
        conversation_id: &str,
        _conversation_turn_id: &str,
    ) -> Result<Value, String> {
        let task_session_id = self
            .task_session_id()
            .ok_or_else(|| "task lifecycle finalization is unavailable in this host".to_string())?;
        let snapshot = self
            .database
            .local_task_manager_session_snapshot(
                self.owner_user_id.as_str(),
                conversation_id,
                task_session_id,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(json!({
            "finalized": snapshot.open_required.is_empty(),
            "can_parent_succeed": snapshot.open_required.is_empty()
                && snapshot.terminal_blocked.is_empty(),
            "parent_should_block": !snapshot.terminal_blocked.is_empty(),
            "session": snapshot.as_value(),
        }))
    }

    async fn task_board_updated_event(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
    ) -> Option<Value> {
        let task_board = self
            .database
            .local_task_board_prompt(self.owner_user_id.as_str(), conversation_id)
            .await
            .ok()?;
        Some(json!({
            "event": "conversation.task_board.updated",
            "data": {
                "conversation_id": conversation_id,
                "conversation_turn_id": conversation_turn_id,
                "task_session_id": self.task_session_id(),
                "task_board": task_board,
                "runtime_origin": "local_device"
            }
        }))
    }
}

fn records_to_values<T: serde::Serialize>(records: Vec<T>) -> Result<Vec<Value>, String> {
    records
        .into_iter()
        .map(|record| serde_json::to_value(record).map_err(|error| error.to_string()))
        .collect()
}
