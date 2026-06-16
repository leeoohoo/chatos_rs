use super::*;

impl AppStore {
    pub async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks()),
            Self::Sqlite(store) => store.list_tasks().await,
            Self::Mongo(store) => store.list_tasks().await,
        }
    }

    pub async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks_filtered(filters)),
            Self::Sqlite(store) => store.list_tasks_filtered(filters).await,
            Self::Mongo(store) => store.list_tasks_filtered(filters).await,
        }
    }

    pub async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks_page(filters)),
            Self::Sqlite(store) => store.list_tasks_page(filters).await,
            Self::Mongo(store) => store.list_tasks_page(filters).await,
        }
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task(id)),
            Self::Sqlite(store) => store.get_task(id).await,
            Self::Mongo(store) => store.get_task(id).await,
        }
    }

    pub async fn list_task_summaries(&self) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_summaries()),
            Self::Sqlite(store) => store.list_task_summaries().await,
            Self::Mongo(store) => store.list_task_summaries().await,
        }
    }

    pub async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_summaries_filtered(filters)),
            Self::Sqlite(store) => store.list_task_summaries_filtered(filters).await,
            Self::Mongo(store) => store.list_task_summaries_filtered(filters).await,
        }
    }

    pub async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task_summaries_by_ids(ids)),
            Self::Sqlite(store) => store.get_task_summaries_by_ids(ids).await,
            Self::Mongo(store) => store.get_task_summaries_by_ids(ids).await,
        }
    }

    pub async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_tags()),
            Self::Sqlite(store) => store.list_task_tags().await,
            Self::Mongo(store) => store.list_task_tags().await,
        }
    }

    pub async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        match self {
            Self::InMemory(store) => Ok(store.task_stats()),
            Self::Sqlite(store) => store.task_stats().await,
            Self::Mongo(store) => store.task_stats().await,
        }
    }

    pub async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_due_scheduled_tasks(now)),
            Self::Sqlite(store) => store.list_due_scheduled_tasks(now).await,
            Self::Mongo(store) => store.list_due_scheduled_tasks(now).await,
        }
    }

    pub async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_task(task)),
            Self::Sqlite(store) => store.save_task(task).await,
            Self::Mongo(store) => store.save_task(task).await,
        }
    }

    pub async fn list_task_prerequisites(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_prerequisites(task_id)),
            Self::Sqlite(store) => store.list_task_prerequisites(task_id).await,
            Self::Mongo(store) => store.list_task_prerequisites(task_id).await,
        }
    }

    pub async fn list_task_dependents(
        &self,
        prerequisite_task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_dependents(prerequisite_task_id)),
            Self::Sqlite(store) => store.list_task_dependents(prerequisite_task_id).await,
            Self::Mongo(store) => store.list_task_dependents(prerequisite_task_id).await,
        }
    }

    pub async fn set_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.set_task_prerequisites(task_id, prerequisite_task_ids))
            }
            Self::Sqlite(store) => {
                store
                    .set_task_prerequisites(task_id, prerequisite_task_ids)
                    .await
            }
            Self::Mongo(store) => {
                store
                    .set_task_prerequisites(task_id, prerequisite_task_ids)
                    .await
            }
        }
    }

    pub async fn delete_task(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_task(id)),
            Self::Sqlite(store) => store.delete_task(id).await,
            Self::Mongo(store) => store.delete_task(id).await,
        }
    }
}
