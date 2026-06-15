use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_runs(&self, task_id: Option<&str>) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| task_id.is_none_or(|value| run.task_id == value))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        items
    }

    pub(in crate::store) fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| {
                filters
                    .task_id
                    .as_deref()
                    .is_none_or(|value| run.task_id == value)
            })
            .filter(|run| filters.status.is_none_or(|value| run.status == value))
            .filter(|run| {
                filters
                    .model_config_id
                    .as_deref()
                    .is_none_or(|value| run.model_config_id == value)
            })
            .filter(|run| {
                filters.keyword.as_deref().is_none_or(|value| {
                    run.id.to_ascii_lowercase().contains(value)
                        || run.task_id.to_ascii_lowercase().contains(value)
                        || run.model_config_id.to_ascii_lowercase().contains(value)
                        || run
                            .result_summary
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(value)
                        || run
                            .error_message
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(value)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        apply_offset_limit(&mut items, filters.offset, filters.limit);
        items
    }

    pub(in crate::store) fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> PaginatedResponse<TaskRunRecord> {
        let mut count_filters = filters.clone();
        count_filters.limit = None;
        count_filters.offset = None;
        let total = self.list_runs_filtered(&count_filters).len();
        build_page_response(
            self.list_runs_filtered(filters),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    pub(in crate::store) fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Vec<RunSummaryRecord> {
        self.list_runs_filtered(filters)
            .iter()
            .map(RunSummaryRecord::from)
            .collect()
    }

    pub(in crate::store) fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Vec<RunSummaryRecord> {
        let wanted = ids.iter().collect::<std::collections::HashSet<_>>();
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| wanted.contains(&run.id))
            .map(RunSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn get_run(&self, id: &str) -> Option<TaskRunRecord> {
        self.inner.read().runs.get(id).cloned()
    }

    pub(in crate::store) fn save_run(&self, run: TaskRunRecord) -> TaskRunRecord {
        let mut data = self.inner.write();
        data.runs.insert(run.id.clone(), run.clone());
        run
    }

    pub(in crate::store) fn list_run_events(&self, run_id: &str) -> Vec<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::store) fn append_run_event(&self, event: TaskRunEventRecord) {
        let mut data = self.inner.write();
        data.run_events
            .entry(event.run_id.clone())
            .or_default()
            .push(event.clone());
        let _ = self.run_event_sender.send(event);
    }

    pub(in crate::store) fn mark_cancel_requested(&self, run_id: &str) -> Option<TaskRunRecord> {
        let mut data = self.inner.write();
        data.cancel_requested_runs.insert(run_id.to_string());
        let run = data.runs.get_mut(run_id)?;
        run.cancel_requested = true;
        Some(run.clone())
    }

    pub(in crate::store) fn clear_cancel_requested(&self, run_id: &str) {
        let mut data = self.inner.write();
        data.cancel_requested_runs.remove(run_id);
        if let Some(run) = data.runs.get_mut(run_id) {
            run.cancel_requested = false;
        }
    }

    pub(in crate::store) fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.inner.read().cancel_requested_runs.contains(run_id)
    }

    pub(in crate::store) fn has_active_run_for_task(&self, task_id: &str) -> bool {
        self.inner.read().runs.values().any(|run| {
            run.task_id == task_id
                && matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
        })
    }
}
