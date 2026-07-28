// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionReviewTrigger {
    ReadOnlyLoop,
    MissingTargetedReads,
    PlaceholderProgressWrite,
}

impl TaskExecutionReviewTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnlyLoop => "read_only_loop",
            Self::MissingTargetedReads => "missing_targeted_reads",
            Self::PlaceholderProgressWrite => "placeholder_progress_write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskExecutionReviewCheckpoint {
    pub iteration: usize,
    pub trigger: TaskExecutionReviewTrigger,
    pub read_only_iterations: usize,
    pub missing_read_failures: usize,
    pub policy: TaskExecutionReviewPolicy,
}

pub struct TaskExecutionProgressState {
    policy: TaskExecutionReviewPolicy,
    current_iteration: AtomicUsize,
    last_meaningful_action_iteration: AtomicUsize,
    last_review_iteration: AtomicUsize,
    missing_targeted_read_failures_after_action: AtomicUsize,
    placeholder_progress_write_iteration: AtomicUsize,
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
            missing_targeted_read_failures_after_action: AtomicUsize::new(0),
            placeholder_progress_write_iteration: AtomicUsize::new(0),
        }
    }

    pub fn policy(&self) -> TaskExecutionReviewPolicy {
        self.policy
    }

    pub fn begin_iteration(&self, iteration: usize) {
        self.current_iteration.store(iteration, Ordering::Relaxed);
    }

    pub fn observe_tool_result(&self, payload: &Value) {
        let iteration = self.current_iteration.load(Ordering::Relaxed);
        if tool_result_is_meaningful_engineering_action(payload) {
            self.last_meaningful_action_iteration
                .store(iteration, Ordering::Relaxed);
            self.missing_targeted_read_failures_after_action
                .store(0, Ordering::Relaxed);
            return;
        }

        if tool_result_is_placeholder_progress_write(payload) {
            self.placeholder_progress_write_iteration
                .store(iteration, Ordering::Relaxed);
        }
        if tool_result_is_missing_targeted_read(payload) {
            self.missing_targeted_read_failures_after_action
                .fetch_add(1, Ordering::Relaxed);
        }
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
        let last_review = self.last_review_iteration.load(Ordering::Relaxed);

        let trigger = if placeholder_iteration > 0 && placeholder_iteration > last_review {
            Some(TaskExecutionReviewTrigger::PlaceholderProgressWrite)
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
            .map(|_| TaskExecutionReviewCheckpoint {
                iteration,
                trigger,
                read_only_iterations,
                missing_read_failures,
                policy: self.policy,
            })
    }
}

pub fn tool_result_is_meaningful_engineering_action(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if tool_name_ends_with_any(
        name,
        &[
            "write_file",
            "edit_file",
            "append_file",
            "delete_path",
            "apply_patch",
            "patch",
        ],
    ) {
        return write_result_has_meaningful_project_path(payload);
    }
    if name.ends_with("process_write") {
        return true;
    }
    if !name.ends_with("terminal_controller_execute_command") {
        return false;
    }
    terminal_result_has_meaningful_command(payload)
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
    if !tool_name_ends_with_any(
        name,
        &[
            "write_file",
            "edit_file",
            "append_file",
            "delete_path",
            "apply_patch",
            "patch",
        ],
    ) {
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

fn terminal_result_has_meaningful_command(payload: &Value) -> bool {
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parsed = serde_json::from_str::<Value>(content).ok();
    let command = parsed
        .as_ref()
        .and_then(|value| value.get("common"))
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("result")
                .and_then(|value| value.get("common"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    [
        "cargo test",
        "cargo check",
        "cargo clippy",
        "pytest",
        "python -m unittest",
        "npm test",
        "npm run test",
        "npm run build",
        "pnpm test",
        "pnpm build",
        "yarn test",
        "yarn build",
        "go test",
        "mvn test",
        "gradle test",
        "dotnet test",
        "git apply",
        "apply_patch",
        "sed -i",
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
            "name": "code_maintainer_write_apply_patch",
            "success": true,
            "is_error": false,
            "result": {
                "changed_files": [{ "path": "src/lib.rs" }],
            },
        }));

        assert!(progress.should_trigger_review(5).is_none());
    }

    #[test]
    fn placeholder_progress_write_triggers_review_and_is_not_meaningful_progress() {
        let payload = json!({
            "name": "code_maintainer_write_write_file",
            "success": true,
            "is_error": false,
            "result": {
                "path": "TASK_RUNNER_PROGRESS_NOTE.md",
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
            "task_manager_update_task",
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
                "name": "code_maintainer_write_write_file",
                "success": true,
                "is_error": false,
                "result": { "path": path },
            });
            assert!(!tool_result_is_meaningful_engineering_action(&payload));
            assert!(tool_result_is_placeholder_progress_write(&payload));
        }

        assert!(tool_result_is_meaningful_engineering_action(&json!({
            "name": "code_maintainer_write_apply_patch",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "harness": {
                    "commit": {
                        "changed_files": [{ "path": "src/lib.rs" }],
                    },
                },
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
            })).expect("content"),
        })));
    }
}
