use super::*;

use crate::models::PUBLIC_PROJECT_ID;

impl InMemoryStore {
    pub(in crate::store) fn list_tasks(&self) -> Vec<TaskRecord> {
        let data = self.inner.read();
        let mut items = data.tasks.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Vec<TaskRecord> {
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .filter(|task| filters.status.is_none_or(|value| task.status == value))
            .filter(|task| {
                filters
                    .keyword
                    .as_deref()
                    .is_none_or(|value| task_matches_keyword(task, value))
            })
            .filter(|task| {
                filters
                    .tag
                    .as_deref()
                    .is_none_or(|value| task.tags.iter().any(|item| item == value))
            })
            .filter(|task| {
                filters
                    .model_config_id
                    .as_deref()
                    .is_none_or(|value| task.default_model_config_id.as_deref() == Some(value))
            })
            .filter(|task| {
                filters.project_id.as_deref().is_none_or(|value| {
                    task.project_id == value
                        || (value == PUBLIC_PROJECT_ID
                            && matches!(task.project_id.trim(), "" | "0"))
                })
            })
            .filter(|task| {
                filters.creator_user_id.as_deref().is_none_or(|value| {
                    task.owner_user_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .or_else(|| task.creator_user_id.as_deref())
                        == Some(value)
                })
            })
            .filter(|task| {
                !filters.scheduled_only.unwrap_or(false)
                    || !matches!(task.schedule.mode, TaskScheduleMode::Manual)
            })
            .filter(|task| {
                if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
                    return task.parent_task_id.as_deref() == Some(parent_task_id);
                }
                filters.include_subtasks != Some(false)
                    || task
                        .parent_task_id
                        .as_deref()
                        .map(str::trim)
                        .is_none_or(str::is_empty)
            })
            .filter(|task| {
                filters
                    .source_run_id
                    .as_deref()
                    .is_none_or(|value| task.source_run_id.as_deref() == Some(value))
            })
            .filter(|task| {
                filters
                    .source_session_id
                    .as_deref()
                    .is_none_or(|value| task.source_session_id.as_deref() == Some(value))
            })
            .filter(|task| {
                if filters.source_user_message_ids.is_empty() && filters.source_turn_ids.is_empty()
                {
                    return true;
                }
                task.source_user_message_id
                    .as_ref()
                    .is_some_and(|id| filters.source_user_message_ids.contains(id))
                    || task
                        .source_turn_id
                        .as_ref()
                        .is_some_and(|id| filters.source_turn_ids.contains(id))
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        apply_offset_limit(&mut items, filters.offset, filters.limit);
        items
    }

    pub(in crate::store) fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> PaginatedResponse<TaskRecord> {
        let mut count_filters = filters.clone();
        count_filters.limit = None;
        count_filters.offset = None;
        let total = self.list_tasks_filtered(&count_filters).len();
        build_page_response(
            self.list_tasks_filtered(filters),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    pub(in crate::store) fn get_task(&self, id: &str) -> Option<TaskRecord> {
        self.inner.read().tasks.get(id).cloned()
    }

    pub(in crate::store) fn list_task_summaries(&self) -> Vec<TaskSummaryRecord> {
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .map(TaskSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Vec<TaskSummaryRecord> {
        self.list_tasks_filtered(filters)
            .iter()
            .map(TaskSummaryRecord::from)
            .collect()
    }

    pub(in crate::store) fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Vec<TaskSummaryRecord> {
        let wanted = ids.iter().collect::<std::collections::HashSet<_>>();
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .filter(|task| wanted.contains(&task.id))
            .map(TaskSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn list_task_tags(&self) -> Vec<String> {
        let data = self.inner.read();
        let mut tags = data
            .tasks
            .values()
            .flat_map(|task| task.tags.iter().cloned())
            .collect::<Vec<_>>();
        tags.sort();
        tags.dedup();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TaskMcpConfig, TaskScheduleConfig, TaskToolState};
    use std::collections::BTreeSet;
    use tokio::sync::broadcast;

    fn test_store() -> InMemoryStore {
        let (run_event_sender, _) = broadcast::channel(8);
        InMemoryStore::new(run_event_sender)
    }

    fn task_record(id: &str, parent_task_id: Option<&str>) -> TaskRecord {
        TaskRecord {
            id: id.to_string(),
            title: id.to_string(),
            description: None,
            objective: format!("do {id}"),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: format!("task-{id}"),
            tenant_id: "tenant".to_string(),
            subject_id: "subject".to_string(),
            project_id: PUBLIC_PROJECT_ID.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: parent_task_id.map(ToOwned::to_owned),
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: TaskToolState::default(),
            mcp_config: TaskMcpConfig::default(),
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
            deleted_at: None,
        }
    }

    fn task_ids(tasks: Vec<TaskRecord>) -> BTreeSet<String> {
        tasks.into_iter().map(|task| task.id).collect()
    }

    #[test]
    fn include_subtasks_false_returns_only_top_level_tasks() {
        let store = test_store();
        store.save_task(task_record("root", None));
        store.save_task(task_record("child", Some("root")));

        let root_only = store.list_tasks_filtered(&TaskListFilters {
            include_subtasks: Some(false),
            ..TaskListFilters::default()
        });
        assert_eq!(task_ids(root_only), BTreeSet::from(["root".to_string()]));

        let child_tasks = store.list_tasks_filtered(&TaskListFilters {
            parent_task_id: Some("root".to_string()),
            include_subtasks: Some(false),
            ..TaskListFilters::default()
        });
        assert_eq!(task_ids(child_tasks), BTreeSet::from(["child".to_string()]));

        let all_tasks = store.list_tasks_filtered(&TaskListFilters::default());
        assert_eq!(
            task_ids(all_tasks),
            BTreeSet::from(["child".to_string(), "root".to_string()])
        );
    }
}
