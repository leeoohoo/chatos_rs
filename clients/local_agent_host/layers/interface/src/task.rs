// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::task_write_scope::{local_task_write_scope, local_task_write_scope_overlap};
use crate::{validate_identifier, validate_text, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;

pub const LOCAL_TASK_GRAPH_MAX_TASKS: usize = 128;
pub const LOCAL_TASK_GRAPH_MAX_DEPENDENCIES: usize = 512;
pub const LOCAL_TASK_MAX_OWNED_PATHS: usize = 200;
pub const LOCAL_TASK_MAX_OWNED_PATH_BYTES: usize = 4_096;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalTaskGraphNotificationPolicy {
    ProgressSummary,
    AllTasks,
    Milestones,
    GraphSummary,
}

impl Default for LocalTaskGraphNotificationPolicy {
    fn default() -> Self {
        Self::ProgressSummary
    }
}

impl LocalTaskGraphNotificationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProgressSummary => "progress_summary",
            Self::AllTasks => "all_tasks",
            Self::Milestones => "milestones",
            Self::GraphSummary => "graph_summary",
        }
    }

    pub const fn default_for_task_count(task_count: usize) -> Self {
        // Legacy graphs without a persisted policy retain their original behavior.
        // New batch creation explicitly freezes Default (progress_summary).
        if task_count > 1 {
            Self::GraphSummary
        } else {
            Self::AllTasks
        }
    }
}

impl FromStr for LocalTaskGraphNotificationPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "progress_summary" => Ok(Self::ProgressSummary),
            "all_tasks" => Ok(Self::AllTasks),
            "milestones" => Ok(Self::Milestones),
            "graph_summary" => Ok(Self::GraphSummary),
            _ => Err(format!("unknown Task graph notification policy: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskSpec {
    pub task_id: String,
    pub title: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    #[serde(default)]
    pub input: Value,
    pub max_iterations: u32,
}

impl LocalTaskSpec {
    fn validate(&self) -> Result<(), String> {
        validate_identifier("task_id", &self.task_id)?;
        let title = self.title.trim();
        if title.is_empty() || title.len() > 1_000 || title.contains('\0') {
            return Err("title must be 1..=1000 characters without NUL".to_string());
        }
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        crate::validate_task_final_response_limit(&self.input)?;
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("task input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "task input exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct LocalTaskDependency {
    pub task_id: String,
    pub prerequisite_task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateTaskGraphCommand {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub tasks: Vec<LocalTaskSpec>,
    #[serde(default)]
    pub dependencies: Vec<LocalTaskDependency>,
}

impl CreateTaskGraphCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("graph_id", &self.graph_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("source_entity_type", &self.source_entity_type)?;
        validate_identifier("source_entity_id", &self.source_entity_id)?;
        if self.tasks.is_empty() || self.tasks.len() > LOCAL_TASK_GRAPH_MAX_TASKS {
            return Err(format!(
                "tasks must contain 1..={LOCAL_TASK_GRAPH_MAX_TASKS} items"
            ));
        }
        if self.dependencies.len() > LOCAL_TASK_GRAPH_MAX_DEPENDENCIES {
            return Err(format!(
                "dependencies must contain at most {LOCAL_TASK_GRAPH_MAX_DEPENDENCIES} items"
            ));
        }
        let mut task_ids = HashSet::new();
        for task in &self.tasks {
            task.validate()?;
            if !task_ids.insert(task.task_id.as_str()) {
                return Err(format!("task_id is duplicated: {}", task.task_id));
            }
        }
        resolve_local_task_graph_notification_policy(
            self.tasks.iter().map(|task| &task.input),
            self.tasks.len(),
        )?;
        validate_dependencies(&task_ids, &self.dependencies)?;
        validate_final_delivery(&self.tasks, &self.dependencies)?;
        validate_parallel_write_ownership(&self.tasks, &self.dependencies)
    }
}

fn validate_final_delivery(
    tasks: &[LocalTaskSpec],
    dependencies: &[LocalTaskDependency],
) -> Result<(), String> {
    let mut final_task = None;
    for task in tasks {
        let Some(value) = task.input.get("final_delivery") else {
            continue;
        };
        let selected = value.as_bool().ok_or("final_delivery must be a boolean")?;
        if selected && final_task.replace(task.task_id.as_str()).is_some() {
            return Err("a graph may designate only one final_delivery Task".to_string());
        }
    }
    if let Some(final_id) = final_task {
        let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
        for dependency in dependencies {
            outgoing
                .entry(&dependency.prerequisite_task_id)
                .or_default()
                .push(&dependency.task_id);
        }
        for task in tasks {
            if task.task_id != final_id && !task_reaches(&task.task_id, final_id, &outgoing) {
                return Err(format!(
                    "final_delivery Task must depend on every other graph Task: {}",
                    task.task_id
                ));
            }
        }
    }
    Ok(())
}

pub fn resolve_local_task_graph_notification_policy<'a>(
    inputs: impl IntoIterator<Item = &'a Value>,
    task_count: usize,
) -> Result<LocalTaskGraphNotificationPolicy, String> {
    let mut declared = None;
    for input in inputs {
        if let Some(value) = input.get("notification_milestone") {
            if !value.is_boolean() {
                return Err("notification_milestone must be a boolean".to_string());
            }
        }
        let Some(value) = input.get("graph_notification_policy") else {
            continue;
        };
        let value = value
            .as_str()
            .ok_or_else(|| "graph_notification_policy must be a string".to_string())?;
        let policy = LocalTaskGraphNotificationPolicy::from_str(value)?;
        if declared.is_some_and(|declared| declared != policy) {
            return Err(
                "all Tasks in one graph must use the same graph_notification_policy".to_string(),
            );
        }
        declared = Some(policy);
    }
    Ok(declared
        .unwrap_or_else(|| LocalTaskGraphNotificationPolicy::default_for_task_count(task_count)))
}

fn validate_parallel_write_ownership(
    tasks: &[LocalTaskSpec],
    dependencies: &[LocalTaskDependency],
) -> Result<(), String> {
    let mut write_scopes = Vec::new();
    for task in tasks {
        if let Some(owned_paths) = local_task_write_scope(&task.input, &task.task_id)? {
            write_scopes.push((task.task_id.as_str(), owned_paths));
        }
    }
    if write_scopes.len() < 2 {
        return Ok(());
    }

    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for dependency in dependencies {
        outgoing
            .entry(dependency.prerequisite_task_id.as_str())
            .or_default()
            .push(dependency.task_id.as_str());
    }
    for left_index in 0..write_scopes.len() {
        for right_index in (left_index + 1)..write_scopes.len() {
            let (left_id, left_paths) = &write_scopes[left_index];
            let (right_id, right_paths) = &write_scopes[right_index];
            if task_reaches(left_id, right_id, &outgoing)
                || task_reaches(right_id, left_id, &outgoing)
            {
                continue;
            }
            if let Some(conflict) = local_task_write_scope_overlap(left_paths, right_paths) {
                return Err(format!(
                    "parallel write tasks have overlapping owned_paths: {left_id} and {right_id} conflict at {conflict}; declare disjoint paths or add a prerequisite"
                ));
            }
        }
    }
    Ok(())
}

fn task_reaches(start: &str, target: &str, outgoing: &HashMap<&str, Vec<&str>>) -> bool {
    let mut pending = VecDeque::from([start]);
    let mut visited = HashSet::new();
    while let Some(task_id) = pending.pop_front() {
        if !visited.insert(task_id) {
            continue;
        }
        for next in outgoing.get(task_id).into_iter().flatten() {
            if *next == target {
                return true;
            }
            pending.push_back(next);
        }
    }
    false
}

fn validate_dependencies(
    task_ids: &HashSet<&str>,
    dependencies: &[LocalTaskDependency],
) -> Result<(), String> {
    let mut unique = HashSet::new();
    let mut indegree = task_ids
        .iter()
        .map(|task_id| ((*task_id).to_string(), 0_usize))
        .collect::<HashMap<_, _>>();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for dependency in dependencies {
        validate_identifier("dependency.task_id", &dependency.task_id)?;
        validate_identifier(
            "dependency.prerequisite_task_id",
            &dependency.prerequisite_task_id,
        )?;
        if !task_ids.contains(dependency.task_id.as_str())
            || !task_ids.contains(dependency.prerequisite_task_id.as_str())
        {
            return Err("every dependency must reference tasks in the same graph".to_string());
        }
        if dependency.task_id == dependency.prerequisite_task_id {
            return Err("a task cannot depend on itself".to_string());
        }
        if !unique.insert((
            dependency.task_id.as_str(),
            dependency.prerequisite_task_id.as_str(),
        )) {
            return Err("task dependency is duplicated".to_string());
        }
        *indegree
            .get_mut(dependency.task_id.as_str())
            .expect("validated task") += 1;
        outgoing
            .entry(dependency.prerequisite_task_id.as_str())
            .or_default()
            .push(dependency.task_id.as_str());
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(task_id, degree)| (*degree == 0).then_some(task_id.clone()))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(task_id) = ready.pop_front() {
        visited += 1;
        for dependent in outgoing.get(task_id.as_str()).into_iter().flatten() {
            let degree = indegree.get_mut(*dependent).expect("validated dependent");
            *degree -= 1;
            if *degree == 0 {
                ready.push_back((*dependent).to_string());
            }
        }
    }
    if visited != task_ids.len() {
        return Err("task dependency graph contains a cycle".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetTaskGraphCommand {
    pub owner_user_id: String,
    pub graph_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetMessageTaskGraphCommand {
    pub owner_user_id: String,
    pub source_conversation_id: String,
    pub source_turn_id: String,
    #[serde(default)]
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetTaskRunsCommand {
    pub owner_user_id: String,
    pub task_id: String,
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalTaskGraphListScope {
    Active,
    Terminal,
    #[default]
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListTaskGraphsCommand {
    pub owner_user_id: String,
    #[serde(default)]
    pub scope: LocalTaskGraphListScope,
    #[serde(default)]
    pub source_entity_type: Option<String>,
    #[serde(default)]
    pub source_entity_id: Option<String>,
    pub before_updated_at_unix_ms: Option<i64>,
    pub before_graph_id: Option<String>,
    pub limit: u32,
}

impl ListTaskGraphsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        match (
            self.source_entity_type.as_deref(),
            self.source_entity_id.as_deref(),
        ) {
            (None, None) => {}
            (Some(entity_type), Some(entity_id)) => {
                validate_identifier("source_entity_type", entity_type)?;
                validate_identifier("source_entity_id", entity_id)?;
            }
            _ => {
                return Err(
                    "source_entity_type and source_entity_id must be supplied together".to_string(),
                );
            }
        }
        if !(1..=100).contains(&self.limit) {
            return Err("limit must be between 1 and 100".to_string());
        }
        match (
            self.before_updated_at_unix_ms,
            self.before_graph_id.as_deref(),
        ) {
            (None, None) => Ok(()),
            (Some(timestamp), Some(graph_id)) if timestamp >= 0 => {
                validate_identifier("before_graph_id", graph_id)
            }
            _ => Err(
                "before_updated_at_unix_ms and before_graph_id must be supplied together"
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelTaskCommand {
    pub owner_user_id: String,
    pub task_id: String,
    pub expected_version: Option<u64>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacement_task_ids: Vec<String>,
}

impl CancelTaskCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("task_id", &self.task_id)?;
        if self.expected_version == Some(0) {
            return Err("expected_version must be greater than zero".to_string());
        }
        validate_text("reason", &self.reason, 4_000)?;
        if self.replacement_task_ids.len() > LOCAL_TASK_GRAPH_MAX_TASKS {
            return Err(format!(
                "replacement_task_ids must contain at most {LOCAL_TASK_GRAPH_MAX_TASKS} items"
            ));
        }
        let mut unique = HashSet::new();
        for replacement_task_id in &self.replacement_task_ids {
            validate_identifier("replacement_task_id", replacement_task_id)?;
            if replacement_task_id == &self.task_id {
                return Err("a cancelled task cannot replace itself".to_string());
            }
            if !unique.insert(replacement_task_id) {
                return Err("replacement_task_ids contains a duplicate".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryTaskCommand {
    pub owner_user_id: String,
    pub task_id: String,
    pub expected_version: u64,
    #[serde(default)]
    pub retry_instruction: Option<String>,
}

impl RetryTaskCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("task_id", &self.task_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if let Some(instruction) = self.retry_instruction.as_deref() {
            validate_text("retry_instruction", instruction, 8_000)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestartTaskCommand {
    pub owner_user_id: String,
    pub task_id: String,
    pub expected_version: u64,
    pub reason: String,
}

impl RestartTaskCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("task_id", &self.task_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        validate_text("reason", &self.reason, 4_000)
    }
}

impl GetTaskGraphCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("graph_id", &self.graph_id)
    }
}

impl GetMessageTaskGraphCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("source_conversation_id", &self.source_conversation_id)?;
        validate_identifier("source_turn_id", &self.source_turn_id)?;
        if let Some(message_id) = self.source_user_message_id.as_deref() {
            validate_identifier("source_user_message_id", message_id)?;
        }
        Ok(())
    }
}

impl GetTaskRunsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("task_id", &self.task_id)?;
        if !(1..=100).contains(&self.limit) {
            return Err("limit must be between 1 and 100".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalTaskStatus {
    Pending,
    Ready,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalTaskGraphStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl LocalTaskGraphStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn derive(tasks: &[LocalTaskRecord]) -> Self {
        if tasks
            .iter()
            .all(|task| task.status == LocalTaskStatus::Succeeded)
        {
            return Self::Succeeded;
        }
        let has_active = tasks.iter().any(|task| {
            matches!(
                task.status,
                LocalTaskStatus::Pending | LocalTaskStatus::Ready | LocalTaskStatus::Running
            )
        });
        if has_active {
            let has_progress = tasks.iter().any(|task| {
                task.active_run_id.is_some()
                    || !matches!(
                        task.status,
                        LocalTaskStatus::Pending | LocalTaskStatus::Ready
                    )
            });
            return if has_progress {
                Self::Running
            } else {
                Self::Pending
            };
        }
        if tasks
            .iter()
            .any(|task| task.status == LocalTaskStatus::Failed)
        {
            Self::Failed
        } else if tasks
            .iter()
            .any(|task| task.status == LocalTaskStatus::Cancelled)
        {
            Self::Cancelled
        } else {
            Self::Failed
        }
    }
}

impl FromStr for LocalTaskGraphStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown local task graph status: {value}")),
        }
    }
}

impl LocalTaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }
}

impl FromStr for LocalTaskStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "blocked" => Ok(Self::Blocked),
            _ => Err(format!("unknown local task status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskRecord {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub task_id: String,
    pub title: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub input: Value,
    pub max_iterations: u32,
    pub status: LocalTaskStatus,
    pub active_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacement_task_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_because_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskGraph {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub status: LocalTaskGraphStatus,
    pub tasks: Vec<LocalTaskRecord>,
    pub dependencies: Vec<LocalTaskDependency>,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskGraphSummary {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub status: LocalTaskGraphStatus,
    pub task_count: u32,
    pub succeeded_task_count: u32,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskGraphPage {
    pub graphs: Vec<LocalTaskGraphSummary>,
    pub next_before_updated_at_unix_ms: Option<i64>,
    pub next_before_graph_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalMessageTaskGraphNode {
    pub task: LocalTaskRecord,
    pub depth: u32,
    pub is_root: bool,
    pub is_current_message: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalMessageTaskGraphEdge {
    pub source_task_id: String,
    pub target_task_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalMessageTaskGraph {
    pub root_task_ids: Vec<String>,
    pub nodes: Vec<LocalMessageTaskGraphNode>,
    pub edges: Vec<LocalMessageTaskGraphEdge>,
    pub source_conversation_id: String,
    pub source_turn_id: String,
    pub source_user_message_id: Option<String>,
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
