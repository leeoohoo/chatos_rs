// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_CONFIRMED_PROJECT_PATHS: usize = 128;
const MAX_CONFIRMED_VALIDATION_COMMANDS: usize = 128;
const MAX_CONFIRMED_ACCEPTANCE_TOOLS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutionReviewPolicy {
    pub read_only_iterations: usize,
    pub missing_read_failures: usize,
    pub repeat_interval_iterations: usize,
}

impl TaskExecutionReviewPolicy {
    pub fn new(
        read_only_iterations: usize,
        missing_read_failures: usize,
        repeat_interval_iterations: usize,
    ) -> Self {
        Self {
            read_only_iterations: read_only_iterations.max(1),
            missing_read_failures: missing_read_failures.max(1),
            repeat_interval_iterations: repeat_interval_iterations.max(1),
        }
    }
}

impl Default for TaskExecutionReviewPolicy {
    fn default() -> Self {
        Self::new(8, 2, 8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionReviewTrigger {
    ReadOnlyLoop,
    MissingTargetedReads,
    PlaceholderProgressWrite,
    StaleProjectWrite,
}

impl TaskExecutionReviewTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnlyLoop => "read_only_loop",
            Self::MissingTargetedReads => "missing_targeted_reads",
            Self::PlaceholderProgressWrite => "placeholder_progress_write",
            Self::StaleProjectWrite => "stale_project_write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutionReviewCheckpoint {
    pub iteration: usize,
    pub trigger: TaskExecutionReviewTrigger,
    pub read_only_iterations: usize,
    pub missing_read_failures: usize,
    pub checkpoints_since_action: usize,
    pub policy: TaskExecutionReviewPolicy,
}

pub struct TaskExecutionProgressState {
    policy: TaskExecutionReviewPolicy,
    current_iteration: AtomicUsize,
    last_meaningful_action_iteration: AtomicUsize,
    last_review_iteration: AtomicUsize,
    checkpoints_since_action: AtomicUsize,
    project_mutation_generation: AtomicUsize,
    last_validated_generation: AtomicUsize,
    missing_targeted_read_failures_after_action: AtomicUsize,
    placeholder_progress_write_iteration: AtomicUsize,
    stale_project_write_failure_iteration: AtomicUsize,
    confirmed_project_paths: Mutex<BTreeSet<String>>,
    confirmed_validation_commands: Mutex<BTreeSet<String>>,
    confirmed_acceptance_tools: Mutex<BTreeSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutionProgressSnapshot {
    pub policy: TaskExecutionReviewPolicy,
    pub current_iteration: usize,
    pub last_meaningful_action_iteration: usize,
    pub last_review_iteration: usize,
    pub checkpoints_since_action: usize,
    pub project_mutation_generation: usize,
    pub last_validated_generation: Option<usize>,
    pub missing_targeted_read_failures_after_action: usize,
    pub placeholder_progress_write_iteration: usize,
    pub stale_project_write_failure_iteration: usize,
    pub confirmed_project_paths: Vec<String>,
    #[serde(default)]
    pub confirmed_validation_commands: Vec<String>,
    #[serde(default)]
    pub confirmed_acceptance_tools: Vec<String>,
}

impl Default for TaskExecutionProgressState {
    fn default() -> Self {
        Self::new(TaskExecutionReviewPolicy::default())
    }
}

impl TaskExecutionProgressState {
    pub fn new(policy: TaskExecutionReviewPolicy) -> Self {
        Self {
            policy,
            current_iteration: AtomicUsize::new(0),
            last_meaningful_action_iteration: AtomicUsize::new(0),
            last_review_iteration: AtomicUsize::new(0),
            checkpoints_since_action: AtomicUsize::new(0),
            project_mutation_generation: AtomicUsize::new(0),
            last_validated_generation: AtomicUsize::new(usize::MAX),
            missing_targeted_read_failures_after_action: AtomicUsize::new(0),
            placeholder_progress_write_iteration: AtomicUsize::new(0),
            stale_project_write_failure_iteration: AtomicUsize::new(0),
            confirmed_project_paths: Mutex::new(BTreeSet::new()),
            confirmed_validation_commands: Mutex::new(BTreeSet::new()),
            confirmed_acceptance_tools: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn policy(&self) -> TaskExecutionReviewPolicy {
        self.policy
    }

    pub fn snapshot(&self) -> TaskExecutionProgressSnapshot {
        TaskExecutionProgressSnapshot {
            policy: self.policy,
            current_iteration: self.current_iteration.load(Ordering::Relaxed),
            last_meaningful_action_iteration: self
                .last_meaningful_action_iteration
                .load(Ordering::Relaxed),
            last_review_iteration: self.last_review_iteration.load(Ordering::Relaxed),
            checkpoints_since_action: self.checkpoints_since_action.load(Ordering::Relaxed),
            project_mutation_generation: self.project_mutation_generation.load(Ordering::Relaxed),
            last_validated_generation: match self.last_validated_generation.load(Ordering::Relaxed)
            {
                usize::MAX => None,
                generation => Some(generation),
            },
            missing_targeted_read_failures_after_action: self
                .missing_targeted_read_failures_after_action
                .load(Ordering::Relaxed),
            placeholder_progress_write_iteration: self
                .placeholder_progress_write_iteration
                .load(Ordering::Relaxed),
            stale_project_write_failure_iteration: self
                .stale_project_write_failure_iteration
                .load(Ordering::Relaxed),
            confirmed_project_paths: self.confirmed_project_paths(),
            confirmed_validation_commands: self.confirmed_validation_commands(),
            confirmed_acceptance_tools: self.confirmed_acceptance_tools(),
        }
    }

    pub fn restore_snapshot(&self, snapshot: &TaskExecutionProgressSnapshot) {
        self.current_iteration
            .store(snapshot.current_iteration, Ordering::Relaxed);
        self.last_meaningful_action_iteration
            .store(snapshot.last_meaningful_action_iteration, Ordering::Relaxed);
        self.last_review_iteration
            .store(snapshot.last_review_iteration, Ordering::Relaxed);
        self.checkpoints_since_action
            .store(snapshot.checkpoints_since_action, Ordering::Relaxed);
        self.project_mutation_generation
            .store(snapshot.project_mutation_generation, Ordering::Relaxed);
        self.last_validated_generation.store(
            snapshot.last_validated_generation.unwrap_or(usize::MAX),
            Ordering::Relaxed,
        );
        self.missing_targeted_read_failures_after_action.store(
            snapshot.missing_targeted_read_failures_after_action,
            Ordering::Relaxed,
        );
        self.placeholder_progress_write_iteration.store(
            snapshot.placeholder_progress_write_iteration,
            Ordering::Relaxed,
        );
        self.stale_project_write_failure_iteration.store(
            snapshot.stale_project_write_failure_iteration,
            Ordering::Relaxed,
        );
        if let Ok(mut paths) = self.confirmed_project_paths.lock() {
            paths.clear();
            paths.extend(
                snapshot
                    .confirmed_project_paths
                    .iter()
                    .take(MAX_CONFIRMED_PROJECT_PATHS)
                    .cloned(),
            );
        }
        if let Ok(mut commands) = self.confirmed_validation_commands.lock() {
            commands.clear();
            commands.extend(
                snapshot
                    .confirmed_validation_commands
                    .iter()
                    .take(MAX_CONFIRMED_VALIDATION_COMMANDS)
                    .cloned(),
            );
        }
        if let Ok(mut tools) = self.confirmed_acceptance_tools.lock() {
            tools.clear();
            tools.extend(
                snapshot
                    .confirmed_acceptance_tools
                    .iter()
                    .take(MAX_CONFIRMED_ACCEPTANCE_TOOLS)
                    .cloned(),
            );
        }
    }

    pub fn begin_iteration(&self, iteration: usize) {
        self.current_iteration.store(iteration, Ordering::Relaxed);
    }

    pub fn observe_tool_result(&self, payload: &Value) {
        self.record_confirmed_project_paths(payload);
        self.record_confirmed_acceptance_tool(payload);
        let iteration = self.current_iteration.load(Ordering::Relaxed);
        if tool_result_is_project_mutation(payload) {
            self.project_mutation_generation
                .fetch_add(1, Ordering::Relaxed);
            self.record_meaningful_action(iteration);
            return;
        }
        if tool_result_is_validation(payload) {
            let command = terminal_result_command(payload);
            if !command.is_empty() {
                if let Ok(mut commands) = self.confirmed_validation_commands.lock() {
                    if commands.len() < MAX_CONFIRMED_VALIDATION_COMMANDS {
                        commands.insert(command);
                    }
                }
            }
            if self.record_validation_for_current_generation() {
                self.record_meaningful_action(iteration);
            }
            return;
        }

        if tool_result_is_placeholder_progress_write(payload) {
            self.placeholder_progress_write_iteration
                .store(iteration, Ordering::Relaxed);
        }
        if tool_result_is_stale_project_write_failure(payload) {
            self.stale_project_write_failure_iteration
                .store(iteration, Ordering::Relaxed);
        }
        if tool_result_is_missing_targeted_read(payload) {
            self.missing_targeted_read_failures_after_action
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn confirmed_project_paths(&self) -> Vec<String> {
        self.confirmed_project_paths
            .lock()
            .map(|paths| paths.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn confirmed_validation_commands(&self) -> Vec<String> {
        self.confirmed_validation_commands
            .lock()
            .map(|commands| commands.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn confirmed_acceptance_tools(&self) -> Vec<String> {
        self.confirmed_acceptance_tools
            .lock()
            .map(|tools| tools.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn record_confirmed_project_paths(&self, payload: &Value) {
        let paths = confirmed_project_paths_from_tool_result(payload);
        if paths.is_empty() {
            return;
        }
        let Ok(mut confirmed) = self.confirmed_project_paths.lock() else {
            return;
        };
        for path in paths {
            if confirmed.len() >= MAX_CONFIRMED_PROJECT_PATHS {
                break;
            }
            confirmed.insert(path);
        }
    }

    fn record_confirmed_acceptance_tool(&self, payload: &Value) {
        if payload.get("success").and_then(Value::as_bool) != Some(true)
            || payload.get("is_error").and_then(Value::as_bool) == Some(true)
        {
            return;
        }
        let Some(name) = payload.get("name").and_then(Value::as_str) else {
            return;
        };
        if !name.contains("browser_tools") {
            return;
        }
        if let Ok(mut tools) = self.confirmed_acceptance_tools.lock() {
            if tools.len() < MAX_CONFIRMED_ACCEPTANCE_TOOLS {
                tools.insert(name.to_string());
            }
        }
    }

    fn record_meaningful_action(&self, iteration: usize) {
        self.last_meaningful_action_iteration
            .store(iteration, Ordering::Relaxed);
        self.checkpoints_since_action.store(0, Ordering::Relaxed);
        self.missing_targeted_read_failures_after_action
            .store(0, Ordering::Relaxed);
        self.stale_project_write_failure_iteration
            .store(0, Ordering::Relaxed);
    }

    fn record_validation_for_current_generation(&self) -> bool {
        let generation = self.project_mutation_generation.load(Ordering::Relaxed);
        self.last_validated_generation
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last_validated| {
                (last_validated != generation).then_some(generation)
            })
            .is_ok()
    }

    pub fn should_trigger_review(&self, iteration: usize) -> Option<TaskExecutionReviewCheckpoint> {
        let last_action = self
            .last_meaningful_action_iteration
            .load(Ordering::Relaxed);
        let read_only_iterations = iteration.saturating_sub(last_action);
        let missing_read_failures = self
            .missing_targeted_read_failures_after_action
            .load(Ordering::Relaxed);
        let placeholder_iteration = self
            .placeholder_progress_write_iteration
            .load(Ordering::Relaxed);
        let stale_write_iteration = self
            .stale_project_write_failure_iteration
            .load(Ordering::Relaxed);
        let last_review = self.last_review_iteration.load(Ordering::Relaxed);

        let trigger = if placeholder_iteration > 0 && placeholder_iteration > last_review {
            Some(TaskExecutionReviewTrigger::PlaceholderProgressWrite)
        } else if stale_write_iteration > 0 && stale_write_iteration > last_review {
            Some(TaskExecutionReviewTrigger::StaleProjectWrite)
        } else if missing_read_failures >= self.policy.missing_read_failures {
            Some(TaskExecutionReviewTrigger::MissingTargetedReads)
        } else if read_only_iterations >= self.policy.read_only_iterations {
            Some(TaskExecutionReviewTrigger::ReadOnlyLoop)
        } else {
            None
        }?;

        if last_review > 0
            && iteration.saturating_sub(last_review) < self.policy.repeat_interval_iterations
        {
            return None;
        }

        self.last_review_iteration
            .compare_exchange(last_review, iteration, Ordering::Relaxed, Ordering::Relaxed)
            .ok()
            .map(|_| {
                let checkpoints_since_action = self
                    .checkpoints_since_action
                    .fetch_add(1, Ordering::Relaxed)
                    + 1;
                TaskExecutionReviewCheckpoint {
                    iteration,
                    trigger,
                    read_only_iterations,
                    missing_read_failures,
                    checkpoints_since_action,
                    policy: self.policy,
                }
            })
    }
}

pub fn tool_result_is_stale_project_write_failure(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) == Some(true)
        && payload.get("is_error").and_then(Value::as_bool) != Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if !tool_name_ends_with_any(name, &["stage_edit_batch", "commit_edit_session"]) {
        return false;
    }
    let evidence = payload.to_string().to_ascii_lowercase();
    [
        "stale_context",
        "expected_match",
        "patch context not found",
        "expected_matches mismatch",
        "file content likely changed",
        "patch context is stale",
    ]
    .iter()
    .any(|needle| evidence.contains(needle))
}

pub fn tool_result_is_meaningful_engineering_action(payload: &Value) -> bool {
    tool_result_is_project_mutation(payload) || tool_result_is_validation(payload)
}

fn tool_result_is_project_mutation(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if tool_name_ends_with_any(name, &["commit_edit_session"]) {
        return write_result_has_meaningful_project_path(payload);
    }
    false
}

fn tool_result_is_validation(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    name.ends_with("terminal_controller_execute_command")
        && terminal_result_exit_succeeded(payload)
        && terminal_result_has_validation_command(payload)
}

pub fn tool_result_is_missing_targeted_read(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) == Some(true)
        && payload.get("is_error").and_then(Value::as_bool) != Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if !targeted_read_tool_name(name) {
        return false;
    }
    let mut text = String::new();
    collect_tool_result_error_text(payload, &mut text);
    let normalized = text.to_ascii_lowercase();
    [
        "no such file",
        "not found",
        "cannot find",
        "can't find",
        "could not find",
        "does not exist",
        "enoent",
        "os error 2",
        "不存在",
        "找不到",
        "未找到",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub fn tool_result_is_placeholder_progress_write(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if !tool_name_ends_with_any(name, &["commit_edit_session"]) {
        return false;
    }
    let parsed_content = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok());
    payload
        .get("result")
        .into_iter()
        .chain(parsed_content.as_ref())
        .any(value_contains_placeholder_progress_path)
}

fn targeted_read_tool_name(name: &str) -> bool {
    tool_name_ends_with_any(name, &["read_file_raw", "read_file_range", "read_file"])
}

fn confirmed_project_paths_from_tool_result(payload: &Value) -> Vec<String> {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return Vec::new();
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return Vec::new();
    };
    if name.ends_with("terminal_controller_execute_command") {
        if !terminal_result_exit_succeeded(payload) {
            return Vec::new();
        }
        let command = terminal_result_command(payload);
        let mut paths = BTreeSet::new();
        if command.starts_with("npm ") || command == "npm" {
            paths.insert("package.json".to_string());
            if command.contains(" ci") || command.contains(" install") || command.contains(" audit")
            {
                // npm resolves these files from the current workspace. A successful install,
                // ci, or audit therefore proves the package baseline exists even when the model
                // did not issue a separate read/list call for package-lock.json.
                paths.insert("package-lock.json".to_string());
            }
        }
        return paths
            .into_iter()
            .filter(|path| project_path_is_meaningful_progress(path))
            .collect();
    }
    if !tool_name_ends_with_any(
        name,
        &[
            "list_dir",
            "read_file_raw",
            "read_file_range",
            "read_file",
            "search_text",
            "search_files",
            "stage_edit_batch",
            "commit_edit_session",
        ],
    ) {
        return Vec::new();
    }
    let parsed_content = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok());
    let mut paths = BTreeSet::new();
    collect_confirmed_project_paths(payload, &mut paths);
    if let Some(content) = parsed_content.as_ref() {
        collect_confirmed_project_paths(content, &mut paths);
    }
    paths
        .into_iter()
        .take(MAX_CONFIRMED_PROJECT_PATHS)
        .collect()
}

fn collect_confirmed_project_paths(value: &Value, output: &mut BTreeSet<String>) {
    if output.len() >= MAX_CONFIRMED_PROJECT_PATHS {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "path" | "file" | "filename" | "relative_path" | "changed_paths"
                ) {
                    collect_path_values(value, output);
                } else if value.is_object() || value.is_array() {
                    collect_confirmed_project_paths(value, output);
                }
                if output.len() >= MAX_CONFIRMED_PROJECT_PATHS {
                    break;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_confirmed_project_paths(item, output);
                if output.len() >= MAX_CONFIRMED_PROJECT_PATHS {
                    break;
                }
            }
        }
        _ => {}
    }
}

fn collect_path_values(value: &Value, output: &mut BTreeSet<String>) {
    match value {
        Value::String(path) => {
            if let Some(path) = normalize_confirmed_project_path(path) {
                output.insert(path);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_path_values(item, output);
            }
        }
        Value::Object(_) => collect_confirmed_project_paths(value, output),
        _ => {}
    }
}

fn normalize_confirmed_project_path(path: &str) -> Option<String> {
    let normalized = path
        .trim()
        .trim_matches('`')
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.chars().count() > 512
        || normalized.as_bytes().get(1) == Some(&b':')
        || normalized
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        || !project_path_is_meaningful_progress(normalized.as_str())
    {
        return None;
    }
    Some(normalized)
}

fn tool_name_ends_with_any(name: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| name.ends_with(suffix))
}

fn collect_tool_result_error_text(value: &Value, output: &mut String) {
    match value {
        Value::String(text) => {
            output.push(' ');
            output.push_str(text);
        }
        Value::Array(items) => {
            for item in items {
                collect_tool_result_error_text(item, output);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "content"
                        | "result"
                        | "error"
                        | "message"
                        | "detail"
                        | "details"
                        | "path"
                        | "file"
                        | "filename"
                ) {
                    collect_tool_result_error_text(value, output);
                }
            }
        }
        _ => {}
    }
}

fn write_result_has_meaningful_project_path(payload: &Value) -> bool {
    let parsed_content = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok());
    payload
        .get("result")
        .into_iter()
        .chain(parsed_content.as_ref())
        .any(value_contains_meaningful_project_path)
}

fn value_contains_meaningful_project_path(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            if key == "path" {
                return value
                    .as_str()
                    .is_some_and(project_path_is_meaningful_progress);
            }
            value_contains_meaningful_project_path(value)
        }),
        Value::Array(items) => items.iter().any(value_contains_meaningful_project_path),
        _ => false,
    }
}

fn value_contains_placeholder_progress_path(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            if key == "path" {
                return value
                    .as_str()
                    .is_some_and(|path| !project_path_is_meaningful_progress(path));
            }
            value_contains_placeholder_progress_path(value)
        }),
        Value::Array(items) => items.iter().any(value_contains_placeholder_progress_path),
        _ => false,
    }
}

fn project_path_is_meaningful_progress(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() {
        return false;
    }
    let components = normalized
        .trim_start_matches("./")
        .split('/')
        .filter(|component| !component.is_empty());
    !components
        .into_iter()
        .any(project_path_component_is_non_engineering_progress)
}

fn project_path_component_is_non_engineering_progress(component: &str) -> bool {
    let normalized = component.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        ".chatos" | ".git" | ".cache" | "node_modules" | "target" | "target-shared"
    ) {
        return true;
    }
    [
        "progress-guard",
        "inspection-unlock",
        "read-unlock",
        "unblock",
        "unlock",
        "restore",
        "enable-tools",
        "enable_tools",
        "resume-tools",
        "resume_tools",
        "placeholder",
        "sentinel",
        "probe",
        "task-runner-notes",
        "task_runner_notes",
        "task-runner-progress",
        "task_runner_progress",
        "task_runner_progress_note",
        "task-runner-progress-note",
        "execution-notes",
        "execution_notes",
        "inspection-note",
        "inspection_note",
        "progress-note",
        "progress_note",
        "执行记录",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
        || [
            ["task_runner", "temp"],
            ["task-runner", "temp"],
            ["task_runner", "notes"],
            ["task-runner", "notes"],
            ["temp", "restore"],
        ]
        .iter()
        .any(|markers| markers.iter().all(|marker| normalized.contains(marker)))
}

fn terminal_result_command(payload: &Value) -> String {
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parsed = serde_json::from_str::<Value>(content).ok();
    parsed
        .as_ref()
        .map(chatos_mcp_runtime::structured_result_payload)
        .and_then(|value| value.get("common"))
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("result")
                .map(chatos_mcp_runtime::structured_result_payload)
                .and_then(|value| value.get("common"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn terminal_result_exit_succeeded(payload: &Value) -> bool {
    let direct_exit_code = payload
        .get("result")
        .map(chatos_mcp_runtime::structured_result_payload)
        .and_then(|result| result.get("exit_code"))
        .and_then(Value::as_i64);
    let content_exit_code = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .map(|content| chatos_mcp_runtime::structured_result_payload(&content).clone())
        .and_then(|content| content.get("exit_code").and_then(Value::as_i64));
    direct_exit_code.or(content_exit_code) == Some(0)
}

fn terminal_result_has_validation_command(payload: &Value) -> bool {
    let command = terminal_result_command(payload);
    [
        "cargo test",
        "cargo check",
        "cargo clippy",
        "pytest",
        "python -m unittest",
        "npm test",
        "npm run test",
        "npm run typecheck",
        "npm run build",
        "npm ci",
        "npm install",
        "npm audit",
        "pnpm test",
        "pnpm run test",
        "pnpm run typecheck",
        "pnpm build",
        "pnpm install",
        "pnpm audit",
        "yarn test",
        "yarn run test",
        "yarn run typecheck",
        "yarn build",
        "yarn install",
        "yarn audit",
        "go test",
        "mvn test",
        "gradle test",
        "dotnet test",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn review_triggers_after_repeated_read_only_iterations() {
        let progress = TaskExecutionProgressState::default();

        assert!(progress.should_trigger_review(7).is_none());
        let checkpoint = progress
            .should_trigger_review(8)
            .expect("read-only checkpoint");
        assert_eq!(checkpoint.trigger, TaskExecutionReviewTrigger::ReadOnlyLoop);
        assert_eq!(checkpoint.read_only_iterations, 8);
        assert!(progress.should_trigger_review(9).is_none());
        assert!(progress.should_trigger_review(16).is_some());
    }

    #[test]
    fn missing_targeted_reads_trigger_review_without_restricting_tools() {
        let progress = TaskExecutionProgressState::default();
        for iteration in [1, 2] {
            progress.begin_iteration(iteration);
            progress.observe_tool_result(&json!({
                "name": "code_maintainer_read_read_file_raw",
                "success": false,
                "is_error": true,
                "content": "ENOENT: package.json does not exist",
            }));
        }

        let checkpoint = progress
            .should_trigger_review(3)
            .expect("missing-read checkpoint");
        assert_eq!(
            checkpoint.trigger,
            TaskExecutionReviewTrigger::MissingTargetedReads
        );
        assert_eq!(checkpoint.missing_read_failures, 2);
    }

    #[test]
    fn successful_source_write_resets_missing_read_budget() {
        let progress = TaskExecutionProgressState::default();
        for iteration in [1, 2] {
            progress.begin_iteration(iteration);
            progress.observe_tool_result(&json!({
                "name": "code_maintainer_read_read_file",
                "success": false,
                "is_error": true,
                "content": "README.md not found",
            }));
        }
        assert!(progress.should_trigger_review(3).is_some());

        progress.begin_iteration(4);
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {
                "committed_paths": [{ "path": "src/lib.rs" }],
            },
        }));

        assert!(progress.should_trigger_review(5).is_none());
    }

    #[test]
    fn placeholder_progress_write_triggers_review_and_is_not_meaningful_progress() {
        let payload = json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {
                "committed_paths": [{ "path": "TASK_RUNNER_PROGRESS_NOTE.md" }],
            },
        });
        assert!(!tool_result_is_meaningful_engineering_action(&payload));
        assert!(tool_result_is_placeholder_progress_write(&payload));

        let progress = TaskExecutionProgressState::default();
        progress.begin_iteration(4);
        progress.observe_tool_result(&payload);
        let checkpoint = progress
            .should_trigger_review(5)
            .expect("placeholder checkpoint");
        assert_eq!(
            checkpoint.trigger,
            TaskExecutionReviewTrigger::PlaceholderProgressWrite
        );
    }

    #[test]
    fn missing_targeted_read_detection_supports_harness_prefixed_tools() {
        assert!(tool_result_is_missing_targeted_read(&json!({
            "name": "harness_code_read_file_range",
            "success": false,
            "is_error": true,
            "content": "file not found: src/main.rs",
        })));
        assert!(!tool_result_is_missing_targeted_read(&json!({
            "name": "harness_code_search_text",
            "success": false,
            "is_error": true,
            "content": "not found",
        })));
    }

    #[test]
    fn observation_and_task_bookkeeping_are_not_engineering_progress() {
        for name in [
            "project_runtime_environment_get_project_runtime_environment_info",
            "project_management_update_project_task",
            "task_run_process_record_process",
            "code_maintainer_read_read_file_raw",
        ] {
            assert!(!tool_result_is_meaningful_engineering_action(&json!({
                "name": name,
                "success": true,
                "is_error": false,
            })));
        }
    }

    #[test]
    fn placeholder_paths_are_rejected_but_source_writes_are_meaningful() {
        for path in [
            ".chatos/tmp/inspection-unlock.txt",
            "mdm-service/.progress-guard-placeholder",
            "UNBLOCK.md",
            "src/probe_progress_guard.py",
            "TASK_RUNNER_TEMP_RESTORE.txt",
            "task-runner-temp-unlock.txt",
            "ENABLE_TOOLS_AFTER_WRITE.md",
            "docs/oms-order-entry-task-runner-notes.md",
            "docs/task_runner_execution_notes.md",
        ] {
            let payload = json!({
                "name": "code_maintainer_write_commit_edit_session",
                "success": true,
                "is_error": false,
                "result": { "committed_paths": [{ "path": path }] },
            });
            assert!(!tool_result_is_meaningful_engineering_action(&payload));
            assert!(tool_result_is_placeholder_progress_write(&payload));
        }

        assert!(tool_result_is_meaningful_engineering_action(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "committed_paths": [{ "path": "src/lib.rs" }],
            })).expect("content"),
        })));
    }

    #[test]
    fn targeted_test_command_is_meaningful_progress() {
        assert!(tool_result_is_meaningful_engineering_action(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "python -m unittest discover -s tests -v",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        })));
    }

    #[test]
    fn terminal_file_overwrite_is_not_meaningful_engineering_progress() {
        assert!(!tool_result_is_meaningful_engineering_action(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "python3 -c \"from pathlib import Path; Path('a').write_text('x')\"",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        })));
    }

    #[test]
    fn successful_file_tools_build_a_bounded_confirmed_path_index() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_search_text",
            "success": true,
            "is_error": false,
            "result": {
                "matches": [
                    { "path": "src/domain/index.ts", "line": 4 },
                    { "path": "src/domain/index.ts", "line": 9 },
                    { "path": "../outside.rs", "line": 1 },
                    { "path": "/absolute.rs", "line": 1 },
                    { "path": "target/debug/generated.rs", "line": 1 },
                    { "path": "node_modules/pkg/index.js", "line": 1 }
                ]
            }
        }));
        progress.observe_tool_result(&json!({
            "name": "harness_code_commit_edit_session",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "changed_paths": ["src/domain/index.ts", "src/domain/domain.test.ts"]
            })).expect("content")
        }));

        assert_eq!(
            progress.confirmed_project_paths(),
            vec![
                "src/domain/domain.test.ts".to_string(),
                "src/domain/index.ts".to_string(),
            ]
        );
    }

    #[test]
    fn confirmed_project_path_index_stops_at_managed_capacity() {
        let progress = TaskExecutionProgressState::default();
        let entries = (0..MAX_CONFIRMED_PROJECT_PATHS + 16)
            .map(|index| json!({ "path": format!("src/module_{index}.rs") }))
            .collect::<Vec<_>>();

        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_list_dir",
            "success": true,
            "is_error": false,
            "result": { "entries": entries }
        }));

        assert_eq!(
            progress.confirmed_project_paths().len(),
            MAX_CONFIRMED_PROJECT_PATHS
        );
    }

    #[test]
    fn failed_file_tools_do_not_confirm_paths() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": false,
            "is_error": true,
            "content": "not found",
            "result": { "path": "src/missing.rs" }
        }));

        assert!(progress.confirmed_project_paths().is_empty());
    }

    #[test]
    fn repeated_validation_only_counts_once_without_a_new_mutation() {
        let progress = TaskExecutionProgressState::default();
        let validation = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "npm run build",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&validation);
        progress.begin_iteration(5);
        progress.observe_tool_result(&validation);

        assert!(progress.should_trigger_review(8).is_none());
        let checkpoint = progress
            .should_trigger_review(9)
            .expect("repeated validation must not reset progress");
        assert_eq!(checkpoint.read_only_iterations, 8);
    }

    #[test]
    fn all_successful_validation_commands_are_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        for command in ["npm test", "npm run build"] {
            progress.observe_tool_result(&json!({
                "name": "terminal_controller_execute_command",
                "success": true,
                "is_error": false,
                "content": serde_json::to_string(&json!({
                    "common": command,
                    "exit_code": 0,
                })).expect("content"),
                "result": { "exit_code": 0 },
            }));
        }

        assert_eq!(
            progress.confirmed_validation_commands(),
            vec!["npm run build".to_string(), "npm test".to_string()]
        );
    }

    #[test]
    fn common_node_dependency_and_typecheck_commands_are_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        for command in [
            "npm install --package-lock-only --ignore-scripts --registry=https://registry.npmjs.org",
            "npm ci --ignore-scripts --registry=https://registry.npmjs.org",
            "npm run typecheck",
            "npm audit --audit-level=high --json --registry=https://registry.npmjs.org",
        ] {
            progress.observe_tool_result(&json!({
                "name": "terminal_controller_execute_command",
                "success": true,
                "is_error": false,
                "content": serde_json::to_string(&json!({
                    "common": command,
                    "exit_code": 0,
                })).expect("content"),
                "result": { "exit_code": 0 },
            }));
        }

        assert_eq!(
            progress.confirmed_validation_commands(),
            vec![
                "npm audit --audit-level=high --json --registry=https://registry.npmjs.org".to_string(),
                "npm ci --ignore-scripts --registry=https://registry.npmjs.org".to_string(),
                "npm install --package-lock-only --ignore-scripts --registry=https://registry.npmjs.org".to_string(),
                "npm run typecheck".to_string(),
            ]
        );
        assert_eq!(
            progress.confirmed_project_paths(),
            vec!["package-lock.json".to_string(), "package.json".to_string()]
        );
    }

    #[test]
    fn nested_structured_validation_result_is_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "_structured_result": {
                    "_structured_result": {
                        "common": "cargo test -p example",
                        "exit_code": 0
                    }
                }
            })).expect("content"),
            "result": {
                "_structured_result": {
                    "_structured_result": {
                        "exit_code": 0
                    }
                }
            },
        }));

        assert_eq!(
            progress.confirmed_validation_commands(),
            vec!["cargo test -p example".to_string()]
        );
    }

    #[test]
    fn successful_browser_tools_are_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "browser_tools_browser_snapshot",
            "success": true,
            "is_error": false,
            "result": { "url": "http://127.0.0.1:4173" },
        }));

        assert_eq!(
            progress.confirmed_acceptance_tools(),
            vec!["browser_tools_browser_snapshot".to_string()]
        );
    }

    #[test]
    fn project_mutation_allows_one_new_validation_to_count_as_progress() {
        let progress = TaskExecutionProgressState::default();
        let validation = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "cargo test -p example",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        });
        let mutation = json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {
                "committed_paths": [{ "path": "src/lib.rs" }],
            },
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&validation);
        progress.begin_iteration(8);
        progress.observe_tool_result(&mutation);
        progress.begin_iteration(12);
        progress.observe_tool_result(&validation);

        assert!(progress.should_trigger_review(19).is_none());
        assert!(progress.should_trigger_review(20).is_some());
    }

    #[test]
    fn repeated_checkpoints_track_reviews_since_last_progress() {
        let progress = TaskExecutionProgressState::default();

        let first = progress.should_trigger_review(8).expect("first checkpoint");
        let second = progress
            .should_trigger_review(16)
            .expect("second checkpoint");
        let third = progress
            .should_trigger_review(24)
            .expect("third checkpoint");

        assert_eq!(first.checkpoints_since_action, 1);
        assert_eq!(second.checkpoints_since_action, 2);
        assert_eq!(third.checkpoints_since_action, 3);
    }

    #[test]
    fn failed_validation_command_is_not_progress() {
        let progress = TaskExecutionProgressState::default();
        let failed_build = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "npm run build",
                "exit_code": 127,
            })).expect("content"),
            "result": {
                "common": "npm run build",
                "exit_code": 127,
            },
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&failed_build);

        let checkpoint = progress
            .should_trigger_review(8)
            .expect("failed build must not reset review progress");
        assert_eq!(checkpoint.read_only_iterations, 8);
    }

    #[test]
    fn process_input_is_not_validation_progress() {
        let progress = TaskExecutionProgressState::default();
        let process_write = json!({
            "name": "terminal_controller_process_write",
            "success": true,
            "is_error": false,
            "content": "submitted",
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&process_write);

        assert!(progress.should_trigger_review(8).is_some());
    }

    #[test]
    fn stale_session_write_failure_triggers_actionable_review() {
        let progress = TaskExecutionProgressState::default();
        let stale_patch = json!({
            "name": "code_maintainer_write_stage_edit_batch",
            "success": false,
            "is_error": true,
            "content": "Patch context not found in file. Patch context is stale.",
        });

        progress.begin_iteration(3);
        progress.observe_tool_result(&stale_patch);

        let checkpoint = progress
            .should_trigger_review(4)
            .expect("stale patch failure must trigger review");
        assert_eq!(
            checkpoint.trigger,
            TaskExecutionReviewTrigger::StaleProjectWrite
        );
    }
}
