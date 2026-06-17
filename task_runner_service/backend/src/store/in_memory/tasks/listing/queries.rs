use super::*;

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
                filters
                    .creator_user_id
                    .as_deref()
                    .is_none_or(|value| task.creator_user_id.as_deref() == Some(value))
            })
            .filter(|task| {
                !filters.scheduled_only.unwrap_or(false)
                    || !matches!(task.schedule.mode, TaskScheduleMode::Manual)
            })
            .filter(|task| {
                filters
                    .parent_task_id
                    .as_deref()
                    .is_none_or(|value| task.parent_task_id.as_deref() == Some(value))
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
