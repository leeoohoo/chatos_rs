// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp::{TaskDraft, TaskManagerStore, TaskStreamChunkCallback, TaskUpdatePatch};
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
}

impl LocalTaskManagerStore {
    pub(super) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        ask_user_prompts: LocalAskUserPromptRegistry,
    ) -> Self {
        Self {
            database,
            owner_user_id: owner_user_id.into(),
            ask_user_prompts,
        }
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
        records_to_values(
            self.database
                .create_local_task_board_tasks(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    conversation_turn_id,
                    draft_tasks,
                )
                .await
                .map_err(|error| error.to_string())?,
        )
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
        records_to_values(
            self.database
                .list_local_task_board_tasks(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    conversation_turn_id,
                    include_done,
                    limit,
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    }

    async fn update_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: TaskUpdatePatch,
    ) -> Result<Value, String> {
        serde_json::to_value(
            self.database
                .update_local_task_board_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_id,
                    patch,
                )
                .await
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }

    async fn complete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: Option<TaskUpdatePatch>,
    ) -> Result<Value, String> {
        serde_json::to_value(
            self.database
                .complete_local_task_board_task(
                    self.owner_user_id.as_str(),
                    conversation_id,
                    task_id,
                    patch.unwrap_or_default(),
                )
                .await
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }

    async fn delete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        self.database
            .delete_local_task_board_task(self.owner_user_id.as_str(), conversation_id, task_id)
            .await
            .map_err(|error| error.to_string())
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
