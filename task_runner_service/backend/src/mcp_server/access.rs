use crate::auth::CurrentUser;
use crate::models::{
    TaskListFilters, TaskRecord, TaskRunRecord, TaskScheduleMode, TaskStatsResponse, TaskStatus,
    UiPromptRecord,
};

use super::chatos_async_planner::require_chatos_async_source_context;
use super::support::ensure_task_owner;
use super::{McpRequestContext, McpToolProfile, TaskRunnerMcpService};

impl TaskRunnerMcpService {
    pub(super) async fn task_stats_for_user(
        &self,
        current_user: &CurrentUser,
    ) -> Result<TaskStatsResponse, String> {
        if current_user.is_admin() {
            return self.task_service.task_stats().await;
        }
        let tasks = self
            .task_service
            .list_tasks_filtered(TaskListFilters {
                creator_user_id: Some(current_user.id.clone()),
                ..TaskListFilters::default()
            })
            .await?;
        let mut stats = TaskStatsResponse {
            total: 0,
            scheduled: 0,
            follow_up: 0,
            draft: 0,
            ready: 0,
            queued: 0,
            running: 0,
            succeeded: 0,
            failed: 0,
            blocked: 0,
            cancelled: 0,
            archived: 0,
        };
        for task in tasks {
            stats.total += 1;
            if !matches!(task.schedule.mode, TaskScheduleMode::Manual) {
                stats.scheduled += 1;
            }
            if task.parent_task_id.is_some() {
                stats.follow_up += 1;
            }
            match task.status {
                TaskStatus::Draft => stats.draft += 1,
                TaskStatus::Ready => stats.ready += 1,
                TaskStatus::Queued => stats.queued += 1,
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Succeeded => stats.succeeded += 1,
                TaskStatus::Failed => stats.failed += 1,
                TaskStatus::Blocked => stats.blocked += 1,
                TaskStatus::Cancelled => stats.cancelled += 1,
                TaskStatus::Archived => stats.archived += 1,
            }
        }
        Ok(stats)
    }

    pub(super) async fn require_tasks_for_user(
        &self,
        task_ids: &[String],
        current_user: &CurrentUser,
    ) -> Result<(), String> {
        for task_id in task_ids {
            self.require_task_for_user(task_id.as_str(), current_user)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn first_existing_chatos_async_task(
        &self,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Option<TaskRecord>, String> {
        Ok(self
            .existing_chatos_async_tasks(current_user, request_context)
            .await?
            .into_iter()
            .next())
    }

    pub(super) async fn existing_chatos_async_tasks(
        &self,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Vec<TaskRecord>, String> {
        if request_context.tool_profile() != McpToolProfile::ChatosAsyncPlanner {
            return Ok(Vec::new());
        }
        let (source_session_id, source_user_message_id) =
            require_chatos_async_source_context(request_context)?;
        self.task_service
            .list_tasks_for_chatos_message(source_session_id, source_user_message_id)
            .await
            .map(|tasks| {
                tasks
                    .into_iter()
                    .filter(|task| {
                        task.creator_user_id.as_deref() == Some(current_user.id.as_str())
                    })
                    .collect()
            })
    }

    pub(super) async fn require_task_for_user(
        &self,
        task_id: &str,
        current_user: &CurrentUser,
    ) -> Result<TaskRecord, String> {
        let task = self
            .task_service
            .get_task(task_id)
            .await?
            .ok_or_else(|| format!("任务不存在: {task_id}"))?;
        ensure_task_owner(&task, current_user)?;
        Ok(task)
    }

    pub(super) async fn require_run_for_user(
        &self,
        run_id: &str,
        current_user: &CurrentUser,
    ) -> Result<TaskRunRecord, String> {
        let run = self
            .run_service
            .get_run(run_id)
            .await?
            .ok_or_else(|| format!("运行记录不存在: {run_id}"))?;
        self.require_task_for_user(run.task_id.as_str(), current_user)
            .await?;
        Ok(run)
    }

    pub(super) async fn require_prompt_for_user(
        &self,
        prompt: &UiPromptRecord,
        current_user: &CurrentUser,
    ) -> Result<(), String> {
        if current_user.is_admin() {
            return Ok(());
        }
        if let Some(task_id) = prompt.task_id.as_deref() {
            self.require_task_for_user(task_id, current_user).await?;
            return Ok(());
        }
        if let Some(run_id) = prompt.run_id.as_deref() {
            self.require_run_for_user(run_id, current_user).await?;
            return Ok(());
        }
        Err("当前 agent 无权访问该提示".to_string())
    }

    pub(super) async fn filter_runs_for_user(
        &self,
        runs: Vec<TaskRunRecord>,
        current_user: &CurrentUser,
    ) -> Result<Vec<TaskRunRecord>, String> {
        if current_user.is_admin() {
            return Ok(runs);
        }
        let mut out = Vec::new();
        for run in runs {
            if self
                .require_task_for_user(run.task_id.as_str(), current_user)
                .await
                .is_ok()
            {
                out.push(run);
            }
        }
        Ok(out)
    }

    pub(super) async fn filter_prompts_for_user(
        &self,
        prompts: Vec<UiPromptRecord>,
        current_user: &CurrentUser,
    ) -> Result<Vec<UiPromptRecord>, String> {
        if current_user.is_admin() {
            return Ok(prompts);
        }
        let mut out = Vec::new();
        for prompt in prompts {
            if self
                .require_prompt_for_user(&prompt, current_user)
                .await
                .is_ok()
            {
                out.push(prompt);
            }
        }
        Ok(out)
    }
}
