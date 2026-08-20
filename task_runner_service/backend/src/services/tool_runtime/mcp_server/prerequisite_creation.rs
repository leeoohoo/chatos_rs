// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{now_rfc3339, TaskRunRecord, TaskScheduleConfig, TaskStatus};
use crate::services::project_management_api_client;

use super::chatos_async_planner::{
    planner_prerequisite_create_request, planner_root_create_request,
    require_chatos_async_source_context,
};
use super::support::{ensure_client_ref_graph_acyclic, reusable_chatos_async_task};
use super::{
    CreateProjectExecutionTaskItem, CreateProjectExecutionTasksArgs, CreateTaskArgs,
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

        let project_task_ids_by_ref = chatos_project_execution::build_project_task_scope_refs(
            request_context
                .expected_project_task_ids
                .iter()
                .map(String::as_str),
        )
        .into_iter()
        .map(|(project_task_id, project_task_ref)| (project_task_ref, project_task_id))
        .collect::<BTreeMap<_, _>>();
        let submitted_project_task_ids = args
            .tasks
            .iter()
            .map(|item| item.project_task_ref.trim())
            .filter(|value| !value.is_empty())
            .map(|project_task_ref| {
                project_task_ids_by_ref
                    .get(project_task_ref)
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "未知 project_task_ref: {project_task_ref}；必须使用 selected_project_tasks 中的程序引用"
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        validate_project_execution_scope(
            &request_context.expected_project_task_ids,
            &submitted_project_task_ids,
        )?;
        validate_parallel_owned_paths(args.tasks.as_slice())?;
        validate_project_execution_task_contracts(args.tasks.as_slice())?;

        let execution_group_id = request_context
            .source_user_message_id
            .clone()
            .ok_or_else(|| "Chatos source_user_message_id 是必需的".to_string())?;
        let source_session_id = request_context.source_session_id.clone();
        let source_user_message_id = request_context.source_user_message_id.clone();

        let existing = self
            .existing_chatos_async_tasks(current_user, request_context)
            .await?
            .into_iter()
            .filter(reusable_chatos_async_task)
            .collect::<Vec<_>>();
        if !existing.is_empty() {
            let existing_project_task_ids = existing
                .iter()
                .filter_map(|task| {
                    task.input_payload
                        .as_ref()
                        .and_then(|payload| payload.get("project_task_id"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .collect::<std::collections::BTreeSet<_>>();
            validate_project_execution_scope(
                &request_context.expected_project_task_ids,
                &existing_project_task_ids,
            )?;
            return Ok(json!({
                "awaiting_confirmation": true,
                "idempotent_reused": true,
                "created_tasks": existing.iter().map(|task| {
                    json!({
                        "task_id": task.id,
                        "title": task.title,
                        "status": task.status,
                    })
                }).collect::<Vec<_>>(),
                "dependency_edges": existing.iter().flat_map(|task| {
                    task.prerequisite_task_ids.iter().map(|prerequisite_task_id| json!({
                        "task_id": task.id,
                        "prerequisite_task_id": prerequisite_task_id,
                    }))
                }).collect::<Vec<_>>(),
                "auto_started_runs": [],
                "task_links": existing.iter().filter_map(|task| {
                    let project_task_id = task.input_payload
                        .as_ref()
                        .and_then(|payload| payload.get("project_task_id"))
                        .and_then(Value::as_str)?;
                    Some(json!({
                        "project_task_id": project_task_id,
                        "task_runner_task_id": task.id,
                        "execution_group_id": execution_group_id,
                    }))
                }).collect::<Vec<_>>(),
            }));
        }

        let mut project_task_by_ref = HashMap::new();
        let mut converted = Vec::new();
        for item in args.tasks {
            let client_ref = item.client_ref.trim().to_string();
            if client_ref.is_empty() {
                return Err("client_ref 不能为空".to_string());
            }
            let project_task_ref = item.project_task_ref.trim();
            let project_task_id = project_task_ids_by_ref
                .get(project_task_ref)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "未知 project_task_ref: {project_task_ref}；必须使用 selected_project_tasks 中的程序引用"
                    )
                })?;
            if project_task_by_ref
                .insert(client_ref.clone(), project_task_id.clone())
                .is_some()
            {
                return Err(format!("client_ref 重复: {client_ref}"));
            }
            let owned_paths = item
                .owned_paths
                .iter()
                .map(|path| normalize_owned_path(path))
                .collect::<Result<Vec<_>, _>>()?;
            let acceptance_criteria =
                normalize_acceptance_criteria(item.acceptance_criteria.iter().map(String::as_str))?;
            let task_role = item.task_role.trim().to_ascii_lowercase();
            let input_payload = enrich_project_execution_payload(
                item.input_payload,
                &project_id,
                &requirement_id,
                &project_task_id,
                &execution_group_id,
                owned_paths.as_slice(),
                acceptance_criteria.as_slice(),
                task_role.as_str(),
            );
            converted.push(CreateTaskWithPrerequisitesItem {
                client_ref,
                task: CreateTaskArgs {
                    title: item.title,
                    description: item.description,
                    objective: item.objective,
                    input_payload: Some(input_payload),
                    priority: item.priority,
                    tags: item.tags,
                    default_model_config_id: item.default_model_config_id,
                    // Workspace capabilities are selected explicitly by the planning Agent.
                    // Task Runner derives the runtime workspace route from that frozen tool
                    // snapshot; it must not turn every project task into a full execution task.
                    requires_execution: item.requires_execution,
                    // Requirement planning only materializes a deferred DAG. A due
                    // ContactAsync schedule would let the global scheduler start it
                    // before Chatos receives explicit user confirmation.
                    schedule: Some(TaskScheduleConfig::default()),
                    enabled_builtin_kinds: item.enabled_builtin_kinds,
                    external_mcp_config_ids: item.external_mcp_config_ids,
                    selected_plugins: None,
                    prerequisite_task_ids: Some(item.prerequisite_task_ids),
                    mcp_config: None,
                },
                prerequisite_refs: item.prerequisite_refs,
                context_refs: item.context_refs,
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
                    task_runner_run_id: None,
                    task_runner_status: Some("ready".to_string()),
                    execution_group_id: Some(execution_group_id.clone()),
                    last_callback_event: Some("task.planned".to_string()),
                    last_callback_at: Some(now_rfc3339()),
                    last_error_message: None,
                    source_session_id: source_session_id.clone(),
                    source_user_message_id: source_user_message_id.clone(),
                    supersedes_task_runner_task_ids: Vec::new(),
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
            "awaiting_confirmation": true,
            "created_tasks": created,
            "dependency_edges": result.get("dependency_edges").cloned().unwrap_or_else(|| json!([])),
            "removed_redundant_edges": result.get("removed_redundant_edges").cloned().unwrap_or_else(|| json!([])),
            "dependency_diagnostics": result.get("dependency_diagnostics").cloned().unwrap_or_else(|| json!({})),
            "auto_started_runs": [],
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

        let mut tasks = args.tasks;
        let mut refs = HashSet::new();
        for task in &tasks {
            let client_ref = task.client_ref.trim();
            if client_ref.is_empty() {
                return Err("client_ref 不能为空".to_string());
            }
            if !refs.insert(client_ref.to_string()) {
                return Err(format!("client_ref 重复: {client_ref}"));
            }
        }

        for task in &tasks {
            for prerequisite_ref in &task.prerequisite_refs {
                let prerequisite_ref = prerequisite_ref.trim();
                if !refs.contains(prerequisite_ref) {
                    return Err(format!("未知 prerequisite_ref: {prerequisite_ref}"));
                }
                if prerequisite_ref == task.client_ref.trim() {
                    return Err(format!("任务不能依赖自身: {prerequisite_ref}"));
                }
            }
            for context_ref in &task.context_refs {
                let context_ref = context_ref.trim();
                if !refs.contains(context_ref) {
                    return Err(format!("未知 context_ref: {context_ref}"));
                }
                if context_ref == task.client_ref.trim() {
                    return Err(format!("任务不能把自身作为上下文: {context_ref}"));
                }
            }
            for prerequisite_task_id in task
                .task
                .prerequisite_task_ids
                .as_deref()
                .unwrap_or_default()
            {
                self.require_task_for_user_in_context(
                    prerequisite_task_id,
                    current_user,
                    request_context,
                )
                .await?;
            }
        }
        ensure_client_ref_graph_acyclic(&tasks)?;
        let dependency_diagnostics = reduce_client_ref_dependencies(tasks.as_mut_slice())?;

        let mut ref_to_task_id = HashMap::new();
        let mut created_tasks = Vec::new();
        let mut pending_edges = Vec::<(String, Vec<String>, Vec<String>)>::new();

        let tool_profile = request_context.tool_profile();
        let prerequisite_ref_targets = tasks
            .iter()
            .flat_map(|item| {
                item.prerequisite_refs
                    .iter()
                    .map(|value| value.trim().to_string())
            })
            .collect::<HashSet<_>>();

        for item in tasks {
            let CreateTaskWithPrerequisitesItem {
                client_ref,
                task,
                prerequisite_refs,
                context_refs,
            } = item;
            let client_ref = client_ref.trim().to_string();
            let is_prerequisite_node = prerequisite_ref_targets.contains(client_ref.as_str());
            let mut request = task.into_request()?;
            if tool_profile == McpToolProfile::ProjectRequirementExecutionPlanner {
                let workspace_write_selected = request.mcp_config.as_ref().is_some_and(|config| {
                    config.enabled_builtin_kinds.iter().any(|kind| {
                        chatos_mcp_runtime::builtin_kind_by_any(kind)
                            == Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
                    })
                });
                if let Some(config) = request.mcp_config.as_mut() {
                    // The planner selects capabilities. The program derives the
                    // workspace mutation contract from that frozen selection so
                    // the legacy default `true` cannot contradict an empty/read-only
                    // tool snapshot.
                    config.workspace_changes_required = Some(workspace_write_selected);
                }
            }
            attach_dependency_context_payload(
                &mut request.input_payload,
                client_ref.as_str(),
                context_refs.as_slice(),
            );
            request_context.enforce_plugin_config(&mut request);
            request_context.enforce_created_task_kind(&mut request);
            request.status = if tool_profile == McpToolProfile::ProjectRequirementExecutionPlanner {
                Some(TaskStatus::Ready)
            } else {
                None
            };
            request.project_id = request_context.project_scope_id();
            let prerequisite_task_ids = request.prerequisite_task_ids.clone().unwrap_or_default();
            if matches!(
                tool_profile,
                McpToolProfile::ChatosAsyncPlanner
                    | McpToolProfile::ProjectRequirementExecutionPlanner
            ) {
                if let Some(default_model_config_id) = request_context
                    .default_model_config_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    request.default_model_config_id = Some(default_model_config_id.to_string());
                }
            }
            self.ensure_mcp_default_model_config(&mut request, current_user)
                .await?;
            if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                request = if is_prerequisite_node {
                    planner_prerequisite_create_request(request, request_context)?
                } else {
                    planner_root_create_request(request, request_context)?
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
            pending_edges.push((task.id.clone(), prerequisite_refs, prerequisite_task_ids));
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
            "removed_redundant_edges": dependency_diagnostics.removed_edges,
            "dependency_diagnostics": {
                "submitted_edge_count": dependency_diagnostics.submitted_edge_count,
                "persisted_edge_count": dependency_diagnostics.persisted_edge_count,
            },
            "auto_started_runs": auto_started_runs_for_mcp(auto_started_runs),
        }))
    }
}

#[derive(Debug)]
struct ClientRefDependencyDiagnostics {
    submitted_edge_count: usize,
    persisted_edge_count: usize,
    removed_edges: Vec<chatos_project_execution::DependencyEdge>,
}

fn reduce_client_ref_dependencies(
    tasks: &mut [CreateTaskWithPrerequisitesItem],
) -> Result<ClientRefDependencyDiagnostics, String> {
    let node_ids = tasks
        .iter()
        .map(|task| task.client_ref.trim().to_string())
        .collect::<BTreeSet<_>>();
    let dependency_map = tasks
        .iter()
        .map(|task| {
            (
                task.client_ref.trim().to_string(),
                task.prerequisite_refs.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let submitted_edge_count = dependency_map
        .values()
        .map(|dependencies| {
            dependencies
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect::<BTreeSet<_>>()
                .len()
        })
        .sum();
    let reduction =
        chatos_project_execution::transitive_reduce_prerequisite_map(&node_ids, &dependency_map)?;
    let mut removed_by_dependent = BTreeMap::<String, Vec<String>>::new();
    for edge in &reduction.removed_edges {
        removed_by_dependent
            .entry(edge.dependent_id.clone())
            .or_default()
            .push(edge.prerequisite_id.clone());
    }
    for task in tasks {
        let client_ref = task.client_ref.trim();
        task.prerequisite_refs = reduction
            .dependencies
            .get(client_ref)
            .cloned()
            .unwrap_or_default();
        task.context_refs
            .extend(removed_by_dependent.remove(client_ref).unwrap_or_default());
        task.context_refs = task
            .context_refs
            .drain(..)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != client_ref)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    let persisted_edge_count = reduction.dependencies.values().map(Vec::len).sum();
    Ok(ClientRefDependencyDiagnostics {
        submitted_edge_count,
        persisted_edge_count,
        removed_edges: reduction.removed_edges,
    })
}

fn attach_dependency_context_payload(
    input_payload: &mut Option<Value>,
    client_ref: &str,
    context_refs: &[String],
) {
    let mut payload = match input_payload.take() {
        Some(Value::Object(map)) => map,
        Some(value) => {
            let mut map = serde_json::Map::new();
            map.insert("input".to_string(), value);
            map
        }
        None => serde_json::Map::new(),
    };
    payload.insert(
        "execution_client_ref".to_string(),
        Value::String(client_ref.to_string()),
    );
    payload.insert(
        "dependency_context_refs".to_string(),
        Value::Array(context_refs.iter().cloned().map(Value::String).collect()),
    );
    *input_payload = Some(Value::Object(payload));
}

fn validate_project_execution_scope(
    expected: &std::collections::BTreeSet<String>,
    submitted: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if expected.is_empty() {
        return Err("缺少 Chatos 下发的项目任务执行范围，拒绝创建云端执行任务".to_string());
    }
    chatos_project_execution::validate_exact_project_task_scope(expected, submitted).map_err(
        |mismatch| {
            format!(
                "提交的项目任务与 Chatos 选定执行范围不一致；缺少=[{}]，越界=[{}]",
                mismatch.missing.join(","),
                mismatch.unexpected.join(",")
            )
        },
    )
}

fn validate_parallel_owned_paths(items: &[CreateProjectExecutionTaskItem]) -> Result<(), String> {
    let mut paths_by_ref = BTreeMap::<String, Vec<String>>::new();
    let mut prerequisites_by_ref = BTreeMap::<String, Vec<String>>::new();
    for item in items {
        let client_ref = item.client_ref.trim();
        if client_ref.is_empty() {
            return Err("client_ref 不能为空".to_string());
        }
        if paths_by_ref.contains_key(client_ref) {
            return Err(format!("client_ref 重复: {client_ref}"));
        }
        let mut owned_paths = item
            .owned_paths
            .iter()
            .map(|path| normalize_owned_path(path))
            .collect::<Result<Vec<_>, _>>()?;
        owned_paths.sort();
        owned_paths.dedup();
        let workspace_write_selected = item.enabled_builtin_kinds.as_ref().is_some_and(|kinds| {
            kinds.iter().any(|kind| {
                chatos_mcp_runtime::builtin_kind_by_any(kind)
                    == Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
            })
        });
        if workspace_write_selected && owned_paths.is_empty() {
            return Err(format!(
                "执行任务 {client_ref} 启用了 CodeMaintainerWrite，但没有声明 owned_paths"
            ));
        }
        paths_by_ref.insert(client_ref.to_string(), owned_paths);
        prerequisites_by_ref.insert(
            client_ref.to_string(),
            item.prerequisite_refs
                .iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        );
    }
    for (client_ref, prerequisites) in &prerequisites_by_ref {
        if let Some(unknown) = prerequisites
            .iter()
            .find(|prerequisite| !paths_by_ref.contains_key(*prerequisite))
        {
            return Err(format!(
                "执行任务 {client_ref} 引用了未知 prerequisite_ref: {unknown}"
            ));
        }
    }

    let refs = paths_by_ref.keys().cloned().collect::<Vec<_>>();
    for (left_index, left_ref) in refs.iter().enumerate() {
        for right_ref in refs.iter().skip(left_index + 1) {
            if dependency_path_exists(left_ref, right_ref, &prerequisites_by_ref)
                || dependency_path_exists(right_ref, left_ref, &prerequisites_by_ref)
            {
                continue;
            }
            for left_path in &paths_by_ref[left_ref] {
                for right_path in &paths_by_ref[right_ref] {
                    if owned_paths_overlap(left_path, right_path) {
                        return Err(format!(
                            "并行执行任务的文件所有权冲突: {left_ref} owns `{left_path}`, {right_ref} owns `{right_path}`；请重新划分 owned_paths 或增加 prerequisite_refs"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_project_execution_task_contracts(
    items: &[CreateProjectExecutionTaskItem],
) -> Result<(), String> {
    for item in items {
        let client_ref = item.client_ref.trim();
        normalize_acceptance_criteria(item.acceptance_criteria.iter().map(String::as_str))?;
        let task_role = item.task_role.trim().to_ascii_lowercase();
        if !matches!(task_role.as_str(), "implementation" | "verification") {
            return Err(format!(
                "执行任务 {client_ref} 的 task_role 必须是 implementation 或 verification"
            ));
        }
        let write_selected = project_execution_item_selects_kind(
            item,
            chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite,
        );
        let terminal_selected = project_execution_item_selects_kind(
            item,
            chatos_mcp_runtime::BuiltinMcpKind::TerminalController,
        );
        if task_role == "implementation" {
            if !write_selected {
                return Err(format!(
                    "实施任务 {client_ref} 必须从 Plugin Management 能力清单中选择 CodeMaintainerWrite"
                ));
            }
            if item.owned_paths.is_empty() {
                return Err(format!("实施任务 {client_ref} 必须声明非空 owned_paths"));
            }
        }
        if task_role == "verification" {
            if !item.owned_paths.is_empty() {
                return Err(format!(
                    "验收任务 {client_ref} 必须保持只读，owned_paths 必须为空"
                ));
            }
            if write_selected {
                return Err(format!(
                    "验收任务 {client_ref} 不能启用 CodeMaintainerWrite；发现缺陷后必须进入 repair/reverify 流程"
                ));
            }
            if item
                .enabled_builtin_kinds
                .as_ref()
                .is_none_or(|kinds| kinds.is_empty())
                && item
                    .external_mcp_config_ids
                    .as_ref()
                    .is_none_or(|ids| ids.is_empty())
            {
                return Err(format!(
                    "验收任务 {client_ref} 必须明确选择至少一个只读、终端、浏览器或外部 MCP 能力"
                ));
            }
        }
        if (write_selected || terminal_selected) && item.requires_execution != Some(true) {
            return Err(format!(
                "执行任务 {client_ref} 选择了写入或终端能力时 requires_execution 必须为 true"
            ));
        }
    }
    Ok(())
}

fn project_execution_item_selects_kind(
    item: &CreateProjectExecutionTaskItem,
    expected: chatos_mcp_runtime::BuiltinMcpKind,
) -> bool {
    item.enabled_builtin_kinds.as_ref().is_some_and(|kinds| {
        kinds
            .iter()
            .any(|kind| chatos_mcp_runtime::builtin_kind_by_any(kind) == Some(expected))
    })
}

fn normalize_acceptance_criteria<'a>(
    criteria: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    let mut unique = BTreeSet::new();
    for criterion in criteria {
        let criterion = criterion.trim();
        if criterion.is_empty() {
            return Err("acceptance_criteria 不能包含空字符串".to_string());
        }
        if !unique.insert(criterion.to_string()) {
            return Err(format!("acceptance_criteria 重复: {criterion}"));
        }
        normalized.push(criterion.to_string());
    }
    if normalized.is_empty() {
        return Err("acceptance_criteria 不能为空".to_string());
    }
    Ok(normalized)
}

fn normalize_owned_path(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') {
        return Err(format!("owned_paths 必须是非空仓库相对路径: `{value}`"));
    }
    let components = normalized
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| matches!(*component, "." | ".."))
    {
        return Err(format!("owned_paths 包含非法路径段: `{value}`"));
    }
    Ok(components.join("/"))
}

fn owned_paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn dependency_path_exists(
    task_ref: &str,
    prerequisite_ref: &str,
    prerequisites_by_ref: &BTreeMap<String, Vec<String>>,
) -> bool {
    let mut pending = vec![task_ref];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        for prerequisite in prerequisites_by_ref.get(current).into_iter().flatten() {
            if prerequisite == prerequisite_ref {
                return true;
            }
            pending.push(prerequisite.as_str());
        }
    }
    false
}

#[cfg(test)]
mod project_execution_scope_tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::{
        validate_parallel_owned_paths, validate_project_execution_scope,
        validate_project_execution_task_contracts, CreateProjectExecutionTaskItem,
    };

    #[test]
    fn exact_project_task_scope_is_required() {
        let expected = BTreeSet::from(["task-a".to_string(), "task-b".to_string()]);
        validate_project_execution_scope(&expected, &expected)
            .expect("exact selected scope should be accepted");

        let missing =
            validate_project_execution_scope(&expected, &BTreeSet::from(["task-a".to_string()]))
                .expect_err("partial coverage must be rejected before creating project tasks");
        assert!(missing.contains("缺少=[task-b]"));

        let extra = validate_project_execution_scope(
            &expected,
            &BTreeSet::from([
                "task-a".to_string(),
                "task-b".to_string(),
                "task-outside".to_string(),
            ]),
        )
        .expect_err("out-of-scope project tasks must be rejected");
        assert!(extra.contains("越界=[task-outside]"));
    }

    #[test]
    fn parallel_execution_tasks_reject_overlapping_file_and_directory_ownership() {
        let items: Vec<CreateProjectExecutionTaskItem> = serde_json::from_value(json!([
            {
                "client_ref": "ui",
                "project_task_ref": "pt_1",
                "title": "UI",
                "objective": "UI",
                "acceptance_criteria": ["UI renders"],
                "task_role": "implementation",
                "enabled_builtin_kinds": ["CodeMaintainerWrite"],
                "owned_paths": ["src/components"]
            },
            {
                "client_ref": "dashboard",
                "project_task_ref": "pt_2",
                "title": "Dashboard",
                "objective": "Dashboard",
                "acceptance_criteria": ["Dashboard renders"],
                "task_role": "implementation",
                "enabled_builtin_kinds": ["CodeMaintainerWrite"],
                "owned_paths": ["src/components/BudgetDashboard.tsx"]
            }
        ]))
        .expect("execution items");

        let error = validate_parallel_owned_paths(&items).expect_err("overlap must fail");
        assert!(error.contains("文件所有权冲突"));
        assert!(error.contains("src/components"));
    }

    #[test]
    fn ownership_overlap_is_allowed_when_hard_dependencies_make_tasks_serial() {
        let items: Vec<CreateProjectExecutionTaskItem> = serde_json::from_value(json!([
            {
                "client_ref": "model",
                "project_task_ref": "pt_1",
                "title": "Model",
                "objective": "Model",
                "acceptance_criteria": ["Model works"],
                "task_role": "implementation",
                "enabled_builtin_kinds": ["CodeMaintainerWrite"],
                "owned_paths": ["src/domain"]
            },
            {
                "client_ref": "integration",
                "project_task_ref": "pt_2",
                "title": "Integration",
                "objective": "Integration",
                "acceptance_criteria": ["Integration works"],
                "task_role": "implementation",
                "enabled_builtin_kinds": ["CodeMaintainerWrite"],
                "owned_paths": ["src/domain/money.ts"],
                "prerequisite_refs": ["model"]
            }
        ]))
        .expect("execution items");

        validate_parallel_owned_paths(&items).expect("dependency serializes ownership");
    }

    #[test]
    fn verification_role_is_programmatically_read_only() {
        let items: Vec<CreateProjectExecutionTaskItem> = serde_json::from_value(json!([{
            "client_ref": "verify",
            "project_task_ref": "pt_1",
            "title": "Verify",
            "objective": "Verify",
            "acceptance_criteria": ["browser smoke passes"],
            "task_role": "verification",
            "enabled_builtin_kinds": ["CodeMaintainerWrite", "TerminalController"],
            "owned_paths": []
        }]))
        .expect("verification item");

        let error = validate_project_execution_task_contracts(&items)
            .expect_err("verification cannot receive write tools");
        assert!(error.contains("不能启用 CodeMaintainerWrite"));
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
    owned_paths: &[String],
    acceptance_criteria: &[String],
    task_role: &str,
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
    let item_requirement_id = payload
        .get("requirement_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| requirement_id.to_string());
    payload.insert(
        "requirement_id".to_string(),
        Value::String(item_requirement_id),
    );
    payload.insert(
        "root_requirement_id".to_string(),
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
    payload.insert(
        "owned_paths".to_string(),
        Value::Array(owned_paths.iter().cloned().map(Value::String).collect()),
    );
    payload.insert(
        "acceptance_criteria".to_string(),
        Value::Array(
            acceptance_criteria
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    payload.insert(
        "task_role".to_string(),
        Value::String(task_role.to_string()),
    );
    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        enrich_project_execution_payload, reduce_client_ref_dependencies,
        CreateTasksWithPrerequisitesArgs,
    };

    #[test]
    fn materializer_reduces_hard_edges_and_preserves_removed_edges_as_context() {
        let mut args: CreateTasksWithPrerequisitesArgs = serde_json::from_value(json!({
            "tasks": [
                { "client_ref": "a", "title": "A", "objective": "A" },
                { "client_ref": "b", "title": "B", "objective": "B", "prerequisite_refs": ["a"] },
                { "client_ref": "c", "title": "C", "objective": "C", "prerequisite_refs": ["a", "b"] }
            ]
        }))
        .expect("task graph args");

        let diagnostics = reduce_client_ref_dependencies(args.tasks.as_mut_slice())
            .expect("valid task graph should reduce");

        assert_eq!(diagnostics.submitted_edge_count, 3);
        assert_eq!(diagnostics.persisted_edge_count, 2);
        assert_eq!(args.tasks[2].prerequisite_refs, vec!["b"]);
        assert_eq!(args.tasks[2].context_refs, vec!["a"]);
    }

    #[test]
    fn project_execution_payload_preserves_child_requirement_id() {
        let payload = enrich_project_execution_payload(
            Some(json!({ "requirement_id": " child-requirement ", "slice": "analysis" })),
            "project-1",
            "root-requirement",
            "work-item-1",
            "execution-group-1",
            &[],
            &["criterion".to_string()],
            "implementation",
        );

        assert_eq!(
            payload
                .get("requirement_id")
                .and_then(|value| value.as_str()),
            Some("child-requirement")
        );
        assert_eq!(
            payload
                .get("root_requirement_id")
                .and_then(|value| value.as_str()),
            Some("root-requirement")
        );
        assert_eq!(
            payload.get("slice").and_then(|value| value.as_str()),
            Some("analysis")
        );
    }

    #[test]
    fn project_execution_payload_falls_back_to_root_requirement_id() {
        let payload = enrich_project_execution_payload(
            Some(json!({ "requirement_id": "   " })),
            "project-1",
            "root-requirement",
            "work-item-1",
            "execution-group-1",
            &[],
            &["criterion".to_string()],
            "implementation",
        );

        assert_eq!(
            payload
                .get("requirement_id")
                .and_then(|value| value.as_str()),
            Some("root-requirement")
        );
        assert_eq!(
            payload
                .get("root_requirement_id")
                .and_then(|value| value.as_str()),
            Some("root-requirement")
        );
    }
}
