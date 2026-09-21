// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_CONFIRMED_PROJECT_PATHS: usize = 128;
const MAX_CONFIRMED_VALIDATION_COMMANDS: usize = 128;
const MAX_CONFIRMED_ACCEPTANCE_TOOLS: usize = 128;
const MAX_PENDING_VALIDATION_COMMANDS: usize = 64;
const MAX_PENDING_COMPLETION_REQUIREMENTS: usize = 64;

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
    pending_validation_commands: Mutex<BTreeMap<String, String>>,
    pending_completion_requirements: Mutex<BTreeMap<String, String>>,
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
    #[serde(default)]
    pub pending_validation_commands: BTreeMap<String, String>,
    #[serde(default)]
    pub pending_completion_requirements: BTreeMap<String, String>,
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
            pending_validation_commands: Mutex::new(BTreeMap::new()),
            pending_completion_requirements: Mutex::new(BTreeMap::new()),
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
            pending_validation_commands: self
                .pending_validation_commands
                .lock()
                .map(|commands| commands.clone())
                .unwrap_or_default(),
            pending_completion_requirements: self.pending_completion_requirements(),
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
        if let Ok(mut pending) = self.pending_validation_commands.lock() {
            pending.clear();
            pending.extend(
                snapshot
                    .pending_validation_commands
                    .iter()
                    .take(MAX_PENDING_VALIDATION_COMMANDS)
                    .map(|(process_id, command)| (process_id.clone(), command.clone())),
            );
        }
        if let Ok(mut pending) = self.pending_completion_requirements.lock() {
            pending.clear();
            pending.extend(
                snapshot
                    .pending_completion_requirements
                    .iter()
                    .take(MAX_PENDING_COMPLETION_REQUIREMENTS)
                    .map(|(id, verifier)| (id.clone(), verifier.clone())),
            );
        }
    }

    pub fn begin_iteration(&self, iteration: usize) {
        self.current_iteration.store(iteration, Ordering::Relaxed);
    }

    pub fn observe_tool_result(&self, payload: &Value) {
        self.record_confirmed_project_paths(payload);
        self.record_completion_contract(payload);
        let iteration = self.current_iteration.load(Ordering::Relaxed);
        if tool_result_is_project_mutation(payload) {
            self.project_mutation_generation
                .fetch_add(1, Ordering::Relaxed);
            self.record_meaningful_action(iteration);
            return;
        }
        if let Some(command) = self.validation_command_from_tool_result(payload) {
            if let Ok(mut commands) = self.confirmed_validation_commands.lock() {
                if commands.len() < MAX_CONFIRMED_VALIDATION_COMMANDS {
                    commands.insert(command);
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

    pub fn pending_completion_requirements(&self) -> BTreeMap<String, String> {
        self.pending_completion_requirements
            .lock()
            .map(|requirements| requirements.clone())
            .unwrap_or_default()
    }

    fn record_completion_contract(&self, payload: &Value) {
        if payload.get("success").and_then(Value::as_bool) != Some(true)
            || payload.get("is_error").and_then(Value::as_bool) == Some(true)
        {
            return;
        }
        let tool_name = payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown_tool");
        if let Some(requirement) = tool_result_field(payload, "completionRequirement") {
            let id = requirement
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let verifier = requirement
                .get("verifier")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let (Some(id), Some(verifier), Ok(mut pending)) =
                (id, verifier, self.pending_completion_requirements.lock())
            {
                if pending.len() < MAX_PENDING_COMPLETION_REQUIREMENTS || pending.contains_key(id) {
                    pending.insert(id.to_string(), verifier.to_string());
                }
            }
        }
        if let Some(proof) = tool_result_field(payload, "completionProof") {
            let id = proof
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let verifier = proof
                .get("verifier")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let (Some(id), Some(verifier), Ok(mut pending)) =
                (id, verifier, self.pending_completion_requirements.lock())
            {
                if pending.get(id).is_some_and(|expected| expected == verifier) {
                    pending.remove(id);
                    if let Ok(mut tools) = self.confirmed_acceptance_tools.lock() {
                        if tools.len() < MAX_CONFIRMED_ACCEPTANCE_TOOLS {
                            tools.insert(tool_name.to_string());
                        }
                    }
                }
            }
        }
    }

    fn validation_command_from_tool_result(&self, payload: &Value) -> Option<String> {
        let name = payload.get("name").and_then(Value::as_str)?;
        if name.ends_with("terminal_controller_execute_command") {
            if payload.get("success").and_then(Value::as_bool) != Some(true)
                || payload.get("is_error").and_then(Value::as_bool) == Some(true)
            {
                return None;
            }
            let command = terminal_result_command(payload);
            if !terminal_command_is_validation(command.as_str()) {
                return None;
            }
            if terminal_result_exit_succeeded(payload) {
                return Some(command);
            }
            if terminal_result_is_busy(payload) {
                let process_id = terminal_result_process_id(payload)?;
                if let Ok(mut pending) = self.pending_validation_commands.lock() {
                    if pending.len() < MAX_PENDING_VALIDATION_COMMANDS {
                        pending.insert(process_id, command);
                    }
                }
            }
            return None;
        }
        if !tool_name_ends_with_any(
            name,
            &[
                "terminal_controller_process_wait",
                "terminal_controller_process_poll",
            ],
        ) || terminal_result_is_busy(payload)
        {
            return None;
        }
        let process_id = terminal_result_process_id(payload)?;
        let command = self
            .pending_validation_commands
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(process_id.as_str()))?;
        (payload.get("success").and_then(Value::as_bool) == Some(true)
            && payload.get("is_error").and_then(Value::as_bool) != Some(true)
            && terminal_result_exit_succeeded(payload))
        .then_some(command)
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

include!("progress_review_part01.rs");
