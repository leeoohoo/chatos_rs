use super::support::{
    apply_manager_patch, shared_outcome_items_into_tool_state, task_belongs_to_context,
    task_manager_status_from_task_status, task_priority_from_manager_label,
    task_status_from_manager_status,
};
use super::*;

impl TaskService {
    pub(super) async fn create_followup_task_for_tool(
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

    pub(super) async fn list_tool_tasks(
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

    pub(super) async fn update_task_from_tool(
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

    pub(super) async fn complete_task_from_tool(
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

    pub(super) async fn delete_task_from_tool(
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
