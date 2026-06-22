use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{CreateTaskRequest, TaskMcpConfig};

use super::chatos_async_planner::{
    planner_prerequisite_create_request, planner_root_create_request,
    require_chatos_async_source_context,
};
use super::support::{ensure_client_ref_graph_acyclic, normalize_mcp_builtin_kind_names};
use super::{
    CreateTasksWithPrerequisitesArgs, McpRequestContext, McpToolProfile, TaskRunnerMcpService,
};

impl TaskRunnerMcpService {
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
                .await?;
            if !existing.is_empty() {
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
                self.require_task_for_user(prerequisite_task_id, current_user)
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
                let config = mcp_config.get_or_insert_with(TaskMcpConfig::default);
                config.enabled = true;
                config.enabled_builtin_kinds = normalized;
            }
            let is_prerequisite_node = prerequisite_ref_targets.contains(client_ref.as_str());
            let mut request = CreateTaskRequest {
                title: item.title,
                description: item.description,
                objective: item.objective,
                input_payload: item.input_payload,
                status: None,
                priority: item.priority,
                tags: item.tags,
                default_model_config_id: item.default_model_config_id,
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
                    planner_prerequisite_create_request(request)?
                } else {
                    planner_root_create_request(request)?
                };
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

        Ok(json!({
            "created_tasks": created_tasks,
            "dependency_edges": dependency_edges,
        }))
    }
}
