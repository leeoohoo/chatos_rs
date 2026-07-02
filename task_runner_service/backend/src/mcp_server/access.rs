// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::auth::CurrentUser;
use crate::models::{
    normalize_project_id, AskUserPromptRecord, CreateTaskRequest, TaskListFilters, TaskRecord,
    TaskRunRecord, TaskScheduleMode, TaskStatsResponse, TaskStatus,
};

use super::chatos_async_planner::require_chatos_async_source_context;
use super::support::{
    effective_owner_user_id, enabled_model_configs_for_user, ensure_task_owner,
    model_visible_to_user, select_model_config_id_for_task,
};
use super::{McpRequestContext, McpToolProfile, TaskRunnerMcpService};

impl TaskRunnerMcpService {
    pub(super) async fn task_stats_for_user(
        &self,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<TaskStatsResponse, String> {
        let project_id = request_context.project_scope_id();
        if current_user.is_admin() && project_id.is_none() {
            return self.task_service.task_stats().await;
        }
        let tasks = self
            .task_service
            .list_tasks_filtered(TaskListFilters {
                project_id,
                task_profile: Some(request_context.requested_task_profile().to_string()),
                creator_user_id: if current_user.is_admin() {
                    None
                } else {
                    Some(effective_owner_user_id(current_user)?.to_string())
                },
                ..TaskListFilters::default()
            })
            .await?;
        Ok(task_stats_from_tasks(tasks))
    }

    pub(super) async fn require_task_for_user_in_context(
        &self,
        task_id: &str,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<TaskRecord, String> {
        let task = self.require_task_for_user(task_id, current_user).await?;
        ensure_task_project_scope(&task, request_context)?;
        ensure_task_profile_scope(&task, request_context)?;
        Ok(task)
    }

    pub(super) async fn require_tasks_for_user_in_context(
        &self,
        task_ids: &[String],
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<(), String> {
        for task_id in task_ids {
            self.require_task_for_user_in_context(task_id.as_str(), current_user, request_context)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn require_run_for_user_in_context(
        &self,
        run_id: &str,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<TaskRunRecord, String> {
        let run = self.require_run_for_user(run_id, current_user).await?;
        self.require_task_for_user_in_context(run.task_id.as_str(), current_user, request_context)
            .await?;
        Ok(run)
    }

    pub(super) async fn require_prompt_for_user_in_context(
        &self,
        prompt: &AskUserPromptRecord,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<(), String> {
        if let Some(task_id) = prompt.task_id.as_deref() {
            self.require_task_for_user_in_context(task_id, current_user, request_context)
                .await?;
            return Ok(());
        }
        if let Some(run_id) = prompt.run_id.as_deref() {
            self.require_run_for_user_in_context(run_id, current_user, request_context)
                .await?;
            return Ok(());
        }
        self.require_prompt_for_user(prompt, current_user).await
    }

    pub(super) async fn filter_runs_for_user_in_context(
        &self,
        runs: Vec<TaskRunRecord>,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Vec<TaskRunRecord>, String> {
        if current_user.is_admin() && request_context.project_scope_id().is_none() {
            return Ok(runs);
        }
        let mut out = Vec::new();
        for run in runs {
            if self
                .require_task_for_user_in_context(
                    run.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await
                .is_ok()
            {
                out.push(run);
            }
        }
        Ok(out)
    }

    pub(super) async fn filter_prompts_for_user_in_context(
        &self,
        prompts: Vec<AskUserPromptRecord>,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        if current_user.is_admin() && request_context.project_scope_id().is_none() {
            return Ok(prompts);
        }
        let mut out = Vec::new();
        for prompt in prompts {
            if self
                .require_prompt_for_user_in_context(&prompt, current_user, request_context)
                .await
                .is_ok()
            {
                out.push(prompt);
            }
        }
        Ok(out)
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
        let owner_user_id = effective_owner_user_id(current_user)?;
        self.task_service
            .list_tasks_filtered(TaskListFilters {
                source_session_id: Some(source_session_id.to_string()),
                source_user_message_ids: vec![source_user_message_id.to_string()],
                include_subtasks: Some(false),
                task_profile: Some(request_context.requested_task_profile().to_string()),
                ..TaskListFilters::default()
            })
            .await
            .map(|tasks| {
                tasks
                    .into_iter()
                    .filter(|task| {
                        task.owner_user_id
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .or_else(|| task.creator_user_id.as_deref())
                            == Some(owner_user_id)
                    })
                    .filter(|task| ensure_task_project_scope(task, request_context).is_ok())
                    .collect()
            })
    }

    pub(super) async fn dispatch_chatos_async_task_graph_roots(
        &self,
        task_ids: &[String],
    ) -> Result<Vec<TaskRunRecord>, String> {
        let mut tasks = Vec::new();
        for task_id in task_ids {
            if let Some(task) = self.task_service.get_task(task_id).await? {
                tasks.push(task);
            }
        }
        self.run_service
            .dispatch_ready_chatos_async_tasks(tasks.as_slice())
            .await
    }

    pub(super) async fn dispatch_chatos_async_tasks(
        &self,
        tasks: &[TaskRecord],
    ) -> Result<Vec<TaskRunRecord>, String> {
        self.run_service
            .dispatch_ready_chatos_async_tasks(tasks)
            .await
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
        prompt: &AskUserPromptRecord,
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

    pub(in crate::mcp_server) async fn ensure_mcp_default_model_config(
        &self,
        input: &mut CreateTaskRequest,
        current_user: &CurrentUser,
    ) -> Result<(), String> {
        if input
            .default_model_config_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            let model_config_id = input.default_model_config_id.as_deref().unwrap_or_default();
            let model = self
                .model_config_service
                .get_model_config(model_config_id)
                .await?
                .ok_or_else(|| format!("model config not found: {model_config_id}"))?;
            if !model.enabled {
                return Err(format!("model config is disabled: {model_config_id}"));
            }
            if !model_visible_to_user(&model, current_user) {
                return Err(format!("model config not found: {model_config_id}"));
            }
            return Ok(());
        }

        let models = self.model_config_service.list_model_configs().await?;
        let models = enabled_model_configs_for_user(models, current_user);
        let tags = input.tags.as_deref().unwrap_or(&[]);
        let Some(model_config_id) = select_model_config_id_for_task(
            models.as_slice(),
            input.title.as_str(),
            input.objective.as_str(),
            input.description.as_deref(),
            tags,
        ) else {
            return Err(
                "当前用户没有启用中的 Task 模型配置，请先在 Chatos 的 Task 模型设置里启用至少一个模型"
                    .to_string(),
            );
        };
        input.default_model_config_id = Some(model_config_id);
        Ok(())
    }
}

fn ensure_task_project_scope(
    task: &TaskRecord,
    request_context: &McpRequestContext,
) -> Result<(), String> {
    let Some(expected_project_id) = request_context.project_scope_id() else {
        return Ok(());
    };
    let actual_project_id = normalize_project_id(Some(task.project_id.clone()));
    if actual_project_id == expected_project_id {
        Ok(())
    } else {
        Err("任务不属于当前项目上下文".to_string())
    }
}

fn ensure_task_profile_scope(
    task: &TaskRecord,
    request_context: &McpRequestContext,
) -> Result<(), String> {
    if task
        .task_profile
        .eq_ignore_ascii_case(request_context.requested_task_profile())
    {
        Ok(())
    } else {
        Err("当前 agent 无权访问该任务".to_string())
    }
}

fn task_stats_from_tasks(tasks: Vec<TaskRecord>) -> TaskStatsResponse {
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
    stats
}
