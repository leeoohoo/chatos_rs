// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;
use std::pin::Pin;

use crate::models::TaskScheduleConfig;

use super::*;

impl RunService {
    pub(crate) async fn set_project_execution_paused(
        &self,
        tasks: &[TaskRecord],
        paused: bool,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let mut task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
        task_ids.sort();
        task_ids.dedup();
        let mut start_guards = Vec::with_capacity(task_ids.len());
        for task_id in &task_ids {
            start_guards.push(
                self.start_lock_for_task(task_id.as_str())
                    .lock_owned()
                    .await,
            );
        }
        self.store
            .set_tasks_execution_paused(task_ids.as_slice(), paused)
            .await?;
        self.store
            .set_queued_runs_dispatch_paused(task_ids.as_slice(), paused)
            .await?;
        drop(start_guards);
        if paused {
            return Ok(Vec::new());
        }
        let mut refreshed_tasks = Vec::with_capacity(task_ids.len());
        for task_id in &task_ids {
            if let Some(task) = self.store.get_task(task_id.as_str()).await? {
                refreshed_tasks.push(task);
            }
        }
        self.dispatch_ready_chatos_async_tasks(refreshed_tasks.as_slice())
            .await
    }

    pub(crate) fn dispatch_confirmed_project_execution_tasks<'a>(
        &'a self,
        tasks: &'a [TaskRecord],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TaskRunRecord>, String>> + Send + 'a>> {
        Box::pin(async move {
            let activated_at = now_rfc3339();
            let mut activated_tasks = Vec::with_capacity(tasks.len());
            for task in tasks {
                let mut task = self
                    .store
                    .get_task(task.id.as_str())
                    .await?
                    .ok_or_else(|| format!("task not found: {}", task.id))?;
                task.schedule = TaskScheduleConfig {
                    mode: TaskScheduleMode::ContactAsync,
                    run_at: Some(activated_at.clone()),
                    interval_seconds: None,
                    // The dedicated DAG dispatcher starts roots and unlocks
                    // dependants only after every prerequisite has succeeded.
                    // Keeping next_run_at empty prevents the global scheduler
                    // from bypassing that dependency gate.
                    next_run_at: None,
                    last_scheduled_at: task.schedule.last_scheduled_at.clone(),
                };
                task.updated_at = now_rfc3339();
                activated_tasks.push(self.store.save_task(task).await?);
            }
            self.dispatch_ready_chatos_async_tasks(activated_tasks.as_slice())
                .await
        })
    }

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
                if let Some(run) = self
                    .dispatch_ready_chatos_async_task(task.id.as_str())
                    .await?
                {
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
            self.dispatch_ready_chatos_async_tasks(tasks.as_slice())
                .await
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
                self.consume_chatos_async_schedule_slot_at(task_id, now)
                    .await?;
                Ok(Some(run))
            }
            Err(err) if is_chatos_async_active_run_conflict_error(err.as_str()) => {
                self.consume_chatos_async_schedule_slot(task_id).await?;
                Ok(None)
            }
            Err(err) => {
                self.mark_chatos_async_schedule_failed(task_id, &err)
                    .await?;
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
        task.schedule.mode == TaskScheduleMode::ContactAsync
            && task.status == TaskStatus::Ready
            && !task.task_tool_state.execution_paused
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
