// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{
    BatchTaskDeleteRequest, BatchTaskStatusUpdateRequest, CreateTaskRequest, SkillRecord,
    TaskListFilters, TASK_PROFILE_CHATOS_PLAN,
};

use super::chatos_async_planner::{
    is_system_injected_builtin_kind, planner_root_create_request, planner_update_task_request,
    require_chatos_async_source_context,
};
use super::support::{
    ensure_task_status_update_allowed_from_mcp, external_mcp_configs_for_user,
    remove_internal_task_fields, task_creator_filter, task_for_external_mcp,
    tasks_for_external_mcp,
};
use super::{
    decode_args, text_result, BatchTaskDeleteArgs, BatchTaskStatusUpdateArgs, CancelTaskArgs,
    CreateTaskArgs, CreateTasksWithPrerequisitesArgs, McpRequestContext, McpToolProfile,
    SearchInstalledSkillsArgs, SetTaskPrerequisitesArgs, SkillIdArgs, TaskIdArgs,
    TaskRunnerMcpService, UpdateTaskArgs,
};

impl TaskRunnerMcpService {
    pub(super) async fn call_task_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        match name {
            "list_tasks" => {
                let args: CreateListTasksArgsAlias = decode_args(args)?;
                let tasks = self
                    .task_service
                    .list_tasks_filtered(TaskListFilters {
                        status: args.status,
                        keyword: args.keyword,
                        tag: args.tag,
                        model_config_id: args.model_config_id,
                        project_id: request_context.project_scope_id(),
                        task_profile: Some(request_context.requested_task_profile().to_string()),
                        creator_user_id: task_creator_filter(current_user)?,
                        scheduled_only: args.scheduled_only,
                        parent_task_id: args.parent_task_id,
                        source_run_id: args.source_run_id,
                        limit: args.limit,
                        offset: args.offset,
                        ..TaskListFilters::default()
                    })
                    .await?;
                Ok(text_result(tasks_for_external_mcp(tasks)))
            }
            "get_task" => {
                let args: TaskIdArgs = decode_args(args)?;
                let task = self
                    .require_task_for_user_in_context(
                        args.task_id.as_str(),
                        current_user,
                        request_context,
                    )
                    .await?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "get_task_stats" => {
                let _ = decode_args::<Value>(args).ok();
                let stats = self
                    .task_stats_for_user(current_user, request_context)
                    .await?;
                Ok(text_result(json!(stats)))
            }
            "create_task" => {
                let mut input: CreateTaskRequest =
                    decode_args::<CreateTaskArgs>(args)?.into_request()?;
                let source_context = request_context.task_source_context()?;
                if let Some(prerequisite_task_ids) = input.prerequisite_task_ids.as_ref() {
                    self.require_tasks_for_user_in_context(
                        prerequisite_task_ids.as_slice(),
                        current_user,
                        request_context,
                    )
                    .await?;
                }
                if request_context.tool_profile() == McpToolProfile::ChatosAsyncPlanner {
                    let _ = require_chatos_async_source_context(request_context)?;
                    if let Some(existing) = self
                        .first_existing_chatos_async_task(current_user, request_context)
                        .await?
                    {
                        self.dispatch_chatos_async_tasks(std::slice::from_ref(&existing))
                            .await?;
                        let task = self
                            .task_service
                            .get_task(existing.id.as_str())
                            .await?
                            .unwrap_or(existing);
                        return Ok(text_result(task_for_external_mcp(task)));
                    }
                    self.ensure_mcp_default_model_config(&mut input, current_user)
                        .await?;
                    input = planner_root_create_request(input, request_context)?;
                } else {
                    self.ensure_mcp_default_model_config(&mut input, current_user)
                        .await?;
                }
                if request_context.is_chatos_plan_task_profile() {
                    input.task_profile = Some(TASK_PROFILE_CHATOS_PLAN.to_string());
                }
                let task = self
                    .task_service
                    .create_task(input, Some(current_user), source_context)
                    .await?;
                let task = if request_context.tool_profile() == McpToolProfile::ChatosAsyncPlanner {
                    self.dispatch_chatos_async_tasks(std::slice::from_ref(&task))
                        .await?;
                    self.task_service
                        .get_task(task.id.as_str())
                        .await?
                        .unwrap_or(task)
                } else {
                    task
                };
                Ok(text_result(task_for_external_mcp(task)))
            }
            "list_mcp_builtin_catalog" => {
                let _ = decode_args::<Value>(args).ok();
                let mut catalog = self.mcp_catalog_service.list_catalog();
                if request_context.tool_profile() == McpToolProfile::ChatosAsyncPlanner {
                    catalog.retain(|item| !is_system_injected_builtin_kind(item.kind.as_str()));
                }
                Ok(text_result(json!(catalog)))
            }
            "list_external_mcp_configs" => {
                let _ = decode_args::<Value>(args).ok();
                let configs = self
                    .external_mcp_config_service
                    .list_external_mcp_configs()
                    .await?;
                Ok(text_result(json!(external_mcp_configs_for_user(
                    configs,
                    current_user
                ))))
            }
            "search_installed_skills" => {
                let args: SearchInstalledSkillsArgs = decode_args(args)?;
                let skills = self
                    .skill_service
                    .search_installed_skills_for_user(args.keyword, args.limit, current_user)
                    .await?;
                Ok(text_result(skills_for_external_mcp(skills)))
            }
            "get_skill_detail" => {
                let args: SkillIdArgs = decode_args(args)?;
                let skill = self
                    .skill_service
                    .get_skill_for_user(args.skill_id.as_str(), current_user)
                    .await?
                    .ok_or_else(|| format!("Skill 不存在或无权访问: {}", args.skill_id))?;
                Ok(text_result(skill_detail_for_external_mcp(skill)))
            }
            "create_tasks_with_prerequisites" => {
                let args: CreateTasksWithPrerequisitesArgs = decode_args(args)?;
                let result = self
                    .create_tasks_with_prerequisites(args, current_user, request_context)
                    .await?;
                Ok(text_result(result))
            }
            "update_task" => {
                let mut args: UpdateTaskArgs = decode_args(args)?;
                if args.patch.status.is_some() {
                    ensure_task_status_update_allowed_from_mcp(current_user)?;
                }
                if request_context.tool_profile() == McpToolProfile::ChatosAsyncPlanner {
                    args.patch = planner_update_task_request(args.patch)?;
                }
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let task = self
                    .task_service
                    .update_task(args.task_id.as_str(), args.patch, Some(current_user))
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "set_task_prerequisites" => {
                let args: SetTaskPrerequisitesArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let task = self
                    .task_service
                    .set_task_prerequisites(
                        args.task_id.as_str(),
                        args.prerequisite_task_ids,
                        Some(current_user),
                    )
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "cancel_task" => {
                let args: CancelTaskArgs = decode_args(args)?;
                let task_id = args.task_id.clone();
                self.require_task_for_user_in_context(
                    task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let result = self
                    .task_service
                    .cancel_task(task_id.as_str(), args.into_request(), Some(current_user))
                    .await?
                    .ok_or_else(|| format!("任务不存在: {task_id}"))?;
                Ok(text_result(json!(result)))
            }
            "wait_for_task_completion" => {
                let _ = decode_args::<Value>(args).ok();
                Ok(text_result(json!({
                    "accepted": true,
                    "mode": "background",
                    "message": "Task Runner accepted the arranged tasks for background execution.",
                    "message_zh": "任务系统已接收安排好的任务，并会进入后台执行流程。"
                })))
            }
            "get_task_dependency_graph" => {
                let args: TaskIdArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let graph = self
                    .task_service
                    .get_task_dependency_graph(args.task_id.as_str())
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                let mut value = json!(graph);
                remove_internal_task_fields(&mut value);
                Ok(text_result(value))
            }
            "delete_task" => {
                let args: TaskIdArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let deleted = self.task_service.delete_task(args.task_id.as_str()).await?;
                if !deleted {
                    return Err(format!("任务不存在: {}", args.task_id));
                }
                Ok(text_result(json!({
                    "deleted": true,
                    "task_id": args.task_id,
                })))
            }
            "batch_update_task_status" => {
                ensure_task_status_update_allowed_from_mcp(current_user)?;
                let args: BatchTaskStatusUpdateArgs = decode_args(args)?;
                self.require_tasks_for_user_in_context(
                    args.task_ids.as_slice(),
                    current_user,
                    request_context,
                )
                .await?;
                let result = self
                    .task_service
                    .batch_update_status(BatchTaskStatusUpdateRequest {
                        task_ids: args.task_ids,
                        status: args.status,
                    })
                    .await?;
                Ok(text_result(json!(result)))
            }
            "batch_delete_tasks" => {
                let args: BatchTaskDeleteArgs = decode_args(args)?;
                self.require_tasks_for_user_in_context(
                    args.task_ids.as_slice(),
                    current_user,
                    request_context,
                )
                .await?;
                let result = self
                    .task_service
                    .batch_delete_tasks(BatchTaskDeleteRequest {
                        task_ids: args.task_ids,
                    })
                    .await?;
                Ok(text_result(json!(result)))
            }
            other => Err(format!("unsupported task tool: {other}")),
        }
    }
}

type CreateListTasksArgsAlias = super::ListTasksArgs;

fn skills_for_external_mcp(skills: Vec<SkillRecord>) -> Value {
    Value::Array(
        skills
            .into_iter()
            .map(skill_summary_for_external_mcp)
            .collect(),
    )
}

fn skill_summary_for_external_mcp(skill: SkillRecord) -> Value {
    json!({
        "id": skill.id,
        "name": skill.name,
        "display_name": skill.display_name,
        "description": skill.description,
        "locale": skill.locale,
        "tags": skill.tags,
        "source": skill.source,
        "scope": skill.scope,
        "enabled": skill.enabled,
        "auto_inject": skill.auto_inject,
        "package_file_count": skill.package_file_count,
        "package_total_bytes": skill.package_total_bytes,
        "source_repo": skill.source_repo,
        "source_ref": skill.source_ref,
        "source_path": skill.source_path,
        "content_preview": preview_skill_content(skill.content.as_str()),
    })
}

fn skill_detail_for_external_mcp(skill: SkillRecord) -> Value {
    json!({
        "id": skill.id,
        "name": skill.name,
        "display_name": skill.display_name,
        "description": skill.description,
        "content": skill.content,
        "locale": skill.locale,
        "tags": skill.tags,
        "source": skill.source,
        "scope": skill.scope,
        "enabled": skill.enabled,
        "auto_inject": skill.auto_inject,
        "install_status": skill.install_status,
        "package_file_count": skill.package_file_count,
        "package_total_bytes": skill.package_total_bytes,
        "package_manifest": skill.package_manifest,
        "source_repo": skill.source_repo,
        "source_ref": skill.source_ref,
        "source_path": skill.source_path,
        "source_url": skill.source_url,
    })
}

fn preview_skill_content(content: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 1200;
    let content = content.trim();
    if content.chars().count() <= MAX_PREVIEW_CHARS {
        return content.to_string();
    }
    format!(
        "{}\n...",
        content.chars().take(MAX_PREVIEW_CHARS).collect::<String>()
    )
}
