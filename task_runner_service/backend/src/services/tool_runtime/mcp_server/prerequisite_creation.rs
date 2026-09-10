// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::TaskRunRecord;

use super::chatos_async_planner::{
    planner_prerequisite_create_request, planner_root_create_request,
    require_chatos_async_source_context,
};
use super::support::{ensure_client_ref_graph_acyclic, reusable_chatos_async_task};
use super::{
    CreateTaskWithPrerequisitesItem, CreateTasksWithPrerequisitesArgs, McpRequestContext,
    McpToolProfile, TaskRunnerMcpService,
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
            let plugin_hints = task.normalized_plugin_hints()?;
            let mut request = task.into_request()?;
            attach_dependency_context_payload(
                &mut request.input_payload,
                client_ref.as_str(),
                context_refs.as_slice(),
            );
            request_context.enforce_created_task_context(&mut request);
            request.status = None;
            let prerequisite_task_ids = request.prerequisite_task_ids.clone().unwrap_or_default();
            self.ensure_mcp_default_model_config(&mut request, current_user)
                .await?;
            if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                request = if is_prerequisite_node {
                    planner_prerequisite_create_request(request, request_context)?
                } else {
                    planner_root_create_request(request, request_context)?
                };
            }
            let plugin_selection = self
                .task_service
                .resolve_trusted_task_plugin_selection(
                    &request,
                    plugin_hints.as_slice(),
                    current_user,
                )
                .await?;
            request.plugin_config = plugin_selection.plugin_config;
            let task = self
                .task_service
                .create_task_with_plugin_selection_audit(
                    request,
                    Some(current_user),
                    request_context.task_source_context()?,
                    plugin_selection.audit,
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
    removed_edges: Vec<DependencyEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct DependencyEdge {
    dependent_id: String,
    prerequisite_id: String,
}

struct DependencyReduction {
    dependencies: BTreeMap<String, Vec<String>>,
    removed_edges: Vec<DependencyEdge>,
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
    let reduction = transitive_reduce_prerequisite_map(&node_ids, &dependency_map)?;
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

fn transitive_reduce_prerequisite_map(
    node_ids: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<DependencyReduction, String> {
    if node_ids.iter().any(|node_id| node_id.trim().is_empty()) {
        return Err("依赖图节点 ID 不能为空".to_string());
    }
    let mut normalized = node_ids
        .iter()
        .map(|node_id| (node_id.clone(), Vec::<String>::new()))
        .collect::<BTreeMap<_, _>>();
    for (dependent_id, prerequisite_ids) in dependency_map {
        if !node_ids.contains(dependent_id) {
            return Err(format!("依赖图包含未知任务: {dependent_id}"));
        }
        let dependencies = prerequisite_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        for prerequisite_id in &dependencies {
            if prerequisite_id == dependent_id {
                return Err(format!("任务不能依赖自身: {dependent_id}"));
            }
            if !node_ids.contains(prerequisite_id) {
                return Err(format!(
                    "任务 {dependent_id} 包含未知前置任务: {prerequisite_id}"
                ));
            }
        }
        normalized.insert(dependent_id.clone(), dependencies);
    }
    ensure_prerequisite_map_acyclic(node_ids, &normalized)?;
    let mut reduced = normalized.clone();
    let mut removed_edges = Vec::new();
    for (dependent_id, prerequisite_ids) in &normalized {
        for prerequisite_id in prerequisite_ids {
            if prerequisite_reachable_without_edge(
                dependent_id,
                prerequisite_id,
                &normalized,
                (dependent_id, prerequisite_id),
            ) {
                if let Some(dependencies) = reduced.get_mut(dependent_id) {
                    dependencies.retain(|value| value != prerequisite_id);
                }
                removed_edges.push(DependencyEdge {
                    dependent_id: dependent_id.clone(),
                    prerequisite_id: prerequisite_id.clone(),
                });
            }
        }
    }
    Ok(DependencyReduction {
        dependencies: reduced,
        removed_edges,
    })
}

fn ensure_prerequisite_map_acyclic(
    node_ids: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let mut pending = node_ids.clone();
    let mut resolved = BTreeSet::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .filter(|node_id| {
                dependency_map
                    .get(node_id.as_str())
                    .into_iter()
                    .flatten()
                    .all(|prerequisite_id| resolved.contains(prerequisite_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err("任务依赖图包含循环关系".to_string());
        }
        for node_id in ready {
            pending.remove(node_id.as_str());
            resolved.insert(node_id);
        }
    }
    Ok(())
}

fn prerequisite_reachable_without_edge(
    start: &str,
    target: &str,
    dependency_map: &BTreeMap<String, Vec<String>>,
    excluded_edge: (&str, &str),
) -> bool {
    let mut pending = vec![start.to_string()];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        for prerequisite_id in dependency_map.get(current.as_str()).into_iter().flatten() {
            if current == excluded_edge.0 && prerequisite_id == excluded_edge.1 {
                continue;
            }
            if prerequisite_id == target {
                return true;
            }
            pending.push(prerequisite_id.clone());
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{reduce_client_ref_dependencies, CreateTasksWithPrerequisitesArgs};

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
}
