use super::*;

impl TaskService {
    pub async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        self.hydrate_tasks_prerequisites(self.store.list_tasks().await?)
            .await
    }

    pub async fn list_tasks_filtered(
        &self,
        filters: TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        let filters = sanitize_task_list_filters(filters);
        self.hydrate_tasks_prerequisites(self.store.list_tasks_filtered(&filters).await?)
            .await
    }

    pub async fn list_tasks_page(
        &self,
        filters: TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let mut filters = sanitize_task_list_filters(filters);
        filters.limit = Some(filters.limit.unwrap_or(20));
        filters.offset = Some(filters.offset.unwrap_or(0));
        let mut page = self.store.list_tasks_page(&filters).await?;
        page.items = self.hydrate_tasks_prerequisites(page.items).await?;
        Ok(page)
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        match self.store.get_task(id).await? {
            Some(task) => self.hydrate_task_prerequisites(task).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        self.store.task_stats().await
    }

    pub async fn task_index(&self) -> Result<TaskIndexResponse, String> {
        Ok(TaskIndexResponse {
            tasks: self.store.list_task_summaries().await?,
            tags: self.store.list_task_tags().await?,
        })
    }

    pub async fn list_task_summaries_filtered(
        &self,
        filters: TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let filters = sanitize_task_list_filters(filters);
        self.store.list_task_summaries_filtered(&filters).await
    }

    pub async fn get_task_summaries_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let ids = sanitize_id_list(ids);
        self.store.get_task_summaries_by_ids(&ids).await
    }
}
