use super::support::task_to_manager_value;
use super::*;

#[async_trait]
impl TaskManagerStore for TaskRunnerTaskManagerStore {
    async fn create_tasks_for_turn(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<SharedTaskDraft>,
    ) -> Result<Vec<Value>, String> {
        let draft_count = draft_tasks.len();
        let draft_titles = draft_tasks
            .iter()
            .map(|draft| draft.title.trim().to_string())
            .filter(|title| !title.is_empty())
            .collect::<Vec<_>>();
        info!(
            task_id = conversation_id,
            run_id = conversation_turn_id,
            draft_count,
            draft_titles = draft_titles.join(" | "),
            "task manager received create_tasks_for_turn request"
        );
        let mut created = Vec::with_capacity(draft_count);
        for draft in draft_tasks {
            let task = self
                .task_service
                .create_followup_task_for_tool(conversation_id, conversation_turn_id, draft)
                .await?;
            created.push(task_to_manager_value(&task));
        }
        info!(
            task_id = conversation_id,
            run_id = conversation_turn_id,
            created_count = created.len(),
            "task manager finished create_tasks_for_turn request"
        );
        Ok(created)
    }

    async fn review_and_create_tasks(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<SharedTaskDraft>,
        _timeout_ms: u64,
        _on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tasks = self
            .create_tasks_for_turn(conversation_id, conversation_turn_id, draft_tasks)
            .await?;
        Ok(json!({
            "confirmed": true,
            "cancelled": false,
            "auto_created": true,
            "created_count": tasks.len(),
            "tasks": tasks,
            "conversation_id": conversation_id,
            "conversation_turn_id": conversation_turn_id,
        }))
    }

    async fn list_tasks_for_context(
        &self,
        conversation_id: &str,
        conversation_turn_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let tasks = self
            .task_service
            .list_tool_tasks(conversation_id, conversation_turn_id, include_done, limit)
            .await?;
        Ok(tasks.iter().map(task_to_manager_value).collect::<Vec<_>>())
    }

    async fn update_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<Value, String> {
        let task = self
            .task_service
            .update_task_from_tool(conversation_id, task_id, patch)
            .await?;
        Ok(task_to_manager_value(&task))
    }

    async fn complete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<Value, String> {
        let task = self
            .task_service
            .complete_task_from_tool(conversation_id, task_id, patch)
            .await?;
        Ok(task_to_manager_value(&task))
    }

    async fn delete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        self.task_service
            .delete_task_from_tool(conversation_id, task_id)
            .await
    }

    async fn task_board_updated_event(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
    ) -> Option<Value> {
        Some(json!({
            "event": "task_runner_task_board_updated",
            "data": {
                "task_id": conversation_id,
                "run_id": conversation_turn_id,
            }
        }))
    }
}
