// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

mod local_connector_path;

pub use local_connector_path::{
    local_connector_relative_path_is_safe, local_connector_workspace_root,
    normalize_local_connector_relative_path, parse_local_connector_workspace_root,
    LocalConnectorWorkspaceRef, LOCAL_CONNECTOR_ROOT_PREFIX,
};

pub const STATUS_PLANNING: &str = "planning";
pub const STATUS_PLANNING_STARTED: &str = "planning_started";
pub const STATUS_AWAITING_CONFIRMATION: &str = "awaiting_confirmation";
pub const STATUS_EXECUTION_STARTED: &str = "execution_started";
pub const STATUS_ALREADY_CONFIRMED: &str = "already_confirmed";
pub const STATUS_PAUSED: &str = "paused";
pub const STATUS_STOPPING: &str = "stopping";
pub const STATUS_STOPPED: &str = "stopped";
pub const NEXT_ACTION_PREVIEW_AND_CONFIRM: &str = "preview_and_confirm";

pub const RECOVERY_ACTION_NONE: &str = "none";
pub const RECOVERY_ACTION_RERUN: &str = "rerun";
pub const RECOVERY_ACTION_REGENERATE: &str = "regenerate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementExecutionRecoveryState {
    pub action: &'static str,
    pub reason: &'static str,
    pub replace_previous_batch: bool,
}

pub fn requirement_execution_status_is_stopped_terminal(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        STATUS_STOPPED | "cancelled" | "canceled"
    )
}

pub fn requirement_execution_status_is_stopping(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case(STATUS_STOPPING)
}

pub fn requirement_execution_recovery_state(
    status: &str,
    task_count: usize,
    has_started_runs: bool,
    source_available: bool,
    discarded_tasks: bool,
) -> RequirementExecutionRecoveryState {
    if !source_available {
        return RequirementExecutionRecoveryState {
            action: RECOVERY_ACTION_NONE,
            reason: "source_missing",
            replace_previous_batch: false,
        };
    }

    let replace_previous_batch = true;
    let normalized = status.trim().to_ascii_lowercase();
    if requirement_execution_status_is_stopping(normalized.as_str()) {
        return RequirementExecutionRecoveryState {
            action: RECOVERY_ACTION_NONE,
            reason: "cancellation_settling",
            replace_previous_batch,
        };
    }

    if requirement_execution_status_is_stopped_terminal(normalized.as_str()) {
        if discarded_tasks {
            return RequirementExecutionRecoveryState {
                action: RECOVERY_ACTION_REGENERATE,
                reason: "stopped_after_task_discard",
                replace_previous_batch,
            };
        }
        return if task_count > 0 {
            RequirementExecutionRecoveryState {
                action: RECOVERY_ACTION_RERUN,
                reason: "stopped_with_task_graph",
                replace_previous_batch,
            }
        } else {
            RequirementExecutionRecoveryState {
                action: RECOVERY_ACTION_REGENERATE,
                reason: "stopped_without_task_graph",
                replace_previous_batch,
            }
        };
    }

    if normalized == "failed" && !has_started_runs {
        return RequirementExecutionRecoveryState {
            action: RECOVERY_ACTION_REGENERATE,
            reason: "failed_before_execution_started",
            replace_previous_batch,
        };
    }

    RequirementExecutionRecoveryState {
        action: RECOVERY_ACTION_NONE,
        reason: "not_recoverable_in_current_state",
        replace_previous_batch,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPlane {
    Cloud,
    LocalConnector,
}

impl ExecutionPlane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::LocalConnector => "local_connector",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlanIdentity {
    pub execution_group_id: String,
    pub conversation_id: String,
}

impl ExecutionPlanIdentity {
    pub fn required(execution_group_id: &str, conversation_id: &str) -> Result<Self, String> {
        Self::optional(Some(execution_group_id), Some(conversation_id))?
            .ok_or_else(|| "execution_group_id and conversation_id are required".to_string())
    }

    pub fn optional(
        execution_group_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let execution_group_id = normalized_text(execution_group_id);
        let conversation_id = normalized_text(conversation_id);
        match (execution_group_id, conversation_id) {
            (None, None) => Ok(None),
            (Some(execution_group_id), Some(conversation_id)) => Ok(Some(Self {
                execution_group_id,
                conversation_id,
            })),
            _ => {
                Err("execution_group_id and conversation_id must be provided together".to_string())
            }
        }
    }
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn read_planning_feedback_history(execution: Option<&Value>) -> Vec<String> {
    let Some(execution) = execution else {
        return Vec::new();
    };
    let history = execution
        .get("planning_feedback_history")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|value| normalized_text(Some(value)))
        .collect::<Vec<_>>();
    if !history.is_empty() {
        return history;
    }
    normalized_text(execution.get("planning_feedback").and_then(Value::as_str))
        .into_iter()
        .collect()
}

pub fn append_planning_feedback(previous: &[String], feedback: Option<&str>) -> Vec<String> {
    let mut history = previous
        .iter()
        .filter_map(|value| normalized_text(Some(value.as_str())))
        .collect::<Vec<_>>();
    if let Some(feedback) = normalized_text(feedback) {
        history.push(feedback);
    }
    history
}

pub fn format_planning_feedback_history(history: &[String]) -> Option<String> {
    let history = history
        .iter()
        .filter_map(|value| normalized_text(Some(value.as_str())))
        .collect::<Vec<_>>();
    match history.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        _ => Some(
            history
                .iter()
                .enumerate()
                .map(|(index, value)| format!("{}. {value}", index + 1))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTaskScopeMismatch {
    pub missing: Vec<String>,
    pub unexpected: Vec<String>,
}

impl fmt::Display for ProjectTaskScopeMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "missing=[{}], unexpected=[{}]",
            self.missing.join(","),
            self.unexpected.join(",")
        )
    }
}

pub fn validate_exact_project_task_scope(
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> Result<(), ProjectTaskScopeMismatch> {
    let missing = expected.difference(actual).cloned().collect::<Vec<_>>();
    let unexpected = actual.difference(expected).cloned().collect::<Vec<_>>();
    if missing.is_empty() && unexpected.is_empty() {
        return Ok(());
    }
    Err(ProjectTaskScopeMismatch {
        missing,
        unexpected,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTaskState {
    Planned,
    Active,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    Archived,
    Unknown,
}

pub fn classify_execution_task_status(status: &str) -> ExecutionTaskState {
    match status.trim().to_ascii_lowercase().as_str() {
        "ready" | "todo" | "draft" => ExecutionTaskState::Planned,
        "queued" | "running" | "processing" | "doing" | "in_progress" | "pending" => {
            ExecutionTaskState::Active
        }
        "succeeded" | "success" | "completed" | "done" => ExecutionTaskState::Succeeded,
        "failed" | "error" => ExecutionTaskState::Failed,
        "blocked" => ExecutionTaskState::Blocked,
        "cancelled" | "canceled" => ExecutionTaskState::Cancelled,
        "archived" => ExecutionTaskState::Archived,
        _ => ExecutionTaskState::Unknown,
    }
}

pub fn execution_task_status_is_active(status: &str) -> bool {
    matches!(
        classify_execution_task_status(status),
        ExecutionTaskState::Planned | ExecutionTaskState::Active
    )
}

pub fn execution_task_status_is_success(status: &str) -> bool {
    classify_execution_task_status(status) == ExecutionTaskState::Succeeded
}

pub fn execution_task_status_blocks_confirmation(status: &str) -> bool {
    matches!(
        classify_execution_task_status(status),
        ExecutionTaskState::Failed
            | ExecutionTaskState::Blocked
            | ExecutionTaskState::Cancelled
            | ExecutionTaskState::Archived
    )
}

pub fn execution_task_status_is_planned(status: &str) -> bool {
    classify_execution_task_status(status) == ExecutionTaskState::Planned
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementPlanItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub parent_requirement_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemPlanItem {
    pub id: String,
    pub requirement_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: i64,
    pub tags: Vec<String>,
    pub is_planning_task: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub dependent_id: String,
    pub prerequisite_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyReduction {
    pub dependencies: BTreeMap<String, Vec<String>>,
    pub removed_edges: Vec<DependencyEdge>,
}

/// Reduces a prerequisite graph to the unique transitive reduction of a DAG.
///
/// The map is oriented as `dependent_id -> prerequisite_ids`. Unknown nodes,
/// self dependencies, and cycles are rejected before any edge is removed.
pub fn transitive_reduce_prerequisite_map(
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
        let mut dependencies = prerequisite_ids
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
        dependencies.sort();
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

/// Replaces one node's prerequisite list and removes candidates already
/// implied through another candidate. Existing graph edges must use the same
/// `dependent -> prerequisites` orientation.
pub fn reduce_candidate_prerequisites(
    target_id: &str,
    candidate_prerequisite_ids: Vec<String>,
    existing_dependencies: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    let target_id = target_id.trim();
    if target_id.is_empty() {
        return Err("目标任务 ID 不能为空".to_string());
    }
    let mut dependency_map = existing_dependencies.clone();
    dependency_map.insert(target_id.to_string(), candidate_prerequisite_ids);
    let mut node_ids = BTreeSet::from([target_id.to_string()]);
    for (dependent_id, prerequisite_ids) in &dependency_map {
        node_ids.insert(dependent_id.clone());
        node_ids.extend(
            prerequisite_ids
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    let reduction = transitive_reduce_prerequisite_map(&node_ids, &dependency_map)?;
    Ok(reduction
        .dependencies
        .get(target_id)
        .cloned()
        .unwrap_or_default())
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

pub fn is_done_status(status: &str) -> bool {
    execution_task_status_is_success(status)
}

pub fn validate_requirement_prerequisites(
    items: &[RequirementPlanItem],
    requirement_scope: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let by_id = items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut blockers = Vec::new();
    for requirement_id in requirement_scope {
        let requirement_title = by_id
            .get(requirement_id.as_str())
            .map(|item| item.title.as_str())
            .unwrap_or(requirement_id.as_str());
        for prerequisite_id in dependency_map
            .get(requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|prerequisite_id| !requirement_scope.contains(prerequisite_id.as_str()))
        {
            match by_id.get(prerequisite_id.as_str()) {
                Some(prerequisite) if is_done_status(prerequisite.status.as_str()) => {}
                Some(prerequisite) => blockers.push(format!(
                    "{} 的前置需求未完成：{}（{}）",
                    requirement_title, prerequisite.title, prerequisite.status
                )),
                None => blockers.push(format!(
                    "{} 的前置需求不存在或不可见：{}",
                    requirement_title, prerequisite_id
                )),
            }
        }
    }
    if blockers.is_empty() {
        return Ok(());
    }
    blockers.sort();
    blockers.dedup();
    Err(format!(
        "存在未完成的前置需求，无法执行：{}",
        blockers.join("；")
    ))
}

pub fn collect_requirement_execution_scope(
    items: &[RequirementPlanItem],
    root_id: &str,
    dependency_map: &BTreeMap<String, Vec<String>>,
    include_prerequisite_dependents: bool,
) -> BTreeSet<String> {
    let mut scope = BTreeSet::from([root_id.to_string()]);
    expand_requirement_descendants(items, &mut scope);
    expand_requirement_dependents(dependency_map, &mut scope);
    expand_requirement_descendants(items, &mut scope);

    let status_by_id = items
        .iter()
        .map(|item| (item.id.as_str(), item.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    loop {
        let before = scope.len();
        let current_ids = scope.iter().cloned().collect::<Vec<_>>();
        for requirement_id in current_ids {
            for prerequisite_id in dependency_map
                .get(requirement_id.as_str())
                .into_iter()
                .flatten()
            {
                if scope.contains(prerequisite_id.as_str()) {
                    continue;
                }
                match status_by_id.get(prerequisite_id.as_str()) {
                    Some(status) if is_done_status(status) => {}
                    Some(_) => {
                        scope.insert(prerequisite_id.clone());
                    }
                    None => {}
                }
            }
        }
        expand_requirement_descendants(items, &mut scope);
        if include_prerequisite_dependents {
            expand_requirement_dependents(dependency_map, &mut scope);
            expand_requirement_descendants(items, &mut scope);
        }
        if scope.len() == before {
            break;
        }
    }
    scope
}

fn expand_requirement_descendants(items: &[RequirementPlanItem], scope: &mut BTreeSet<String>) {
    loop {
        let before = scope.len();
        for item in items {
            if item
                .parent_requirement_id
                .as_deref()
                .is_some_and(|parent_id| scope.contains(parent_id))
            {
                scope.insert(item.id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
}

fn expand_requirement_dependents(
    dependency_map: &BTreeMap<String, Vec<String>>,
    scope: &mut BTreeSet<String>,
) {
    loop {
        let before = scope.len();
        for (requirement_id, prerequisite_ids) in dependency_map {
            if prerequisite_ids
                .iter()
                .any(|prerequisite_id| scope.contains(prerequisite_id))
            {
                scope.insert(requirement_id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
}

pub fn add_requirement_work_item_dependencies(
    dependency_map: &mut BTreeMap<String, Vec<String>>,
    work_items: &[WorkItemPlanItem],
    requirement_dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_scope: &BTreeSet<String>,
) {
    for work_item in work_items {
        for prerequisite_requirement_id in requirement_dependency_map
            .get(work_item.requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|requirement_id| requirement_scope.contains(requirement_id.as_str()))
        {
            for prerequisite_item in work_items.iter().filter(|candidate| {
                candidate.requirement_id == *prerequisite_requirement_id
                    && candidate.id != work_item.id
            }) {
                dependency_map
                    .entry(work_item.id.clone())
                    .or_default()
                    .push(prerequisite_item.id.clone());
            }
        }
    }
    for dependencies in dependency_map.values_mut() {
        dependencies.sort();
        dependencies.dedup();
    }
}

pub fn topological_work_item_order(
    work_items: &[WorkItemPlanItem],
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut pending = work_item_ids.clone();
    let mut ready_done = BTreeSet::new();
    let mut order = Vec::new();

    while !pending.is_empty() {
        let ready_ids = pending
            .iter()
            .filter(|work_item_id| {
                dependency_map
                    .get(work_item_id.as_str())
                    .into_iter()
                    .flatten()
                    .filter(|dependency_id| work_item_ids.contains(dependency_id.as_str()))
                    .all(|dependency_id| ready_done.contains(dependency_id.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready_ids.is_empty() {
            return Err("项目任务存在循环前置关系，无法执行".to_string());
        }
        for work_item_id in ready_ids {
            pending.remove(work_item_id.as_str());
            ready_done.insert(work_item_id.clone());
            order.push(work_item_id);
        }
    }
    Ok(order)
}

pub fn missing_project_task_ids<'a>(
    selected_work_items: &'a [WorkItemPlanItem],
    linked_project_task_ids: &BTreeSet<String>,
) -> Vec<&'a str> {
    selected_work_items
        .iter()
        .filter(|item| !linked_project_task_ids.contains(item.id.as_str()))
        .map(|item| item.id.as_str())
        .collect()
}

pub fn select_pending_work_items(
    work_items: &[WorkItemPlanItem],
    requirement_scope: &BTreeSet<String>,
) -> Vec<WorkItemPlanItem> {
    work_items
        .iter()
        .filter(|item| requirement_scope.contains(item.requirement_id.as_str()))
        .filter(|item| !item.status.trim().eq_ignore_ascii_case("archived"))
        .filter(|item| !is_done_status(item.status.as_str()))
        .cloned()
        .collect()
}

pub fn select_unblocked_pending_work_items(
    all_work_items: &[WorkItemPlanItem],
    requirement_scope: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<WorkItemPlanItem>, String> {
    let pending_work_items = select_pending_work_items(all_work_items, requirement_scope);
    if pending_work_items.is_empty() {
        return Ok(Vec::new());
    }

    let all_work_items_by_id = all_work_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let pending_work_items_by_id = pending_work_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut selected_ids = pending_work_items_by_id
        .keys()
        .map(|id| (*id).to_string())
        .collect::<BTreeSet<_>>();

    loop {
        let blocked_ids = selected_ids
            .iter()
            .filter(|work_item_id| {
                dependency_map
                    .get(work_item_id.as_str())
                    .into_iter()
                    .flatten()
                    .any(|dependency_id| {
                        all_work_items_by_id
                            .get(dependency_id.as_str())
                            .is_none_or(|dependency| {
                                !is_done_status(dependency.status.as_str())
                                    && !selected_ids.contains(dependency_id.as_str())
                            })
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        if blocked_ids.is_empty() {
            break;
        }
        for blocked_id in blocked_ids {
            selected_ids.remove(blocked_id.as_str());
        }
    }

    if selected_ids.is_empty() {
        let mut blockers = Vec::new();
        for item in &pending_work_items {
            for dependency_id in dependency_map.get(item.id.as_str()).into_iter().flatten() {
                match all_work_items_by_id.get(dependency_id.as_str()) {
                    Some(dependency) if is_done_status(dependency.status.as_str()) => {}
                    Some(dependency) => blockers.push(format!(
                        "项目任务「{}」的前置任务尚未完成：{}（{}）",
                        item.title, dependency.title, dependency.status
                    )),
                    None => blockers.push(format!(
                        "项目任务「{}」包含不存在或不可见的前置任务: {}",
                        item.title, dependency_id
                    )),
                }
            }
        }
        blockers.sort();
        blockers.dedup();
        return Err(if blockers.is_empty() {
            "当前需求执行范围内没有已解锁的项目任务".to_string()
        } else {
            format!(
                "当前需求执行范围内没有已解锁的项目任务：{}",
                blockers.join("；")
            )
        });
    }

    Ok(pending_work_items
        .into_iter()
        .filter(|item| selected_ids.contains(item.id.as_str()))
        .collect())
}

pub fn sort_work_items_for_planning(work_items: &mut [WorkItemPlanItem]) {
    work_items.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub fn executing_requirement_ids(
    root_requirement_id: &str,
    selected_work_items: &[WorkItemPlanItem],
) -> BTreeSet<String> {
    let mut ids = BTreeSet::from([root_requirement_id.to_string()]);
    ids.extend(
        selected_work_items
            .iter()
            .map(|item| item.requirement_id.clone()),
    );
    ids
}

pub fn build_requirement_execution_user_message(
    requirement: &RequirementPlanItem,
    work_items: &[WorkItemPlanItem],
) -> String {
    const MAX_VISIBLE_TASK_TITLES: usize = 3;
    let mut content = format!(
        "执行需求「{}」的 {} 个关联任务。",
        requirement.title,
        work_items.len()
    );
    let visible_titles = work_items
        .iter()
        .map(|item| item.title.trim())
        .filter(|title| !title.is_empty())
        .take(MAX_VISIBLE_TASK_TITLES)
        .collect::<Vec<_>>();
    if !visible_titles.is_empty() {
        content.push_str("\n\n执行范围：");
        for title in visible_titles {
            content.push_str("\n- ");
            content.push_str(title);
        }
        if work_items.len() > MAX_VISIBLE_TASK_TITLES {
            content.push_str(&format!(
                "\n- 另有 {} 个关联任务",
                work_items.len() - MAX_VISIBLE_TASK_TITLES
            ));
        }
    }
    content
}

#[allow(clippy::too_many_arguments)]
pub fn build_requirement_execution_planner_prompt(
    project_id: &str,
    root_requirement: &RequirementPlanItem,
    requirement_items: &[RequirementPlanItem],
    requirement_scope: &BTreeSet<String>,
    all_work_items: &[WorkItemPlanItem],
    selected_work_items: &[WorkItemPlanItem],
    creation_order: &[String],
    dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_documents: &BTreeMap<String, Value>,
    default_model_config_id: Option<&str>,
    planning_feedback: Option<&str>,
) -> Result<String, String> {
    let scoped_requirements = requirement_items
        .iter()
        .filter(|item| requirement_scope.contains(item.id.as_str()))
        .map(|item| {
            json!({
                "id": item.id,
                "title": item.title,
                "status": item.status,
                "parent_requirement_id": item.parent_requirement_id,
            })
        })
        .collect::<Vec<_>>();
    let selected_project_task_ids = selected_work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let all_work_items_by_id = all_work_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut pending_dependency_map = BTreeMap::<String, Vec<String>>::new();
    let mut satisfied_dependencies = BTreeMap::<String, Vec<Value>>::new();
    for item in selected_work_items {
        for prerequisite_id in dependency_map.get(item.id.as_str()).into_iter().flatten() {
            if selected_project_task_ids.contains(prerequisite_id) {
                pending_dependency_map
                    .entry(item.id.clone())
                    .or_default()
                    .push(prerequisite_id.clone());
                continue;
            }
            let prerequisite = all_work_items_by_id
                .get(prerequisite_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "项目任务「{}」包含不存在或不可见的前置任务: {}",
                        item.title, prerequisite_id
                    )
                })?;
            if !is_done_status(prerequisite.status.as_str()) {
                return Err(format!(
                    "项目任务「{}」的前置任务尚未完成：{}（{}）",
                    item.title, prerequisite.title, prerequisite.status
                ));
            }
            satisfied_dependencies
                .entry(item.id.clone())
                .or_default()
                .push(json!({
                    "id": prerequisite.id,
                    "title": prerequisite.title,
                    "status": prerequisite.status,
                }));
        }
    }
    let dependency_reduction =
        transitive_reduce_prerequisite_map(&selected_project_task_ids, &pending_dependency_map)?;
    let work_items = selected_work_items
        .iter()
        .map(|item| {
            json!({
                "id": item.id,
                "requirement_id": item.requirement_id,
                "title": item.title,
                "description": item.description,
                "status": item.status,
                "priority": item.priority,
                "tags": item.tags,
                "is_planning_task": item.is_planning_task,
                "pending_prerequisite_project_task_ids": dependency_reduction.dependencies
                    .get(item.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                "context_prerequisite_project_task_ids": pending_dependency_map
                    .get(item.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                "satisfied_prerequisite_project_tasks": satisfied_dependencies
                    .get(item.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let selected_project_task_ids = selected_work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let payload = json!({
        "mode": "project_requirement_execution_planning",
        "execution_contract": {
            "user_action": "execute_selected_project_tasks",
            "must_call_tool": "create_project_execution_tasks",
            "must_cover_every_selected_project_task": true,
            "must_not_cover_unselected_project_tasks": true,
            "selected_project_task_ids": selected_project_task_ids,
            "default_model_config_id": default_model_config_id,
            "model_binding_policy": "When default_model_config_id is present, bind it unchanged to every generated execution task.",
            "dependency_policy": "Only pending_prerequisite_project_task_ids are hard blockers. Connect terminal(previous) to entry(next), keep direct blockers only, and never add an edge already implied by another hard path. context_prerequisite_project_task_ids preserve the complete project relationship for explanation and context_refs, but context_refs must not block scheduling. satisfied_prerequisite_project_tasks are completed context and must never be regenerated or emitted as Task Runner dependencies.",
            "decomposition_policy": "Use project-task descriptions and technical documents to create concrete, independently verifiable execution tasks. Split only when it materially improves ownership, ordering, safety, or verification.",
            "planning_task_policy": "is_planning_task=true still requires at least one bound execution task; mark the generated task as planning when appropriate.",
            "forbidden_terminal_response": "Do not return a completion summary before create_project_execution_tasks succeeds and covers every selected project task."
        },
        "project_id": project_id,
        "requirement": {
            "id": root_requirement.id,
            "title": root_requirement.title,
            "status": root_requirement.status,
        },
        "requirements_in_execution_scope": scoped_requirements,
        "selected_project_tasks": work_items,
        "recommended_project_task_creation_order": creation_order,
        "dependency_diagnostics": {
            "removed_redundant_project_task_edges": dependency_reduction.removed_edges,
        },
        "technical_documents_by_requirement": requirement_documents,
        "user_planning_feedback": planning_feedback,
    });
    let payload = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    Ok(format!(
        "这是用户点击‘执行关联任务’产生的强制执行请求。先阅读每个项目任务及其需求技术文档，再生成可验证的执行任务图。你必须调用 create_project_execution_tasks，覆盖全部且仅覆盖 selected_project_tasks，并保持项目任务依赖。只提交工具 Schema 定义的业务字段，运行路由由系统自动完成。已有描述或文档完整不代表任务已执行。若 user_planning_feedback 非空，必须把它作为本轮调整执行计划的最高优先级用户约束，在不破坏执行范围和安全边界的前提下重新拆分任务与依赖。\n\n{payload}"
    ))
}

#[cfg(test)]
mod tests;
