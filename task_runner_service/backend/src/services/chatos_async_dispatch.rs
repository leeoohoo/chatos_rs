// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;
use std::pin::Pin;

use super::*;

impl RunService {
    pub(crate) fn dispatch_ready_chatos_async_tasks<'a>(
        &'a self,
        tasks: &'a [TaskRecord],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TaskRunRecord>, String>> + Send + 'a>> {
        Box::pin(async move {
            let mut runs = Vec::new();
            for task in tasks {
                let task = self.hydrate_task_prerequisites(task.clone()).await?;
                if !self.should_dispatch_chatos_async_task(&task) {
                    continue;
                }
                if !self
                    .task_prerequisites_have_succeeded(&task.prerequisite_task_ids)
                    .await?
                {
                    self.consume_chatos_async_schedule_slot(task.id.as_str())
                        .await?;
                    continue;
                }
                if let Some(run) = self.dispatch_ready_chatos_async_task(task.id.as_str()).await? {
                    runs.push(run);
                }
            }
            Ok(runs)
        })
    }

    pub(crate) fn dispatch_ready_chatos_async_tasks_for_source_task<'a>(
        &'a self,
        task: &'a TaskRecord,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TaskRunRecord>, String>> + Send + 'a>> {
        Box::pin(async move {
            if task.schedule.mode != TaskScheduleMode::ContactAsync {
                return Ok(Vec::new());
            }
            let Some(source_session_id) = normalized_optional(task.source_session_id.clone())
            else {
                return Ok(Vec::new());
            };
            let source_user_message_id = normalized_optional(task.source_user_message_id.clone());
            let source_turn_id = normalized_optional(task.source_turn_id.clone());
            if source_user_message_id.is_none() && source_turn_id.is_none() {
                return Ok(Vec::new());
            }

            let tasks = self
                .store
                .list_tasks_filtered(&TaskListFilters {
                    project_id: Some(task.project_id.clone()),
                    source_session_id: Some(source_session_id),
                    source_user_message_ids: source_user_message_id.into_iter().collect(),
                    source_turn_ids: source_turn_id.into_iter().collect(),
                    task_profile: Some(task.task_profile.clone()),
                    include_subtasks: Some(false),
                    ..TaskListFilters::default()
                })
                .await?;
            self.dispatch_ready_chatos_async_tasks(tasks.as_slice()).await
        })
    }

    async fn dispatch_ready_chatos_async_task(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        if self.has_active_run_for_task(task_id).await? {
            self.consume_chatos_async_schedule_slot(task_id).await?;
            return Ok(None);
        }

        let now = Utc::now();
        match self
            .start_scheduled_run(task_id, StartTaskRunRequest::default())
            .await
        {
            Ok(run) => {
                self.consume_chatos_async_schedule_slot_at(task_id, now).await?;
                Ok(Some(run))
            }
            Err(err) if is_chatos_async_active_run_conflict_error(err.as_str()) => {
                self.consume_chatos_async_schedule_slot(task_id).await?;
                Ok(None)
            }
            Err(err) => {
                self.mark_chatos_async_schedule_failed(task_id, &err).await?;
                Err(err)
            }
        }
    }

    async fn hydrate_task_prerequisites(&self, mut task: TaskRecord) -> Result<TaskRecord, String> {
        task.prerequisite_task_ids = self
            .store
            .list_task_prerequisites(task.id.as_str())
            .await?
            .into_iter()
            .map(|item| item.prerequisite_task_id)
            .collect();
        Ok(task)
    }

    fn should_dispatch_chatos_async_task(&self, task: &TaskRecord) -> bool {
        task.schedule.mode == TaskScheduleMode::ContactAsync && task.status == TaskStatus::Ready
    }

    async fn task_prerequisites_have_succeeded(
        &self,
        prerequisite_task_ids: &[String],
    ) -> Result<bool, String> {
        for prerequisite_task_id in prerequisite_task_ids {
            let Some(task) = self.store.get_task(prerequisite_task_id).await? else {
                return Ok(false);
            };
            if task.status != TaskStatus::Succeeded {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn consume_chatos_async_schedule_slot(&self, task_id: &str) -> Result<(), String> {
        self.consume_chatos_async_schedule_slot_at(task_id, Utc::now())
            .await
    }

    async fn consume_chatos_async_schedule_slot_at(
        &self,
        task_id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<(), String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Ok(());
        };
        task.schedule = advance_task_schedule_after_dispatch(&task.schedule, started_at)?;
        task.updated_at = now_rfc3339();
        self.store.save_task(task).await?;
        Ok(())
    }

    async fn mark_chatos_async_schedule_failed(
        &self,
        task_id: &str,
        error: &str,
    ) -> Result<(), String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Ok(());
        };
        task.result_summary = normalized_optional(Some(format!("scheduler error: {error}")));
        task.updated_at = now_rfc3339();
        self.store.save_task(task).await?;
        Ok(())
    }
}

fn is_chatos_async_active_run_conflict_error(error: &str) -> bool {
    error.contains("active run already exists") || error.contains("已有正在执行")
}
