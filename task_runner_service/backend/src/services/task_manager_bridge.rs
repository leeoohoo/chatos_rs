use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;
use uuid::Uuid;

use chatos_builtin_tools::{
    TaskDraft as SharedTaskDraft, TaskManagerStore, TaskOutcomeItem as SharedTaskOutcomeItem,
    TaskStreamChunkCallback, TaskUpdatePatch as SharedTaskUpdatePatch, TASK_NOT_FOUND_ERR,
};

use crate::models::{
    now_rfc3339, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolOutcomeItem, TaskToolState,
};

use super::{
    normalize_strings, normalized_optional, normalized_optional_nested, validate_required,
    TaskService, TaskStatusExt,
};

pub(super) struct TaskRunnerTaskManagerStore {
    task_service: TaskService,
}

impl TaskRunnerTaskManagerStore {
    pub(super) fn new(task_service: TaskService) -> Self {
        Self { task_service }
    }
}

impl TaskService {
    async fn create_followup_task_for_tool(
        &self,
        root_task_id: &str,
        run_id: &str,
        draft: SharedTaskDraft,
    ) -> Result<TaskRecord, String> {
        validate_required("title", &draft.title)?;
        let parent = self
            .store
            .get_task(root_task_id)
            .await?
            .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let title = draft.title.trim().to_string();
        let description = normalized_optional(Some(draft.details));
        let objective = description.clone().unwrap_or_else(|| title.clone());
        let result_summary = normalized_optional(Some(draft.outcome_summary));
        let status = task_status_from_manager_status(draft.status.as_str());
        let mut task_tool_state = TaskToolState {
            due_at: normalized_optional_nested(draft.due_at),
            outcome_items: shared_outcome_items_into_tool_state(draft.outcome_items),
            resume_hint: normalized_optional(Some(draft.resume_hint)),
            blocker_reason: normalized_optional(Some(draft.blocker_reason)),
            blocker_needs: normalize_strings(draft.blocker_needs),
            blocker_kind: normalized_optional(Some(draft.blocker_kind)),
            completed_at: None,
            last_outcome_at: None,
        };
        if result_summary.is_some() || !task_tool_state.outcome_items.is_empty() {
            task_tool_state.last_outcome_at = Some(now.clone());
        }
        if task_manager_status_from_task_status(status) == "done" {
            task_tool_state.completed_at = Some(now.clone());
        }

        let task = TaskRecord {
            id: id.clone(),
            title,
            description,
            objective,
            input_payload: None,
            status,
            priority: task_priority_from_manager_label(draft.priority.as_str()),
            tags: normalize_strings(draft.tags),
            default_model_config_id: parent.default_model_config_id.clone(),
            memory_thread_id: format!("task-{id}"),
            tenant_id: parent.tenant_id.clone(),
            subject_id: parent.subject_id.clone(),
            creator_user_id: parent.creator_user_id.clone(),
            creator_username: parent.creator_username.clone(),
            creator_display_name: parent.creator_display_name.clone(),
            result_summary,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: Some(parent.id.clone()),
            source_run_id: Some(run_id.to_string()),
            source_session_id: parent.source_session_id.clone(),
            source_turn_id: parent.source_turn_id.clone(),
            source_user_message_id: parent.source_user_message_id.clone(),
            prerequisite_task_ids: Vec::new(),
            task_tool_state,
            mcp_config: parent.mcp_config.clone(),
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.ensure_task_thread(&task).await?;
        let saved = self.store.save_task(task).await?;
        info!(
            root_task_id,
            source_run_id = run_id,
            created_task_id = saved.id.as_str(),
            created_task_title = saved.title.as_str(),
            created_task_status = saved.status.status_string(),
            "task manager auto-created follow-up task during task run"
        );
        Ok(saved)
    }

    async fn list_tool_tasks(
        &self,
        root_task_id: &str,
        source_run_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<TaskRecord>, String> {
        if self.store.get_task(root_task_id).await?.is_none() {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        let mut tasks = self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .filter(|task| task_belongs_to_context(task, root_task_id))
            .collect::<Vec<_>>();
        if let Some(run_id) = source_run_id {
            tasks.retain(|task| task.source_run_id.as_deref() == Some(run_id));
        }
        if !include_done {
            tasks.retain(|task| task_manager_status_from_task_status(task.status) != "done");
        }
        tasks.sort_by(|left, right| {
            if left.id == root_task_id && right.id != root_task_id {
                std::cmp::Ordering::Less
            } else if right.id == root_task_id && left.id != root_task_id {
                std::cmp::Ordering::Greater
            } else {
                right.updated_at.cmp(&left.updated_at)
            }
        });
        tasks.truncate(limit);
        Ok(tasks)
    }

    async fn update_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }

        let now = now_rfc3339();
        apply_manager_patch(&mut task, patch, false, now.as_str())?;
        task.updated_at = now;
        self.ensure_task_thread(&task).await?;
        self.store.save_task(task).await
    }

    async fn complete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }

        let now = now_rfc3339();
        if let Some(patch) = patch {
            apply_manager_patch(&mut task, patch, true, now.as_str())?;
        } else {
            task.status = TaskStatus::Succeeded;
            task.task_tool_state.completed_at = Some(now.clone());
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        task.status = TaskStatus::Succeeded;
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.clone());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        task.updated_at = now;
        self.ensure_task_thread(&task).await?;
        self.store.save_task(task).await
    }

    async fn delete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        if task_id == root_task_id {
            return Err("不能删除当前正在执行的根任务".to_string());
        }
        let Some(task) = self.store.get_task(task_id).await? else {
            return Ok(false);
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Ok(false);
        }
        if self.store.has_active_run_for_task(task_id).await? {
            return Err("任务仍有运行中的执行记录，暂时不能删除".to_string());
        }
        self.store.delete_task(task_id).await
    }
}

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

fn task_belongs_to_context(task: &TaskRecord, root_task_id: &str) -> bool {
    task.id == root_task_id || task.parent_task_id.as_deref() == Some(root_task_id)
}

fn task_to_manager_value(task: &TaskRecord) -> Value {
    json!({
        "id": task.id.clone(),
        "parent_task_id": task.parent_task_id.clone(),
        "source_run_id": task.source_run_id.clone(),
        "title": task.title.clone(),
        "details": task
            .description
            .clone()
            .or_else(|| normalized_optional(Some(task.objective.clone()))),
        "priority": task_priority_to_manager_label(task.priority),
        "status": task_manager_status_from_task_status(task.status),
        "tags": task.tags.clone(),
        "due_at": task.task_tool_state.due_at.clone(),
        "outcome_summary": task.result_summary.clone(),
        "outcome_items": tool_state_outcomes_into_shared(&task.task_tool_state.outcome_items),
        "resume_hint": task.task_tool_state.resume_hint.clone(),
        "blocker_reason": task.task_tool_state.blocker_reason.clone(),
        "blocker_needs": task.task_tool_state.blocker_needs.clone(),
        "blocker_kind": task.task_tool_state.blocker_kind.clone(),
        "completed_at": task.task_tool_state.completed_at.clone(),
        "last_outcome_at": task.task_tool_state.last_outcome_at.clone(),
        "created_at": task.created_at.clone(),
        "updated_at": task.updated_at.clone(),
    })
}

fn apply_manager_patch(
    task: &mut TaskRecord,
    patch: SharedTaskUpdatePatch,
    mark_complete: bool,
    now: &str,
) -> Result<(), String> {
    let requested_status = patch.status.as_deref().map(task_status_from_manager_status);
    if let Some(title) = patch.title {
        validate_required("title", &title)?;
        task.title = title.trim().to_string();
    }
    if let Some(details) = patch.details {
        let normalized = normalized_optional(Some(details));
        task.description = normalized.clone();
        if task.parent_task_id.is_some() {
            task.objective = normalized.unwrap_or_else(|| task.title.clone());
        }
    }
    if let Some(priority) = patch.priority {
        task.priority = task_priority_from_manager_label(priority.as_str());
    }
    if let Some(status) = requested_status {
        task.status = status;
    }
    if let Some(tags) = patch.tags {
        task.tags = normalize_strings(tags);
    }
    if let Some(due_at) = patch.due_at {
        task.task_tool_state.due_at = normalized_optional_nested(due_at);
    }
    if let Some(outcome_summary) = patch.outcome_summary {
        task.result_summary = normalized_optional(Some(outcome_summary));
        if task.result_summary.is_some() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    if let Some(outcome_items) = patch.outcome_items {
        task.task_tool_state.outcome_items = shared_outcome_items_into_tool_state(outcome_items);
        if !task.task_tool_state.outcome_items.is_empty() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    if let Some(resume_hint) = patch.resume_hint {
        task.task_tool_state.resume_hint = normalized_optional(Some(resume_hint));
    }
    if let Some(blocker_reason) = patch.blocker_reason {
        task.task_tool_state.blocker_reason = normalized_optional(Some(blocker_reason));
    }
    if let Some(blocker_needs) = patch.blocker_needs {
        task.task_tool_state.blocker_needs = normalize_strings(blocker_needs);
    }
    if let Some(blocker_kind) = patch.blocker_kind {
        task.task_tool_state.blocker_kind = normalized_optional(Some(blocker_kind));
    }
    if let Some(completed_at) = patch.completed_at {
        task.task_tool_state.completed_at = normalized_optional_nested(completed_at);
    }
    if let Some(last_outcome_at) = patch.last_outcome_at {
        task.task_tool_state.last_outcome_at = normalized_optional_nested(last_outcome_at);
    }
    if mark_complete || matches!(task.status, TaskStatus::Succeeded) {
        task.status = TaskStatus::Succeeded;
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.to_string());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    Ok(())
}

fn task_status_from_manager_status(value: &str) -> TaskStatus {
    match value.trim().to_ascii_lowercase().as_str() {
        "doing" => TaskStatus::Running,
        "blocked" => TaskStatus::Blocked,
        "done" => TaskStatus::Succeeded,
        _ => TaskStatus::Ready,
    }
}

fn task_manager_status_from_task_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "doing",
        TaskStatus::Blocked | TaskStatus::Failed => "blocked",
        TaskStatus::Succeeded | TaskStatus::Cancelled | TaskStatus::Archived => "done",
        TaskStatus::Draft | TaskStatus::Ready => "todo",
    }
}

fn task_priority_from_manager_label(value: &str) -> i32 {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => 100,
        "low" => 10,
        _ => 50,
    }
}

fn task_priority_to_manager_label(value: i32) -> &'static str {
    if value >= 80 {
        "high"
    } else if value <= 20 {
        "low"
    } else {
        "medium"
    }
}

fn shared_outcome_items_into_tool_state(
    items: Vec<SharedTaskOutcomeItem>,
) -> Vec<TaskToolOutcomeItem> {
    items
        .into_iter()
        .map(|item| TaskToolOutcomeItem {
            kind: item.kind,
            text: item.text,
            importance: item.importance,
            refs: item.refs,
        })
        .collect()
}

fn tool_state_outcomes_into_shared(items: &[TaskToolOutcomeItem]) -> Vec<SharedTaskOutcomeItem> {
    items
        .iter()
        .map(|item| SharedTaskOutcomeItem {
            kind: item.kind.clone(),
            text: item.text.clone(),
            importance: item.importance.clone(),
            refs: item.refs.clone(),
        })
        .collect()
}
