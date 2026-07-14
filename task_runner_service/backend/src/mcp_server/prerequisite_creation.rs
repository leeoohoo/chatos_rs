// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{
    now_rfc3339, CreateTaskRequest, TaskRunRecord, TaskScheduleConfig, TaskScheduleMode,
    TaskStatus, TASK_PROFILE_CHATOS_PLAN,
};
use crate::services::project_management_api_client;

use super::chatos_async_planner::{
    planner_prerequisite_create_request, planner_root_create_request,
    require_chatos_async_source_context,
};
use super::support::{
    ensure_client_ref_graph_acyclic, normalize_mcp_builtin_kind_names, reusable_chatos_async_task,
};
use super::{
    normalize_external_mcp_config_ids, normalize_skill_ids,
    task_mcp_config_for_explicit_tool_selection, CreateProjectExecutionTasksArgs,
    CreateTaskWithPrerequisitesItem, CreateTasksWithPrerequisitesArgs, McpRequestContext,
    McpToolProfile, TaskRunnerMcpService,
};

impl TaskRunnerMcpService {
    pub(super) async fn create_project_execution_tasks(
        &self,
        args: CreateProjectExecutionTasksArgs,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        let project_id = args.project_id.trim().to_string();
        if project_id.is_empty() {
            return Err("project_id 不能为空".to_string());
        }
        if request_context
            .project_scope_id()
            .as_deref()
            .is_some_and(|scope| scope != project_id)
        {
            return Err("project_id 与当前 MCP 项目上下文不一致".to_string());
        }
        let requirement_id = args.requirement_id.trim().to_string();
        if requirement_id.is_empty() {
            return Err("requirement_id 不能为空".to_string());
        }
        if args.tasks.is_empty() {
            return Err("tasks 不能为空".to_string());
        }

        let execution_group_id = args
            .execution_group_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| request_context.source_user_message_id.clone())
            .ok_or_else(|| "execution_group_id 或 source_user_message_id 是必需的".to_string())?;
        let source_session_id = request_context.source_session_id.clone();
        let source_user_message_id = request_context.source_user_message_id.clone();

        let mut project_task_by_ref = HashMap::new();
        let mut converted = Vec::new();
        for item in args.tasks {
            let client_ref = item.client_ref.trim().to_string();
            if client_ref.is_empty() {
                return Err("client_ref 不能为空".to_string());
            }
            let project_task_id = item.project_task_id.trim().to_string();
            if project_task_id.is_empty() {
                return Err(format!("project_task_id 不能为空: {client_ref}"));
            }
            if project_task_by_ref
                .insert(client_ref.clone(), project_task_id.clone())
                .is_some()
            {
                return Err(format!("client_ref 重复: {client_ref}"));
            }
            let input_payload = enrich_project_execution_payload(
                item.input_payload,
                &project_id,
                &requirement_id,
                &project_task_id,
                &execution_group_id,
            );
            converted.push(CreateTaskWithPrerequisitesItem {
                client_ref,
                title: item.title,
                description: item.description,
                objective: item.objective,
                input_payload: Some(input_payload),
                priority: item.priority,
                tags: item.tags,
                default_model_config_id: item.default_model_config_id,
                requires_execution: item.requires_execution,
                schedule: Some(TaskScheduleConfig {
                    mode: TaskScheduleMode::ContactAsync,
                    run_at: Some(now_rfc3339()),
                    ..TaskScheduleConfig::default()
                }),
                enabled_builtin_kinds: item.enabled_builtin_kinds,
                external_mcp_config_ids: item.external_mcp_config_ids,
                selected_skill_ids: item.selected_skill_ids,
                prerequisite_refs: item.prerequisite_refs,
                prerequisite_task_ids: item.prerequisite_task_ids,
            });
        }

        let result = self
            .create_tasks_with_prerequisites(
                CreateTasksWithPrerequisitesArgs { tasks: converted },
                current_user,
                request_context,
            )
            .await?;
        let created = result
            .get("created_tasks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let created_task_ids = created
            .iter()
            .filter_map(|task| task.get("task_id").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let auto_started_runs = self
            .dispatch_chatos_async_task_graph_roots(created_task_ids.as_slice())
            .await?;

        let mut task_links = Vec::new();
        for task in &created {
            let Some(client_ref) = task.get("client_ref").and_then(Value::as_str) else {
                continue;
            };
            let Some(task_runner_task_id) = task.get("task_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(project_task_id) = project_task_by_ref.get(client_ref) else {
                return Err(format!(
                    "created task missing project_task_id mapping: {client_ref}"
                ));
            };
            project_management_api_client::sync_work_item_task_runner_status(
                self.task_service.config(),
                project_task_id,
                &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
                    task_runner_task_id: task_runner_task_id.to_string(),
                    task_runner_run_id: auto_started_runs
                        .iter()
                        .find(|run| run.task_id == task_runner_task_id)
                        .map(|run| run.id.clone()),
                    task_runner_status: Some("queued".to_string()),
                    execution_group_id: Some(execution_group_id.clone()),
                    last_callback_event: Some("task.queued".to_string()),
                    last_callback_at: Some(now_rfc3339()),
                    last_error_message: None,
                    source_session_id: source_session_id.clone(),
                    source_user_message_id: source_user_message_id.clone(),
                },
            )
            .await?;
            task_links.push(json!({
                "project_task_id": project_task_id,
                "task_runner_task_id": task_runner_task_id,
                "execution_group_id": execution_group_id,
            }));
        }

        Ok(json!({
            "created_tasks": created,
            "dependency_edges": result.get("dependency_edges").cloned().unwrap_or_else(|| json!([])),
            "auto_started_runs": auto_started_runs_for_mcp(auto_started_runs),
            "task_links": task_links,
        }))
    }

    pub(super) async fn create_tasks_with_prerequisites(
        &self,
        args: CreateTasksWithPrerequisitesArgs,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        if request_context.tool_profile() == McpToolProfile::ChatosAsyncPlanner {
            let _ = require_chatos_async_source_context(request_context)?;
            let existing = self
                .existing_chatos_async_tasks(current_user, request_context)
                .await?
                .into_iter()
                .filter(reusable_chatos_async_task)
                .collect::<Vec<_>>();
            if !existing.is_empty() {
                let auto_started_runs = self
                    .dispatch_chatos_async_tasks(existing.as_slice())
                    .await?;
                return Ok(json!({
                    "idempotent_reused": true,
                    "created_tasks": existing.into_iter().map(|task| {
                        json!({
                            "task_id": task.id,
                            "title": task.title,
                            "status": task.status,
                        })
                    }).collect::<Vec<_>>(),
                    "dependency_edges": [],
                    "auto_started_runs": auto_started_runs_for_mcp(auto_started_runs),
                }));
            }
        }

        if args.tasks.is_empty() {
            return Err("tasks 不能为空".to_string());
        }
        if args.tasks.len() > 50 {
            return Err("一次最多创建 50 个任务".to_string());
        }

        let mut refs = HashSet::new();
        for task in &args.tasks {
            let client_ref = task.client_ref.trim();
            if client_ref.is_empty() {
                return Err("client_ref 不能为空".to_string());
            }
            if !refs.insert(client_ref.to_string()) {
                return Err(format!("client_ref 重复: {client_ref}"));
            }
        }

        for task in &args.tasks {
            for prerequisite_ref in &task.prerequisite_refs {
                let prerequisite_ref = prerequisite_ref.trim();
                if !refs.contains(prerequisite_ref) {
                    return Err(format!("未知 prerequisite_ref: {prerequisite_ref}"));
                }
                if prerequisite_ref == task.client_ref.trim() {
                    return Err(format!("任务不能依赖自身: {prerequisite_ref}"));
                }
            }
            for prerequisite_task_id in &task.prerequisite_task_ids {
                self.require_task_for_user_in_context(
                    prerequisite_task_id,
                    current_user,
                    request_context,
                )
                .await?;
            }
        }
        ensure_client_ref_graph_acyclic(&args.tasks)?;

        let mut ref_to_task_id = HashMap::new();
        let mut created_tasks = Vec::new();
        let mut pending_edges = Vec::<(String, Vec<String>, Vec<String>)>::new();

        let tool_profile = request_context.tool_profile();
        let prerequisite_ref_targets = args
            .tasks
            .iter()
            .flat_map(|item| {
                item.prerequisite_refs
                    .iter()
                    .map(|value| value.trim().to_string())
            })
            .collect::<HashSet<_>>();

        for item in args.tasks {
            let client_ref = item.client_ref.trim().to_string();
            let mut mcp_config = None;
            if let Some(enabled_builtin_kinds) = item.enabled_builtin_kinds {
                let normalized = normalize_mcp_builtin_kind_names(enabled_builtin_kinds)?;
                let config =
                    mcp_config.get_or_insert_with(task_mcp_config_for_explicit_tool_selection);
                config.enabled = true;
                config.enabled_builtin_kinds = normalized;
            }
            if let Some(external_mcp_config_ids) = item.external_mcp_config_ids {
                let config =
                    mcp_config.get_or_insert_with(task_mcp_config_for_explicit_tool_selection);
                config.enabled = true;
                config.external_mcp_config_ids =
                    normalize_external_mcp_config_ids(external_mcp_config_ids);
            }
            if let Some(selected_skill_ids) = item.selected_skill_ids {
                let config =
                    mcp_config.get_or_insert_with(task_mcp_config_for_explicit_tool_selection);
                config.enabled = true;
                config.selected_skill_ids = normalize_skill_ids(selected_skill_ids);
            }
            if let Some(requires_execution) = item.requires_execution {
                mcp_config
                    .get_or_insert_with(crate::models::TaskMcpConfig::default)
                    .requires_execution = requires_execution;
            }
            let is_prerequisite_node = prerequisite_ref_targets.contains(client_ref.as_str());
            let mut request = CreateTaskRequest {
                title: item.title,
                description: item.description,
                objective: item.objective,
                input_payload: item.input_payload,
                status: if tool_profile == McpToolProfile::ProjectRequirementExecutionPlanner {
                    Some(TaskStatus::Ready)
                } else {
                    None
                },
                priority: item.priority,
                tags: item.tags,
                default_model_config_id: item.default_model_config_id,
                project_id: request_context.project_scope_id(),
                task_profile: None,
                tenant_id: None,
                subject_id: None,
                schedule: item.schedule,
                mcp_config,
                prerequisite_task_ids: Some(item.prerequisite_task_ids.clone()),
            };
            self.ensure_mcp_default_model_config(&mut request, current_user)
                .await?;
            if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                request = if is_prerequisite_node {
                    planner_prerequisite_create_request(request, request_context)?
                } else {
                    planner_root_create_request(request, request_context)?
                };
            }
            if request_context.is_chatos_plan_task_profile() {
                request.task_profile = Some(TASK_PROFILE_CHATOS_PLAN.to_string());
            }
            let task = self
                .task_service
                .create_task(
                    request,
                    Some(current_user),
                    request_context.task_source_context()?,
                )
                .await?;
            ref_to_task_id.insert(client_ref.clone(), task.id.clone());
            pending_edges.push((
                task.id.clone(),
                item.prerequisite_refs,
                item.prerequisite_task_ids,
            ));
            created_tasks.push(json!({
                "client_ref": client_ref,
                "task_id": task.id,
                "title": task.title,
                "status": task.status,
            }));
        }

        let mut dependency_edges = Vec::new();
        for (task_id, prerequisite_refs, existing_prerequisite_ids) in pending_edges {
            let mut prerequisite_ids = existing_prerequisite_ids;
            for prerequisite_ref in prerequisite_refs {
                let Some(prerequisite_task_id) = ref_to_task_id.get(prerequisite_ref.trim()) else {
                    return Err(format!("未知 prerequisite_ref: {prerequisite_ref}"));
                };
                prerequisite_ids.push(prerequisite_task_id.clone());
            }
            let task = self
                .task_service
                .set_task_prerequisites(&task_id, prerequisite_ids, Some(current_user))
                .await?
                .ok_or_else(|| format!("任务不存在: {task_id}"))?;
            for prerequisite_task_id in task.prerequisite_task_ids {
                dependency_edges.push(json!({
                    "task_id": task.id,
                    "prerequisite_task_id": prerequisite_task_id,
                }));
            }
        }

        let auto_started_runs = if tool_profile == McpToolProfile::ChatosAsyncPlanner {
            let task_ids = ref_to_task_id.values().cloned().collect::<Vec<_>>();
            self.dispatch_chatos_async_task_graph_roots(task_ids.as_slice())
                .await?
        } else {
            Vec::new()
        };

        Ok(json!({
            "created_tasks": created_tasks,
            "dependency_edges": dependency_edges,
            "auto_started_runs": auto_started_runs_for_mcp(auto_started_runs),
        }))
    }
}

fn auto_started_runs_for_mcp(runs: Vec<TaskRunRecord>) -> Vec<Value> {
    runs.into_iter()
        .map(|run| {
            json!({
                "run_id": run.id,
                "task_id": run.task_id,
                "status": run.status,
            })
        })
        .collect()
}

fn enrich_project_execution_payload(
    input_payload: Option<Value>,
    project_id: &str,
    requirement_id: &str,
    project_task_id: &str,
    execution_group_id: &str,
) -> Value {
    let mut payload = match input_payload {
        Some(Value::Object(map)) => map,
        Some(value) => {
            let mut map = serde_json::Map::new();
            map.insert("input".to_string(), value);
            map
        }
        None => serde_json::Map::new(),
    };
    payload.insert(
        "source".to_string(),
        Value::String("chatos_project_requirement_execution".to_string()),
    );
    payload.insert(
        "project_id".to_string(),
        Value::String(project_id.to_string()),
    );
    payload.insert(
        "requirement_id".to_string(),
        Value::String(requirement_id.to_string()),
    );
    payload.insert(
        "project_task_id".to_string(),
        Value::String(project_task_id.to_string()),
    );
    payload.insert(
        "execution_group_id".to_string(),
        Value::String(execution_group_id.to_string()),
    );
    Value::Object(payload)
}
