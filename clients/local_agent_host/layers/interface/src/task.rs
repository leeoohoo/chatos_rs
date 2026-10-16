// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use unicode_normalization::UnicodeNormalization;

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

pub fn local_task_write_scope(input: &Value, task_id: &str) -> Result<Option<Vec<String>>, String> {
    let Some(normalized) = normalized_owned_paths(input, task_id)? else {
        return Ok(task_enables_code_write(input).then(Vec::new));
    };
    Ok(task_enables_code_write(input).then_some(normalized))
}

fn normalized_owned_paths(input: &Value, task_id: &str) -> Result<Option<Vec<String>>, String> {
    let Some(raw_paths) = input.get("owned_paths") else {
        return Ok(None);
    };
    let paths = raw_paths
        .as_array()
        .ok_or_else(|| format!("owned_paths must be an array for task: {task_id}"))?;
    if paths.len() > LOCAL_TASK_MAX_OWNED_PATHS {
        return Err(format!(
            "owned_paths must contain at most {LOCAL_TASK_MAX_OWNED_PATHS} entries for task: {task_id}"
        ));
    }
    let mut normalized = Vec::with_capacity(paths.len());
    let mut unique = HashSet::new();
    for value in paths {
        let path = value
            .as_str()
            .ok_or_else(|| format!("owned_paths must contain strings for task: {task_id}"))?;
        validate_owned_path(path).map_err(|error| format!("{error} for task: {task_id}"))?;
        if !unique.insert(path) {
            return Err(format!(
                "owned_paths contains a duplicate for task: {task_id}"
            ));
        }
        normalized.push(path.to_string());
    }
    Ok(Some(normalized))
}

/// Returns the project-relative write scope that must be held for the full Task Run.
///
/// `CodeMaintainerWrite` can be constrained by declared `owned_paths`. Terminal and External MCP
/// execution remain entire-project writers. Plugin execution holds one synthetic lock per selected
/// Plugin plus any declared paths: Tasks using different Plugins and disjoint files may run in
/// parallel, while the same Plugin or an explicitly shared file stays serialized.
pub fn local_task_runtime_write_scope(
    input: &Value,
    task_id: &str,
) -> Result<Option<Vec<String>>, String> {
    let plugin_keys = task_selected_plugin_keys(input, task_id)?;
    let external_mcp_selected = task_selects_external_mcp(input, task_id)?;
    let code_write_scope = local_task_write_scope(input, task_id)?;
    if task_enables_terminal(input) || external_mcp_selected {
        return Ok(Some(Vec::new()));
    }
    if !plugin_keys.is_empty() {
        let declared_paths = normalized_owned_paths(input, task_id)?.unwrap_or_default();
        if code_write_scope.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Some(Vec::new()));
        }
        let mut paths = code_write_scope.unwrap_or(declared_paths);
        paths.extend(plugin_keys.into_iter().map(plugin_runtime_lock_path));
        paths.sort();
        paths.dedup();
        return Ok(Some(paths));
    }
    Ok(code_write_scope)
}

fn task_enables_code_write(input: &Value) -> bool {
    task_enables_builtin_kind(input, "CodeMaintainerWrite")
}

fn task_enables_terminal(input: &Value) -> bool {
    task_enables_builtin_kind(input, "TerminalController")
}

fn task_selected_plugin_keys(input: &Value, task_id: &str) -> Result<Vec<String>, String> {
    let Some(value) = input.pointer("/tool_options/plugin_hints") else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let values = value
        .as_array()
        .ok_or_else(|| format!("plugin_hints must be an array for task: {task_id}"))?;
    let mut keys = Vec::with_capacity(values.len());
    let mut unique = HashSet::new();
    for value in values {
        let key = value
            .get("plugin_key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty() && !key.contains('\0'))
            .ok_or_else(|| {
                format!("plugin_hints must contain plugin_key strings for task: {task_id}")
            })?;
        if unique.insert(key) {
            keys.push(key.to_string());
        }
    }
    keys.sort();
    Ok(keys)
}

fn task_selects_external_mcp(input: &Value, task_id: &str) -> Result<bool, String> {
    let Some(value) = input.pointer("/tool_options/external_mcp_config_ids") else {
        return Ok(false);
    };
    if value.is_null() {
        return Ok(false);
    }
    let values = value
        .as_array()
        .ok_or_else(|| format!("external_mcp_config_ids must be an array for task: {task_id}"))?;
    Ok(!values.is_empty())
}

fn plugin_runtime_lock_path(plugin_key: String) -> String {
    let encoded = plugin_key
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(".chatos-runtime/plugin/{encoded}")
}

fn task_enables_builtin_kind(input: &Value, expected: &str) -> bool {
    input
        .pointer("/tool_options/enabled_builtin_kinds")
        .and_then(Value::as_array)
        .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some(expected)))
}

fn validate_owned_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > LOCAL_TASK_MAX_OWNED_PATH_BYTES || path.trim() != path {
        return Err(format!(
            "owned path must be 1..={LOCAL_TASK_MAX_OWNED_PATH_BYTES} bytes without surrounding whitespace"
        ));
    }
    let first_component = path.split('/').next().unwrap_or_default();
    let windows_drive = first_component.as_bytes().get(1) == Some(&b':')
        && first_component
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.starts_with('~')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        || windows_drive
    {
        return Err("owned path must be a normalized project-relative path".to_string());
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

pub fn local_task_write_scope_overlap(left: &[String], right: &[String]) -> Option<String> {
    if left.is_empty() || right.is_empty() {
        return Some("<entire-project>".to_string());
    }
    // Default macOS volumes alias case and Unicode normalization variants. Normalize each
    // scope once, rather than once per pair, and preserve the original model paths.
    let keys = |paths: &[String]| -> Vec<String> {
        paths
            .iter()
            .map(|path| path.nfd().flat_map(char::to_lowercase).collect())
            .collect()
    };
    let left_keys = keys(left);
    let right_keys = keys(right);
    for (left_path, left_key) in left.iter().zip(&left_keys) {
        for (right_path, right_key) in right.iter().zip(&right_keys) {
            if left_key == right_key
                || is_path_ancestor(left_key, right_key)
                || is_path_ancestor(right_key, left_key)
            {
                return Some(if left_path.len() <= right_path.len() {
                    left_path.clone()
                } else {
                    right_path.clone()
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod ownership_alias_tests {
    use super::local_task_write_scope_overlap;

    #[test]
    fn write_locks_cover_case_and_unicode_normalization_aliases() {
        for (left, right) in [
            ("Sources", "sources/app.swift"),
            ("assets/Caf\u{e9}", "ASSETS/Cafe\u{301}/image.png"),
        ] {
            assert!(local_task_write_scope_overlap(&[left.into()], &[right.into()]).is_some());
        }
        assert!(local_task_write_scope_overlap(&["src".into()], &["src-other".into()]).is_none());
    }
}

fn is_path_ancestor(ancestor: &str, descendant: &str) -> bool {
    descendant
        .strip_prefix(ancestor)
        .is_some_and(|suffix| suffix.starts_with('/'))
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
mod tests {
    use super::*;
    use serde_json::json;

    fn record(status: LocalTaskStatus) -> LocalTaskRecord {
        LocalTaskRecord {
            graph_id: "graph-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            task_id: format!("task-{}", status.as_str()),
            title: "Task".to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: Value::Null,
            max_iterations: 4,
            status,
            active_run_id: None,
            result_summary: None,
            version: 1,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    fn task(task_id: &str) -> LocalTaskSpec {
        LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"prompt": task_id}),
            max_iterations: 8,
        }
    }

    fn command(dependencies: Vec<LocalTaskDependency>) -> CreateTaskGraphCommand {
        CreateTaskGraphCommand {
            graph_id: "graph-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![task("task-1"), task("task-2")],
            dependencies,
        }
    }

    fn write_task(task_id: &str, owned_paths: &[&str]) -> LocalTaskSpec {
        let mut value = task(task_id);
        value.input = json!({
            "prompt": task_id,
            "owned_paths": owned_paths,
            "tool_options": {
                "enabled_builtin_kinds": ["CodeMaintainerWrite"]
            }
        });
        value
    }

    fn terminal_task(task_id: &str) -> LocalTaskSpec {
        let mut value = task(task_id);
        value.input = json!({
            "prompt": task_id,
            "tool_options": {
                "enabled_builtin_kinds": ["TerminalController"]
            }
        });
        value
    }

    #[test]
    fn validates_acyclic_graph_and_rejects_cycle() {
        assert!(command(vec![LocalTaskDependency {
            task_id: "task-2".to_string(),
            prerequisite_task_id: "task-1".to_string(),
        }])
        .validate()
        .is_ok());
        assert!(command(vec![
            LocalTaskDependency {
                task_id: "task-2".to_string(),
                prerequisite_task_id: "task-1".to_string(),
            },
            LocalTaskDependency {
                task_id: "task-1".to_string(),
                prerequisite_task_id: "task-2".to_string(),
            },
        ])
        .validate()
        .is_err());
    }

    #[test]
    fn rejects_unordered_overlapping_writers_and_accepts_safe_ownership() {
        let graph = |left: &[&str], right: &[&str], ordered: bool| CreateTaskGraphCommand {
            graph_id: "graph-write".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![write_task("writer-a", left), write_task("writer-b", right)],
            dependencies: ordered
                .then(|| LocalTaskDependency {
                    task_id: "writer-b".to_string(),
                    prerequisite_task_id: "writer-a".to_string(),
                })
                .into_iter()
                .collect(),
        };

        assert!(graph(&["clients/macos"], &["clients/windows"], false)
            .validate()
            .is_ok());
        let overlap = graph(&["clients"], &["clients/macos/App.swift"], false)
            .validate()
            .expect_err("ancestor paths must conflict");
        assert!(overlap.contains("overlapping owned_paths"));
        assert!(graph(&[], &["clients/windows"], false).validate().is_err());
        assert!(graph(&["clients"], &["clients/macos"], true)
            .validate()
            .is_ok());
    }

    #[test]
    fn terminal_is_a_runtime_only_entire_project_writer() {
        let terminal = terminal_task("terminal");
        assert_eq!(
            local_task_write_scope(&terminal.input, &terminal.task_id).expect("static scope"),
            None
        );
        assert_eq!(
            local_task_runtime_write_scope(&terminal.input, &terminal.task_id)
                .expect("runtime scope"),
            Some(Vec::new())
        );
        let mut terminal_with_narrow_code_ownership = write_task("mixed", &["clients/macos"]);
        terminal_with_narrow_code_ownership.input["tool_options"]["enabled_builtin_kinds"] =
            json!(["CodeMaintainerWrite", "TerminalController"]);
        assert_eq!(
            local_task_runtime_write_scope(
                &terminal_with_narrow_code_ownership.input,
                &terminal_with_narrow_code_ownership.task_id,
            )
            .expect("mixed runtime scope"),
            Some(Vec::new())
        );

        let graph = CreateTaskGraphCommand {
            graph_id: "graph-terminal".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![terminal.clone(), terminal_task("terminal-2")],
            dependencies: Vec::new(),
        };
        assert!(graph.validate().is_ok());
    }

    #[test]
    fn plugins_lock_their_own_runtime_and_external_mcp_locks_the_project() {
        let capability_task = |task_id: &str, tool_options: Value| LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({
                "objective": format!("Complete {task_id}"),
                "tool_options": tool_options
            }),
            max_iterations: 4,
        };
        let document = capability_task(
            "document",
            json!({
                "enabled_builtin_kinds": [],
                "plugin_hints": [{"plugin_key": "document"}],
                "external_mcp_config_ids": []
            }),
        );
        let browser = capability_task(
            "browser",
            json!({
                "enabled_builtin_kinds": [],
                "plugin_hints": [{"plugin_key": "browser"}],
                "external_mcp_config_ids": []
            }),
        );
        let document_scope = local_task_runtime_write_scope(&document.input, &document.task_id)
            .expect("document runtime scope")
            .expect("document lock");
        let browser_scope = local_task_runtime_write_scope(&browser.input, &browser.task_id)
            .expect("browser runtime scope")
            .expect("browser lock");
        assert_eq!(document_scope.len(), 1);
        assert_eq!(browser_scope.len(), 1);
        assert!(local_task_write_scope_overlap(&document_scope, &browser_scope).is_none());

        let another_document = capability_task(
            "document-2",
            json!({
                "enabled_builtin_kinds": [],
                "plugin_hints": [{"plugin_key": "document"}],
                "external_mcp_config_ids": []
            }),
        );
        let another_document_scope =
            local_task_runtime_write_scope(&another_document.input, &another_document.task_id)
                .expect("second document runtime scope")
                .expect("second document lock");
        assert!(local_task_write_scope_overlap(&document_scope, &another_document_scope).is_some());

        let external = capability_task(
            "external",
            json!({
                "enabled_builtin_kinds": [],
                "plugin_hints": [],
                "external_mcp_config_ids": ["external-1"]
            }),
        );
        for task in [&document, &browser, &external] {
            assert_eq!(
                local_task_write_scope(&task.input, &task.task_id).expect("static scope"),
                None
            );
        }
        assert_eq!(
            local_task_runtime_write_scope(&external.input, &external.task_id)
                .expect("external runtime scope"),
            Some(Vec::new())
        );

        let graph = CreateTaskGraphCommand {
            graph_id: "graph-dynamic-platform".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![document, browser, external],
            dependencies: Vec::new(),
        };
        assert!(graph.validate().is_ok());

        let malformed = capability_task("malformed", json!({"plugin_hints": "document"}));
        assert!(local_task_runtime_write_scope(&malformed.input, &malformed.task_id).is_err());
        for options in [
            json!({"plugin_hints": [{"plugin_key": "document"}], "external_mcp_config_ids": "bad"}),
            json!({"enabled_builtin_kinds": ["TerminalController"], "plugin_hints": "bad"}),
        ] {
            let malformed = capability_task("malformed-selection", options);
            assert!(local_task_runtime_write_scope(&malformed.input, &malformed.task_id).is_err());
        }
    }

    #[test]
    fn rejects_non_normalized_owned_paths() {
        for path in [
            "/tmp/file",
            "../secret",
            "clients//macos",
            "C:/repo/file",
            "C:repo/file",
            "~/repo/file",
            "a\\b",
        ] {
            let graph = CreateTaskGraphCommand {
                graph_id: "graph-invalid-path".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-1".to_string(),
                tasks: vec![write_task("writer", &[path])],
                dependencies: vec![],
            };
            assert!(graph.validate().is_err(), "path should be rejected: {path}");
        }
    }

    #[test]
    fn notification_policy_defaults_by_graph_shape_and_requires_consistency() {
        assert_eq!(
            LocalTaskGraphNotificationPolicy::default(),
            LocalTaskGraphNotificationPolicy::ProgressSummary
        );
        assert_eq!(
            "progress_summary"
                .parse::<LocalTaskGraphNotificationPolicy>()
                .unwrap()
                .as_str(),
            "progress_summary"
        );
        let single = vec![task("only")];
        assert_eq!(
            resolve_local_task_graph_notification_policy(
                single.iter().map(|task| &task.input),
                single.len(),
            )
            .expect("single policy"),
            LocalTaskGraphNotificationPolicy::AllTasks
        );
        let mut multiple = vec![task("first"), task("second")];
        assert_eq!(
            resolve_local_task_graph_notification_policy(
                multiple.iter().map(|task| &task.input),
                multiple.len(),
            )
            .expect("batch policy"),
            LocalTaskGraphNotificationPolicy::GraphSummary
        );
        for task in &mut multiple {
            task.input["graph_notification_policy"] = json!("milestones");
            task.input["notification_milestone"] = json!(task.task_id == "second");
        }
        assert_eq!(
            resolve_local_task_graph_notification_policy(
                multiple.iter().map(|task| &task.input),
                multiple.len(),
            )
            .expect("explicit policy"),
            LocalTaskGraphNotificationPolicy::Milestones
        );
        multiple[1].input["graph_notification_policy"] = json!("all_tasks");
        let inconsistent = CreateTaskGraphCommand {
            graph_id: "graph-notifications".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: multiple,
            dependencies: Vec::new(),
        };
        assert!(inconsistent.validate().is_err());
    }

    #[test]
    fn final_delivery_requires_one_boolean_sink_covering_the_graph() {
        let mut graph = CreateTaskGraphCommand {
            graph_id: "delivery-graph".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![task("a"), task("b"), task("final")],
            dependencies: vec![
                LocalTaskDependency {
                    task_id: "b".to_string(),
                    prerequisite_task_id: "a".to_string(),
                },
                LocalTaskDependency {
                    task_id: "final".to_string(),
                    prerequisite_task_id: "b".to_string(),
                },
            ],
        };
        graph.tasks[2].input["final_delivery"] = json!(true);
        assert!(graph.validate().is_ok()); // Transitive coverage counts.
        graph.dependencies.pop();
        assert!(graph.validate().unwrap_err().contains("depend on every"));
        graph.tasks[1].input["final_delivery"] = json!(true);
        assert!(graph.validate().unwrap_err().contains("only one"));
        graph.tasks[1].input["final_delivery"] = json!("true");
        assert!(graph.validate().unwrap_err().contains("boolean"));
    }

    #[test]
    fn derives_task_graph_lifecycle_status() {
        let derive = |statuses: &[LocalTaskStatus]| {
            LocalTaskGraphStatus::derive(&statuses.iter().copied().map(record).collect::<Vec<_>>())
        };
        assert_eq!(
            derive(&[LocalTaskStatus::Ready, LocalTaskStatus::Pending]),
            LocalTaskGraphStatus::Pending
        );
        assert_eq!(
            derive(&[LocalTaskStatus::Running, LocalTaskStatus::Pending]),
            LocalTaskGraphStatus::Running
        );
        assert_eq!(
            derive(&[LocalTaskStatus::Succeeded, LocalTaskStatus::Ready]),
            LocalTaskGraphStatus::Running
        );
        assert_eq!(
            derive(&[LocalTaskStatus::Succeeded, LocalTaskStatus::Succeeded]),
            LocalTaskGraphStatus::Succeeded
        );
        assert_eq!(
            derive(&[LocalTaskStatus::Failed, LocalTaskStatus::Blocked]),
            LocalTaskGraphStatus::Failed
        );
        assert_eq!(
            derive(&[LocalTaskStatus::Cancelled, LocalTaskStatus::Blocked]),
            LocalTaskGraphStatus::Cancelled
        );
    }

    #[test]
    fn validates_task_run_page_limit() {
        assert!(GetTaskRunsCommand {
            owner_user_id: "user-1".to_string(),
            task_id: "task-1".to_string(),
            limit: 100,
        }
        .validate()
        .is_ok());
        assert!(GetTaskRunsCommand {
            owner_user_id: "user-1".to_string(),
            task_id: "task-1".to_string(),
            limit: 0,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validates_task_graph_list_cursor_and_owner() {
        let valid = ListTaskGraphsCommand {
            owner_user_id: "user-1".to_string(),
            scope: LocalTaskGraphListScope::Active,
            source_entity_type: Some("conversation_turn".to_string()),
            source_entity_id: Some("turn-1".to_string()),
            before_updated_at_unix_ms: Some(1_000),
            before_graph_id: Some("graph-1".to_string()),
            limit: 25,
        };
        assert!(valid.validate().is_ok());
        assert!(ListTaskGraphsCommand {
            before_graph_id: None,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListTaskGraphsCommand {
            owner_user_id: String::new(),
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListTaskGraphsCommand {
            source_entity_id: None,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListTaskGraphsCommand {
            limit: 101,
            ..valid
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validates_force_restart_reason_and_version() {
        assert!(RestartTaskCommand {
            owner_user_id: "user-1".to_string(),
            task_id: "task-1".to_string(),
            expected_version: 2,
            reason: "rerun with fresh outputs".to_string(),
        }
        .validate()
        .is_ok());
        assert!(RestartTaskCommand {
            owner_user_id: "user-1".to_string(),
            task_id: "task-1".to_string(),
            expected_version: 0,
            reason: "rerun".to_string(),
        }
        .validate()
        .is_err());
        assert!(RestartTaskCommand {
            owner_user_id: "user-1".to_string(),
            task_id: "task-1".to_string(),
            expected_version: 2,
            reason: " ".to_string(),
        }
        .validate()
        .is_err());
    }
}
