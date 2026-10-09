// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_tool_definitions::TASK_BUILTIN_KIND_VALUES,
    task_tools::{CreateTaskItem, TaskPluginHint, TaskScheduleArgs},
};
use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalTaskDependency, LocalTaskGraph, LocalTaskRecord, LocalTaskStatus,
};
use chrono::DateTime;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) struct DependencyReduction {
    pub(super) submitted_edge_count: usize,
    pub(super) persisted_edge_count: usize,
    pub(super) dependencies: BTreeMap<String, Vec<String>>,
    pub(super) removed_edges: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub(super) struct CreatedTaskBinding {
    pub(super) client_ref: String,
    pub(super) task_id: String,
}

pub(super) fn task_runtime_settings(thinking_level: Option<&str>) -> Result<Value, String> {
    let Some(level) = thinking_level
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(Value::Null);
    };
    let normalized = match level.to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => "none",
        "auto" => "auto",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        "max" => "max",
        _ => return Err("invalid thinking_level".to_string()),
    };
    Ok(json!({
        "selected_thinking_level": normalized,
        "reasoning_enabled": normalized != "none"
    }))
}

pub(super) fn task_for_agent_tool(
    task: &LocalTaskRecord,
    dependencies: &[LocalTaskDependency],
) -> Value {
    let input = task.input.as_object();
    let mut prerequisite_task_ids = dependencies
        .iter()
        .filter(|edge| edge.task_id == task.task_id)
        .map(|edge| edge.prerequisite_task_id.clone())
        .collect::<BTreeSet<_>>();
    prerequisite_task_ids.extend(
        input
            .and_then(|input| input.get("prerequisite_task_ids"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    );
    let mut value = json!({
        "id": task.task_id,
        "title": task.title,
        "description": input.and_then(|input| input.get("description")).cloned().unwrap_or(Value::Null),
        "objective": input.and_then(|input| input.get("objective")).cloned().unwrap_or(Value::Null),
        "input_payload": input.and_then(|input| input.get("input_payload")).cloned().unwrap_or(Value::Null),
        "status": task_status_for_agent(task),
        "priority": input.and_then(|input| input.get("priority")).cloned().unwrap_or_else(|| Value::from(0)),
        "tags": input.and_then(|input| input.get("tags")).cloned().unwrap_or_else(|| json!([])),
        "result_summary": task.result_summary.clone(),
        "last_run_id": task.active_run_id,
        "schedule": input.and_then(|input| input.get("schedule")).cloned().unwrap_or_else(|| json!({"mode": "contact_async"})),
        "parent_task_id": input.and_then(|input| input.get("parent_task_id")).cloned().unwrap_or(Value::Null),
        "source_run_id": input.and_then(|input| input.get("source_run_id")).cloned().unwrap_or(Value::Null),
        "prerequisite_task_ids": prerequisite_task_ids,
        "supersedes_task_ids": input.and_then(|input| input.get("supersedes_task_ids")).cloned().unwrap_or_else(|| json!([])),
        "created_at": unix_ms_rfc3339(task.created_at_unix_ms),
        "updated_at": unix_ms_rfc3339(task.updated_at_unix_ms),
    });
    compact_agent_tool_payload(&mut value);
    value
}

pub(super) fn tasks_for_agent_tool(tasks: &[LocalTaskRecord]) -> Value {
    Value::Array(
        tasks
            .iter()
            .map(|task| task_for_agent_tool(task, &[]))
            .collect(),
    )
}

pub(super) fn batch_creation_value(
    graph: &LocalTaskGraph,
    bindings: &[CreatedTaskBinding],
    diagnostics: Option<&DependencyReduction>,
    idempotent_reused: bool,
    auto_started_runs: &[LocalAgentRunRecord],
) -> Value {
    let refs_by_task = bindings
        .iter()
        .map(|binding| (binding.task_id.as_str(), binding.client_ref.as_str()))
        .collect::<HashMap<_, _>>();
    let created_tasks = graph
        .tasks
        .iter()
        .map(|task| {
            let mut value = json!({
                "task_id": task.task_id,
                "title": task.title,
                "status": task_status_for_agent(task),
            });
            if let Some(client_ref) = refs_by_task.get(task.task_id.as_str()) {
                value["client_ref"] = Value::String((*client_ref).to_string());
            }
            value
        })
        .collect::<Vec<_>>();
    let mut dependency_edges = if idempotent_reused {
        Vec::new()
    } else {
        graph
            .dependencies
            .iter()
            .map(|edge| {
                json!({
                    "task_id": edge.task_id,
                    "prerequisite_task_id": edge.prerequisite_task_id,
                })
            })
            .collect::<Vec<_>>()
    };
    if !idempotent_reused {
        for task in &graph.tasks {
            let Some(external) = task
                .input
                .get("prerequisite_task_ids")
                .and_then(Value::as_array)
            else {
                continue;
            };
            dependency_edges.extend(external.iter().filter_map(Value::as_str).map(|id| {
                json!({
                    "task_id": task.task_id,
                    "prerequisite_task_id": id,
                })
            }));
        }
    }
    let removed = diagnostics
        .map(|diagnostics| {
            diagnostics
                .removed_edges
                .iter()
                .map(|(dependent_id, prerequisite_id)| {
                    json!({
                        "dependent_id": dependent_id,
                        "prerequisite_id": prerequisite_id,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let submitted_edge_count = diagnostics
        .map(|diagnostics| diagnostics.submitted_edge_count)
        .unwrap_or(dependency_edges.len());
    let persisted_edge_count = diagnostics
        .map(|diagnostics| diagnostics.persisted_edge_count)
        .unwrap_or(dependency_edges.len());
    json!({
        "idempotent_reused": idempotent_reused,
        "created_tasks": created_tasks,
        "dependency_edges": dependency_edges,
        "removed_redundant_edges": removed,
        "dependency_diagnostics": {
            "submitted_edge_count": submitted_edge_count,
            "persisted_edge_count": persisted_edge_count,
        },
        "auto_started_runs": auto_started_runs.iter().map(|run| json!({
            "run_id": run.run_id,
            "task_id": run.owner_entity_id,
            "status": run.status.as_str(),
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn cancellation_value(
    graph: &LocalTaskGraph,
    task_id: &str,
    reason: &str,
    previous_active_run_id: Option<&str>,
) -> Result<Value, String> {
    let task = graph
        .tasks
        .iter()
        .find(|task| task.task_id == task_id)
        .ok_or_else(|| format!("cancelled Task not found in Task Graph: {task_id}"))?;
    let cascade_cancelled_task_ids = graph
        .tasks
        .iter()
        .filter(|candidate| {
            candidate.task_id != task_id && candidate.status == LocalTaskStatus::Cancelled
        })
        .map(|candidate| candidate.task_id.clone())
        .collect::<Vec<_>>();
    Ok(json!({
        "cancelled": task.status == LocalTaskStatus::Cancelled || !cascade_cancelled_task_ids.is_empty(),
        "task_id": task.task_id,
        "status": task_status_for_agent(task),
        "reason": reason,
        "active_run_ids": previous_active_run_id.into_iter().collect::<Vec<_>>(),
        "cascade_cancelled_task_ids": cascade_cancelled_task_ids,
        "callback_event": "task.cancelled",
        "task": task_for_agent_tool(task, &graph.dependencies),
    }))
}

pub(super) fn task_status_for_agent(task: &LocalTaskRecord) -> &'static str {
    if task.status == LocalTaskStatus::Ready && task.active_run_id.is_some() {
        return "queued";
    }
    match task.status {
        LocalTaskStatus::Pending | LocalTaskStatus::Ready => "ready",
        LocalTaskStatus::Running => "running",
        LocalTaskStatus::Succeeded => "succeeded",
        LocalTaskStatus::Failed => "failed",
        LocalTaskStatus::Cancelled => "cancelled",
        LocalTaskStatus::Blocked => "blocked",
    }
}

fn compact_agent_tool_payload(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(compact_agent_tool_payload),
        Value::Object(object) => {
            let keys = object.keys().cloned().collect::<Vec<_>>();
            let mut truncated_fields = Vec::new();
            for key in keys {
                let Some(item) = object.get_mut(&key) else {
                    continue;
                };
                let limit = match key.as_str() {
                    "result_summary" => 2_400,
                    "objective" | "description" | "error_message" => 6_000,
                    _ => 12_000,
                };
                if let Value::String(text) = item {
                    let mut chars = text.chars();
                    let prefix = chars.by_ref().take(limit).collect::<String>();
                    if chars.next().is_some() {
                        *text = format!(
                            "{prefix}\n… [truncated; use the task/run detail APIs for the full record]"
                        );
                        truncated_fields.push(Value::String(key));
                    }
                } else {
                    compact_agent_tool_payload(item);
                }
            }
            if !truncated_fields.is_empty() {
                object.insert(
                    "_truncated_fields".to_string(),
                    Value::Array(truncated_fields),
                );
            }
        }
        _ => {}
    }
}

fn unix_ms_rfc3339(value: i64) -> Value {
    DateTime::from_timestamp_millis(value)
        .map(|value| Value::String(value.to_rfc3339()))
        .unwrap_or(Value::Null)
}

pub(super) fn reduce_client_ref_dependencies(
    tasks: &mut [CreateTaskItem],
) -> Result<DependencyReduction, String> {
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
    for (dependent_id, prerequisite_id) in &reduction.removed_edges {
        removed_by_dependent
            .entry(dependent_id.clone())
            .or_default()
            .push(prerequisite_id.clone());
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
    Ok(DependencyReduction {
        submitted_edge_count,
        persisted_edge_count: reduction.dependencies.values().map(Vec::len).sum(),
        dependencies: reduction.dependencies,
        removed_edges: reduction.removed_edges,
    })
}

fn transitive_reduce_prerequisite_map(
    node_ids: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<DependencyReduction, String> {
    if node_ids.iter().any(|node_id| node_id.trim().is_empty()) {
        return Err("dependency graph node id cannot be empty".to_string());
    }
    let mut normalized = node_ids
        .iter()
        .map(|node_id| (node_id.clone(), Vec::<String>::new()))
        .collect::<BTreeMap<_, _>>();
    for (dependent_id, prerequisite_ids) in dependency_map {
        if !node_ids.contains(dependent_id) {
            return Err(format!(
                "dependency graph contains unknown task: {dependent_id}"
            ));
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
                return Err(format!("task cannot depend on itself: {dependent_id}"));
            }
            if !node_ids.contains(prerequisite_id) {
                return Err(format!(
                    "task {dependent_id} contains unknown prerequisite: {prerequisite_id}"
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
                removed_edges.push((dependent_id.clone(), prerequisite_id.clone()));
            }
        }
    }
    Ok(DependencyReduction {
        submitted_edge_count: 0,
        persisted_edge_count: 0,
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
            return Err("task dependency graph contains a cycle".to_string());
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

pub(super) fn attach_dependency_context_payload(
    input_payload: Value,
    client_ref: &str,
    context_refs: &[String],
) -> Value {
    let mut payload = match input_payload {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        value => {
            let mut map = serde_json::Map::new();
            map.insert("input".to_string(), value);
            map
        }
    };
    payload.insert(
        "execution_client_ref".to_string(),
        Value::String(client_ref.to_string()),
    );
    payload.insert(
        "dependency_context_refs".to_string(),
        Value::Array(context_refs.iter().cloned().map(Value::String).collect()),
    );
    Value::Object(payload)
}

pub(super) fn task_prompt(
    objective: &str,
    description: &str,
    input_payload: &Value,
) -> Result<String, String> {
    let objective = objective.trim();
    if objective.is_empty() {
        return Err("task objective cannot be empty".to_string());
    }
    let english = !contains_cjk(objective) && !contains_cjk(description);
    let mut prompt = if english {
        format!("Task Objective:\n{objective}")
    } else {
        format!("任务目标：\n{objective}")
    };
    let description = description.trim();
    if !description.is_empty() {
        prompt.push_str(if english {
            "\n\nTask Description:\n"
        } else {
            "\n\n任务说明：\n"
        });
        prompt.push_str(description);
    }
    if !input_payload.is_null() {
        prompt.push_str(if english {
            "\n\nInput Data:\n"
        } else {
            "\n\n输入数据：\n"
        });
        prompt.push_str(
            &serde_json::to_string(input_payload)
                .map_err(|error| format!("task input is not serializable: {error}"))?,
        );
    }
    prompt.push_str(if english {
        "\n\n[Project Inspection Evidence Contract]\nWhen the objective asks to inspect, review, evaluate, understand, or summarize a real project, do not finish from README or architecture documents alone. Inspect and cross-check the build or package manifests, source entry points, representative core implementations, and tests or CI configuration. Report the concrete files and checks used as evidence and state any validation gap. When terminal or build tools are available and the user expects current behavior or correctness, run a proportionate validation. Finish once the evidence is sufficient; do not perform unrelated work.\n\n[Output Language Policy]\nUse the language requested by the user or used in this task for all user-visible prose. Preserve code identifiers, commands, paths, APIs, and product names."
    } else {
        "\n\n[项目检查最低证据契约]\n当目标要求检查、审查、评估、了解或总结真实项目时，不得只依据 README 或架构文档结束。至少读取并交叉核对构建或包管理清单、源码入口、具有代表性的核心实现，以及测试或 CI 配置；结果必须列出实际作为证据的文件和检查，并说明尚未验证的部分。若本轮提供终端或构建工具，且用户关注当前行为或正确性，应执行与目标相称的验证。证据充分后及时结束，不得扩展无关工作。\n\n[输出语言规则]\n所有用户可见文本使用用户要求或当前任务所使用的语言；代码标识符、命令、路径、API 和产品名保持原样。"
    });
    Ok(prompt)
}

pub(super) fn validated_builtin_kinds(
    requires_execution: bool,
    requested: Vec<String>,
) -> Result<Vec<String>, String> {
    let allowed = TASK_BUILTIN_KIND_VALUES.into_iter().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut kinds = Vec::with_capacity(requested.len() + 2);
    for kind in requested {
        if !allowed.contains(kind.as_str()) {
            return Err(format!("unsupported enabled_builtin_kind: {kind}"));
        }
        if !seen.insert(kind.clone()) {
            return Err(format!("duplicated enabled_builtin_kind: {kind}"));
        }
        kinds.push(kind);
    }
    if !requires_execution
        && kinds.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "CodeMaintainerWrite" | "TerminalController" | "RequirementSurveyWrite"
            )
        })
    {
        return Err(
            "execution-only builtin capabilities require requires_execution=true".to_string(),
        );
    }
    complete_kind_dependency(
        &mut kinds,
        &mut seen,
        "CodeMaintainerWrite",
        "CodeMaintainerRead",
    );
    complete_kind_dependency(
        &mut kinds,
        &mut seen,
        "RequirementSurveyWrite",
        "RequirementSurveyRead",
    );
    Ok(kinds)
}

fn complete_kind_dependency(
    kinds: &mut Vec<String>,
    seen: &mut HashSet<String>,
    selected: &str,
    dependency: &str,
) {
    if seen.contains(selected) && seen.insert(dependency.to_string()) {
        kinds.push(dependency.to_string());
    }
}

pub(super) fn validate_external_mcp_ids(config_ids: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(config_ids.len());
    let mut seen = HashSet::with_capacity(config_ids.len());
    for raw in config_ids {
        let id = raw.trim();
        if id.is_empty() || id.len() > 256 {
            return Err("external MCP ids must contain 1..=256 characters".to_string());
        }
        if !seen.insert(id.to_string()) {
            return Err(format!("duplicated external MCP id: {id}"));
        }
        normalized.push(id.to_string());
    }
    Ok(normalized)
}

pub(super) fn validate_plugin_hints(hints: &[TaskPluginHint]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for hint in hints {
        let key = hint.plugin_key.trim();
        if key.is_empty() || !seen.insert(key) {
            return Err("plugin_hints must contain unique non-empty plugin_key values".to_string());
        }
        if hint.reason.chars().count() > 1_000 {
            return Err("plugin hint reason cannot exceed 1000 characters".to_string());
        }
    }
    Ok(())
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|character| {
        ('\u{3400}'..='\u{4dbf}').contains(&character)
            || ('\u{4e00}'..='\u{9fff}').contains(&character)
    })
}

pub(super) fn contact_async_schedule(schedule: Option<TaskScheduleArgs>) -> Result<Value, String> {
    let schedule = schedule.unwrap_or_default();
    if let Some(mode) = schedule.mode.as_deref() {
        if !matches!(mode, "manual" | "once" | "interval" | "contact_async") {
            return Err(format!("unsupported Task schedule mode: {mode}"));
        }
    }
    if schedule
        .interval_seconds
        .is_some_and(|seconds| seconds <= 0)
    {
        return Err("Task schedule.interval_seconds must be greater than zero".to_string());
    }
    let run_at = schedule
        .run_at
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let run_at_unix_ms = run_at
        .as_deref()
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.timestamp_millis())
                .map_err(|_| "Task schedule.run_at must be an RFC 3339 timestamp".to_string())
        })
        .transpose()?;
    Ok(json!({
        "mode": "contact_async",
        "run_at": run_at,
        "run_at_unix_ms": run_at_unix_ms,
        "interval_seconds": Value::Null,
    }))
}

#[cfg(test)]
mod result_tests {
    use super::*;

    #[test]
    fn task_tool_payload_returns_the_latest_model_output() {
        let task = LocalTaskRecord {
            graph_id: "graph-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation_turn".to_string(),
            source_entity_id: "turn-1".to_string(),
            task_id: "task-1".to_string(),
            title: "Inspect project".to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"objective": "Inspect the project"}),
            max_iterations: 600,
            status: LocalTaskStatus::Succeeded,
            active_run_id: None,
            result_summary: Some("Godot project; run with godot --path .".to_string()),
            version: 2,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };

        let output = task_for_agent_tool(&task, &[]);
        assert_eq!(
            output["result_summary"],
            "Godot project; run with godot --path ."
        );
    }
}
