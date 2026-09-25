// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
    pub async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks()),
            Self::Postgres(store) => store.list_tasks().await,
        }
    }

    pub async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks_filtered(filters)),
            Self::Postgres(store) => store.list_tasks_filtered(filters).await,
        }
    }

    pub async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks_page(filters)),
            Self::Postgres(store) => store.list_tasks_page(filters).await,
        }
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task(id)),
            Self::Postgres(store) => store.get_task(id).await,
        }
    }

    pub async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_summaries_filtered(filters)),
            Self::Postgres(store) => store.list_task_summaries_filtered(filters).await,
        }
    }

    pub async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task_summaries_by_ids(ids)),
            Self::Postgres(store) => store.get_task_summaries_by_ids(ids).await,
        }
    }

    pub async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_tags()),
            Self::Postgres(store) => store.list_task_tags().await,
        }
    }

    pub async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        match self {
            Self::InMemory(store) => Ok(store.task_stats()),
            Self::Postgres(store) => store.task_stats().await,
        }
    }

    pub async fn task_stats_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<TaskStatsResponse, String> {
        match self {
            Self::InMemory(store) => Ok(store.task_stats_filtered(filters)),
            Self::Postgres(store) => store.task_stats_filtered(filters).await,
        }
    }

    pub async fn claim_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => {
                let due = store.list_due_scheduled_tasks(now);
                let mut claimed = Vec::with_capacity(due.len().min(limit));
                for task in due.into_iter().take(limit) {
                    let Some(expected_next_run_at) = task.schedule.next_run_at.as_deref() else {
                        continue;
                    };
                    let schedule =
                        crate::services::advance_task_schedule_after_dispatch(&task.schedule, now)?;
                    if let Some(task) = store.update_task_schedule_if_next_run_at(
                        task.id.as_str(),
                        expected_next_run_at,
                        schedule,
                        now_rfc3339().as_str(),
                    ) {
                        claimed.push(task);
                    }
                }
                Ok(claimed)
            }
            Self::Postgres(store) => store.claim_due_scheduled_tasks(now, limit).await,
        }
    }

    pub async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_task(task)),
            Self::Postgres(store) => store.save_task(task).await,
        }
    }

    pub async fn update_tasks_batch(&self, tasks: &[TaskRecord]) -> Result<(), String> {
        match self {
            Self::InMemory(store) => store.update_tasks_batch(tasks),
            Self::Postgres(store) => store.update_tasks_batch(tasks).await,
        }
    }

    pub async fn save_task_and_set_prerequisites_if_revision(
        &self,
        task: TaskRecord,
        prerequisite_task_ids: Vec<String>,
        expected_revision: i64,
    ) -> Result<Option<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_task_and_set_prerequisites_if_revision(
                task,
                prerequisite_task_ids,
                expected_revision,
            )),
            Self::Postgres(store) => {
                store
                    .save_task_and_set_prerequisites_if_revision(
                        task,
                        prerequisite_task_ids,
                        expected_revision,
                    )
                    .await
            }
        }
    }

    pub async fn update_task_schedule_if_next_run_at(
        &self,
        task_id: &str,
        expected_next_run_at: &str,
        schedule: TaskScheduleConfig,
        updated_at: &str,
    ) -> Result<Option<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.update_task_schedule_if_next_run_at(
                task_id,
                expected_next_run_at,
                schedule,
                updated_at,
            )),
            Self::Postgres(store) => {
                store
                    .update_task_schedule_if_next_run_at(
                        task_id,
                        expected_next_run_at,
                        schedule,
                        updated_at,
                    )
                    .await
            }
        }
    }

    pub async fn list_task_prerequisites(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_prerequisites(task_id)),
            Self::Postgres(store) => store.list_task_prerequisites(task_id).await,
        }
    }

    pub async fn list_task_prerequisites_for_tasks(
        &self,
        task_ids: &[String],
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_prerequisites_for_tasks(task_ids)),
            Self::Postgres(store) => store.list_task_prerequisites_for_tasks(task_ids).await,
        }
    }

    pub async fn list_task_dependents(
        &self,
        prerequisite_task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_dependents(prerequisite_task_id)),
            Self::Postgres(store) => store.list_task_dependents(prerequisite_task_id).await,
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
            Self::Postgres(store) => {
                store
                    .set_task_prerequisites(task_id, prerequisite_task_ids)
                    .await
            }
        }
    }

    pub async fn dependency_graph_revision(&self) -> Result<i64, String> {
        match self {
            Self::InMemory(store) => Ok(store.dependency_graph_revision()),
            Self::Postgres(store) => store.dependency_graph_revision().await,
        }
    }

    pub async fn set_task_prerequisites_if_revision(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
        expected_revision: i64,
    ) -> Result<Option<Vec<TaskPrerequisiteRecord>>, String> {
        match self {
            Self::InMemory(store) => Ok(store.set_task_prerequisites_if_revision(
                task_id,
                prerequisite_task_ids,
                expected_revision,
            )),
            Self::Postgres(store) => {
                store
                    .set_task_prerequisites_if_revision(
                        task_id,
                        prerequisite_task_ids,
                        expected_revision,
                    )
                    .await
            }
        }
    }

    pub async fn delete_task(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_task(id)),
            Self::Postgres(store) => store.delete_task(id).await,
        }
    }
}
