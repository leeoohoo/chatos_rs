// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use async_trait::async_trait;
use chatos_ai_runtime::{RuntimeBeforeModelRequest, RuntimeIterationContext, RuntimeLifecycleHook};
use std::sync::atomic::AtomicUsize;

const READ_ONLY_PROGRESS_BUDGET: usize = 8;
const PROGRESS_GUIDANCE_REPEAT_INTERVAL: usize = 8;
const MISSING_TARGETED_READ_FAILURE_BUDGET: usize = 2;
const MISSING_READ_GUIDANCE_REPEAT_INTERVAL: usize = 4;
const REPEATED_DISCOVERY_TOOL_NAMES: &[&str] = &[
    "project_runtime_environment_get_project_runtime_environment_info",
    "task_manager_add_task",
    "task_manager_list_tasks",
    "task_run_process_record_process",
    "code_maintainer_read_list_dir",
    "code_maintainer_read_search_text",
    "code_maintainer_read_search_files",
    "harness_code_list_branches",
    "terminal_controller_execute_command",
    "terminal_controller_get_recent_logs",
    "terminal_controller_process",
    "terminal_controller_process_kill",
    "terminal_controller_process_list",
    "terminal_controller_process_log",
    "terminal_controller_process_poll",
    "terminal_controller_process_wait",
    "terminal_controller_process_write",
];
const ASK_USER_TOOL_NAMES: &[&str] = &[
    "ask_user_prompt_mixed_form",
    "ask_user_prompt_key_values",
    "ask_user_prompt_choices",
];
const TARGETED_READ_TOOL_NAMES: &[&str] = &[
    "code_maintainer_read_read_file_raw",
    "code_maintainer_read_read_file_range",
    "code_maintainer_read_read_file",
];

#[derive(Default)]
struct TaskExecutionProgressState {
    current_iteration: AtomicUsize,
    last_meaningful_action_iteration: AtomicUsize,
    last_guidance_iteration: AtomicUsize,
    missing_targeted_read_failures_after_guard: AtomicUsize,
    last_missing_read_guidance_iteration: AtomicUsize,
}

impl TaskExecutionProgressState {
    fn begin_iteration(&self, iteration: usize) {
        self.current_iteration.store(iteration, Ordering::Relaxed);
    }

    fn observe_tool_result(&self, payload: &Value) {
        if tool_result_is_meaningful_engineering_action(payload) {
            self.last_meaningful_action_iteration.store(
                self.current_iteration.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.missing_targeted_read_failures_after_guard
                .store(0, Ordering::Relaxed);
            return;
        }

        if self.inspection_tools_are_restricted() && tool_result_is_missing_targeted_read(payload) {
            self.missing_targeted_read_failures_after_guard
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn should_inject_guidance(&self, iteration: usize) -> bool {
        let last_action = self
            .last_meaningful_action_iteration
            .load(Ordering::Relaxed);
        if iteration.saturating_sub(last_action) < READ_ONLY_PROGRESS_BUDGET {
            return false;
        }
        let last_guidance = self.last_guidance_iteration.load(Ordering::Relaxed);
        if last_guidance > 0
            && iteration.saturating_sub(last_guidance) < PROGRESS_GUIDANCE_REPEAT_INTERVAL
        {
            return false;
        }
        self.last_guidance_iteration
            .compare_exchange(
                last_guidance,
                iteration,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    fn inspection_tools_are_restricted(&self) -> bool {
        let last_guidance = self.last_guidance_iteration.load(Ordering::Relaxed);
        let last_action = self
            .last_meaningful_action_iteration
            .load(Ordering::Relaxed);
        last_guidance > 0 && last_guidance > last_action
    }

    fn should_inject_missing_read_guidance(&self, iteration: usize) -> bool {
        if !self.targeted_reads_restricted_by_missing_files() {
            return false;
        }
        let last_guidance = self
            .last_missing_read_guidance_iteration
            .load(Ordering::Relaxed);
        if last_guidance > 0
            && iteration.saturating_sub(last_guidance) < MISSING_READ_GUIDANCE_REPEAT_INTERVAL
        {
            return false;
        }
        self.last_missing_read_guidance_iteration
            .compare_exchange(
                last_guidance,
                iteration,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    fn targeted_reads_restricted_by_missing_files(&self) -> bool {
        self.inspection_tools_are_restricted()
            && self
                .missing_targeted_read_failures_after_guard
                .load(Ordering::Relaxed)
                >= MISSING_TARGETED_READ_FAILURE_BUDGET
    }

    fn missing_targeted_read_failure_count(&self) -> usize {
        self.missing_targeted_read_failures_after_guard
            .load(Ordering::Relaxed)
    }

    fn restricted_tool_names(&self, iteration: usize) -> Vec<&'static str> {
        let mut restricted = if self.last_guidance_iteration.load(Ordering::Relaxed) > 0 {
            ASK_USER_TOOL_NAMES.to_vec()
        } else {
            Vec::new()
        };
        if !self.inspection_tools_are_restricted() {
            return restricted;
        }
        let last_action = self
            .last_meaningful_action_iteration
            .load(Ordering::Relaxed);
        let restrict_targeted_reads = iteration.saturating_sub(last_action)
            >= READ_ONLY_PROGRESS_BUDGET * 2
            || self.targeted_reads_restricted_by_missing_files();
        restricted.extend(REPEATED_DISCOVERY_TOOL_NAMES.iter().copied());
        if restrict_targeted_reads {
            restricted.extend(TARGETED_READ_TOOL_NAMES.iter().copied());
        }
        restricted
    }
}

struct TaskRunnerLifecycleHook {
    finalization: TaskFinalizationLifecycleHook,
    progress: Arc<TaskExecutionProgressState>,
    store: crate::store::AppStore,
    run_id: String,
}

impl TaskRunnerLifecycleHook {
    fn new(
        max_iterations: usize,
        progress: Arc<TaskExecutionProgressState>,
        store: crate::store::AppStore,
        run_id: String,
    ) -> Self {
        Self {
            finalization: TaskFinalizationLifecycleHook::new(max_iterations),
            progress,
            store,
            run_id,
        }
    }
}

#[async_trait]
impl RuntimeLifecycleHook for TaskRunnerLifecycleHook {
    async fn before_model_request(
        &self,
        context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        self.progress.begin_iteration(context.iteration);
        let iteration = context.iteration;
        let mut before = self.finalization.before_model_request(context).await?;
        if !before.tools_enabled {
            return Ok(before);
        }

        let inject_progress_guidance = self.progress.should_inject_guidance(iteration);
        let inject_missing_read_guidance =
            self.progress.should_inject_missing_read_guidance(iteration);
        let disabled_tool_names = self.progress.restricted_tool_names(iteration);
        if !disabled_tool_names.is_empty() {
            before = before.with_disabled_tool_names(disabled_tool_names.iter().copied());
        }
        if !inject_progress_guidance && !inject_missing_read_guidance {
            return Ok(before);
        }

        let targeted_reads_restricted = disabled_tool_names
            .iter()
            .any(|name| TARGETED_READ_TOOL_NAMES.contains(name));
        let missing_read_count = self.progress.missing_targeted_read_failure_count();
        let restriction_detail = if inject_missing_read_guidance {
            "Targeted read-file tools are now temporarily unavailable because this run repeatedly tried to read a file that the workspace reported as missing. Treat the missing file as evidence, not as a permission problem. Do not read the same missing path again, do not ask the user for clarification about that path, and do not create placeholder/unlock files. Use the available write/edit/apply-patch tools on files that already exist or create the actual required project file if that is the intended implementation. If the task cannot proceed without an external artifact, mark the Task Manager item as terminally blocked with the exact missing artifact and evidence."
        } else if targeted_reads_restricted {
            "All read-only, terminal, and AskUser tools are now temporarily unavailable because the first guard did not produce a real project-file write."
        } else {
            "Directory listing, search, environment, terminal, and AskUser tools are now temporarily unavailable. Targeted read-file tools remain available only for exact files already identified; do not repeat a file read whose content is already in the conversation."
        };
        let message = format!(
            "[Task Runner execution progress guard]\nThis run has spent {READ_ONLY_PROGRESS_BUDGET} or more model/tool iterations without a meaningful engineering action. Re-reading the same files through code tools or terminal commands, repeatedly fetching runtime environment information, updating Task Manager, or recording process notes does not count as implementation progress. Stop repeating inspection. {restriction_detail} This is an intentional progress guard, not a missing permission or missing user input. Do not mark tasks blocked merely because discovery tools were rate-limited, and do not ask the user to rerun the task or provide files that were already inspected. If one exact source file is still needed and a targeted read-file tool remains available, read it once; otherwise use the available write/edit/apply-patch tools now. Writes under `.chatos`, build outputs, caches, or dependency directories do not count. After a successful write to a real project file, terminal and read tools will become available again for targeted tests and diff inspection. If the task is analysis-only and the evidence is already sufficient, stop calling tools and provide the final conclusion."
        );
        before.input_items.push(json!({
            "role": "system",
            "content": message,
        }));
        let event_type = if inject_missing_read_guidance {
            "execution_missing_read_guard"
        } else {
            "execution_progress_guard"
        };
        let event_summary = if inject_missing_read_guidance {
            "检测到重复读取不存在文件，已禁用精确读取并要求模型改为实现或终态阻塞"
        } else {
            "检测到重复只读循环，已要求模型立即进入实现、验证或收口"
        };
        self.store.append_run_event_sync(TaskRunEventRecord::new(
            self.run_id.clone(),
            event_type,
            Some(event_summary.to_string()),
            Some(json!({
                "iteration": iteration,
                "read_only_budget": READ_ONLY_PROGRESS_BUDGET,
                "missing_targeted_read_failure_budget": MISSING_TARGETED_READ_FAILURE_BUDGET,
                "missing_targeted_read_failure_count": missing_read_count,
                "disabled_tool_names": disabled_tool_names,
            })),
        ));
        Ok(before)
    }
}

impl RunService {
    pub(super) fn build_runtime_execution_state(
        &self,
        task_id: &str,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        run_spec: &TaskRunSpec,
        tool_result_model_budget_limits: ToolResultModelBudgetLimits,
        max_iterations: usize,
        effective_workspace_dir: &str,
    ) -> RuntimeExecutionState {
        let path_redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace(
            self.config.default_workspace_dir.as_str(),
            effective_workspace_dir,
        );
        let task_completed_abort = Arc::new(AtomicBool::new(false));
        let pending_stream_event =
            Arc::new(parking_lot::Mutex::new(PendingRunStreamEvent::default()));
        let abort_token = tokio_util::sync::CancellationToken::new();
        let progress = Arc::new(TaskExecutionProgressState::default());
        let callbacks = self.build_runtime_callbacks(
            task_id.to_string(),
            run.id.clone(),
            Arc::clone(&pending_stream_event),
            Arc::clone(&task_completed_abort),
            abort_token.clone(),
            path_redactor.clone(),
            Arc::clone(&progress),
        );
        let cancel_requested = Arc::new(AtomicBool::new(self.store.is_cancel_requested(&run.id)));
        let (stop_cancel_poll, cancel_poll_handle) = self.start_runtime_abort_polling(
            task_id,
            run.id.as_str(),
            Arc::clone(&cancel_requested),
            Arc::clone(&task_completed_abort),
            abort_token.clone(),
        );
        let runtime_options = AiRuntimeOptions::new(Some(run.id.clone()), Some(run.id.clone()))
            .with_caller_model(Some(model_config.model.clone()))
            .with_record_options(run_spec.record_options.clone())
            .with_tool_result_model_budget_limits(Some(tool_result_model_budget_limits))
            .with_lifecycle_hook(Some(Arc::new(TaskRunnerLifecycleHook::new(
                max_iterations,
                progress,
                self.store.clone(),
                run.id.clone(),
            ))))
            .with_callbacks(callbacks)
            .with_abort_token(Some(abort_token))
            .with_abort_checker(Some(Arc::new({
                let cancel_requested = Arc::clone(&cancel_requested);
                let task_completed_abort = Arc::clone(&task_completed_abort);
                move |_| {
                    cancel_requested.load(Ordering::Relaxed)
                        || task_completed_abort.load(Ordering::Relaxed)
                }
            })));

        RuntimeExecutionState {
            runtime_options,
            pending_stream_event,
            task_completed_abort,
            stop_cancel_poll,
            cancel_poll_handle,
        }
    }

    fn build_runtime_callbacks(
        &self,
        task_id: String,
        run_id: String,
        pending_stream_event: PendingRunStreamState,
        task_completed_abort: Arc<AtomicBool>,
        abort_token: tokio_util::sync::CancellationToken,
        path_redactor: crate::services::path_redaction::WorkspacePathRedactor,
        progress: Arc<TaskExecutionProgressState>,
    ) -> RuntimeCallbacks {
        let store_for_callbacks = self.store.clone();
        let run_id_for_chunk = run_id.clone();

        RuntimeCallbacks {
            on_chunk: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id_for_chunk.clone();
                let pending = Arc::clone(&pending_stream_event);
                let path_redactor = path_redactor.clone();
                move |chunk| {
                    if chunk.is_empty() {
                        return;
                    }
                    let flushed = {
                        let mut state = pending.lock();
                        state.push("chunk", &chunk)
                    };
                    if let Some(flushed) = flushed {
                        append_pending_stream_event(
                            &store,
                            run_id.as_str(),
                            flushed,
                            Some(&path_redactor),
                        );
                    }
                }
            })),
            on_thinking: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let pending = Arc::clone(&pending_stream_event);
                let path_redactor = path_redactor.clone();
                move |chunk| {
                    if chunk.is_empty() {
                        return;
                    }
                    let flushed = {
                        let mut state = pending.lock();
                        state.push("thinking", &chunk)
                    };
                    if let Some(flushed) = flushed {
                        append_pending_stream_event(
                            &store,
                            run_id.as_str(),
                            flushed,
                            Some(&path_redactor),
                        );
                    }
                }
            })),
            on_tools_start: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let pending = Arc::clone(&pending_stream_event);
                let path_redactor = path_redactor.clone();
                move |payload| {
                    flush_pending_stream_event(
                        &store,
                        run_id.as_str(),
                        &pending,
                        Some(&path_redactor),
                    );
                    let mut payload = sanitize_runtime_event_payload(payload);
                    path_redactor.redact_value(&mut payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tools_start",
                        Some("开始调用工具".to_string()),
                        Some(payload),
                    ));
                }
            })),
            on_tools_stream: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let task_id = task_id.clone();
                let task_completed_abort = Arc::clone(&task_completed_abort);
                let abort_token = abort_token.clone();
                let path_redactor = path_redactor.clone();
                let progress = Arc::clone(&progress);
                move |payload| {
                    progress.observe_tool_result(&payload);
                    if tool_result_marks_root_task_done(&payload, task_id.as_str()) {
                        task_completed_abort.store(true, Ordering::Relaxed);
                        abort_token.cancel();
                    }
                    let mut payload = sanitize_runtime_event_payload(payload);
                    path_redactor.redact_value(&mut payload);
                    let browser_session = browser_session_event_payload(&payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tool_stream",
                        None,
                        Some(payload),
                    ));
                    if let Some(browser_session) = browser_session {
                        store.append_run_event_sync(TaskRunEventRecord::new(
                            run_id.clone(),
                            "browser_session",
                            None,
                            Some(browser_session),
                        ));
                    }
                }
            })),
            on_tools_end: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let path_redactor = path_redactor.clone();
                move |payload| {
                    let mut payload = sanitize_runtime_event_payload(payload);
                    path_redactor.redact_value(&mut payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tools_end",
                        Some("工具调用结束".to_string()),
                        Some(payload),
                    ));
                }
            })),
            on_turn_phase: None,
            on_runtime_guidance_applied: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
            on_before_model_input: None,
            on_before_model_request: Some(Arc::new({
                let store = store_for_callbacks;
                let run_id = run_id.clone();
                let pending = Arc::clone(&pending_stream_event);
                let path_redactor = path_redactor.clone();
                move |payload| {
                    flush_pending_stream_event(
                        &store,
                        run_id.as_str(),
                        &pending,
                        Some(&path_redactor),
                    );
                    let mut payload = sanitize_runtime_event_payload(payload);
                    path_redactor.redact_value(&mut payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "model_request",
                        Some("即将发起模型请求".to_string()),
                        Some(payload),
                    ));
                }
            })),
            on_before_send_model_request: None,
        }
    }

    pub(super) fn start_runtime_abort_polling(
        &self,
        task_id: &str,
        run_id: &str,
        cancel_requested: Arc<AtomicBool>,
        task_completed_abort: Arc<AtomicBool>,
        abort_token: tokio_util::sync::CancellationToken,
    ) -> (Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
        let stop_cancel_poll = Arc::new(AtomicBool::new(false));
        let cancel_poll_handle = tokio::spawn({
            let store = self.store.clone();
            let task_id = task_id.to_string();
            let run_id = run_id.to_string();
            let cancel_requested = Arc::clone(&cancel_requested);
            let task_completed_abort = Arc::clone(&task_completed_abort);
            let stop_cancel_poll = Arc::clone(&stop_cancel_poll);
            async move {
                while !stop_cancel_poll.load(Ordering::Relaxed) {
                    match store.get_task(&task_id).await {
                        Ok(Some(task)) if task.status == TaskStatus::Succeeded => {
                            task_completed_abort.store(true, Ordering::Relaxed);
                            abort_token.cancel();
                            break;
                        }
                        Ok(_) => {}
                        Err(err) => {
                            warn!(
                                "failed to refresh task completion flag for task {}: {}",
                                task_id, err
                            );
                        }
                    }
                    match store.fetch_cancel_requested(&run_id).await {
                        Ok(is_requested) => {
                            cancel_requested.store(is_requested, Ordering::Relaxed);
                            if is_requested {
                                abort_token.cancel();
                                break;
                            }
                        }
                        Err(err) => {
                            warn!(
                                "failed to refresh cancel_requested flag for run {}: {}",
                                run_id, err
                            );
                        }
                    }
                    tokio::time::sleep(crate::services::RUN_CANCEL_POLL_INTERVAL).await;
                }
            }
        });

        (stop_cancel_poll, cancel_poll_handle)
    }
}

fn tool_result_is_meaningful_engineering_action(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if name.contains("code_maintainer_write_")
        && [
            "write_file",
            "edit_file",
            "append_file",
            "delete_path",
            "apply_patch",
            "patch",
        ]
        .iter()
        .any(|tool| name.ends_with(tool))
    {
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

fn tool_result_is_missing_targeted_read(payload: &Value) -> bool {
    if payload.get("success").and_then(Value::as_bool) == Some(true)
        && payload.get("is_error").and_then(Value::as_bool) != Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if !TARGETED_READ_TOOL_NAMES.contains(&name) {
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
        "execution-notes",
        "execution_notes",
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

const EVENT_SECRET_VALUE_MASK: &str = "******";

fn sanitize_runtime_event_payload(mut payload: Value) -> Value {
    sanitize_runtime_event_value(&mut payload);
    payload
}

fn browser_session_event_payload(value: &Value) -> Option<Value> {
    match value {
        Value::Object(map) => {
            if let Some(session) = map.get("browser_session").filter(|value| value.is_object()) {
                return Some(session.clone());
            }
            map.values().find_map(browser_session_event_payload)
        }
        Value::Array(items) => items.iter().find_map(browser_session_event_payload),
        _ => None,
    }
}

fn sanitize_runtime_event_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_ask_user_tool = map
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.contains("ask_user_prompt"));
            if is_ask_user_tool {
                sanitize_ask_user_tool_result(map);
            }
            if object_looks_like_ask_user_response(map) {
                if let Some(values) = map.get_mut("values") {
                    redact_all_values(values);
                }
            }
            for item in map.values_mut() {
                sanitize_runtime_event_value(item);
            }
        }
        Value::Array(items) => {
            for item in items {
                sanitize_runtime_event_value(item);
            }
        }
        Value::String(_) => {
            if let Some(parsed) = sanitize_json_string(value) {
                *value = parsed;
            }
        }
        _ => {}
    }
}

fn sanitize_ask_user_tool_result(map: &mut serde_json::Map<String, Value>) {
    if let Some(content) = map.get_mut("content") {
        sanitize_ask_user_response_string(content);
    }
    if let Some(result) = map.get_mut("result") {
        redact_all_response_values(result);
        sanitize_runtime_event_value(result);
    }
}

fn sanitize_ask_user_response_string(value: &mut Value) {
    let Some(text) = value.as_str() else {
        sanitize_runtime_event_value(value);
        return;
    };
    let Ok(mut parsed) = serde_json::from_str::<Value>(text) else {
        return;
    };
    redact_all_response_values(&mut parsed);
    if let Ok(redacted) = serde_json::to_string(&parsed) {
        *value = Value::String(redacted);
    }
}

fn sanitize_json_string(value: &Value) -> Option<Value> {
    let text = value.as_str()?;
    let mut parsed = serde_json::from_str::<Value>(text).ok()?;
    if !looks_like_ask_user_response(&parsed) {
        return None;
    }
    redact_all_response_values(&mut parsed);
    serde_json::to_string(&parsed).ok().map(Value::String)
}

fn looks_like_ask_user_response(value: &Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    object_looks_like_ask_user_response(map)
}

fn object_looks_like_ask_user_response(map: &serde_json::Map<String, Value>) -> bool {
    let Some(status) = map.get("status").and_then(Value::as_str) else {
        return false;
    };
    matches!(
        status,
        "pending" | "submitted" | "cancelled" | "timed_out" | "failed"
    ) && map.get("values").is_some()
}

fn redact_all_response_values(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(values) = map.get_mut("values") {
                redact_all_values(values);
            }
            for item in map.values_mut() {
                redact_all_response_values(item);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_all_response_values(item);
            }
        }
        _ => {}
    }
}

fn redact_all_values(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for item in map.values_mut() {
                if !item.is_null() {
                    *item = Value::String(EVENT_SECRET_VALUE_MASK.to_string());
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if !item.is_null() {
                    *item = Value::String(EVENT_SECRET_VALUE_MASK.to_string());
                }
            }
        }
        other if !other.is_null() => {
            *other = Value::String(EVENT_SECRET_VALUE_MASK.to_string());
        }
        _ => {}
    }
}

fn tool_result_marks_root_task_done(payload: &Value, task_id: &str) -> bool {
    if payload.get("success").and_then(Value::as_bool) != Some(true)
        || payload.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    if !name.ends_with("complete_task") && !name.ends_with("update_task") {
        return false;
    }
    let Some(content) = payload.get("content").and_then(Value::as_str) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return false;
    };
    let Some(task) = value.get("task") else {
        return false;
    };
    if task.get("id").and_then(Value::as_str) != Some(task_id) {
        return false;
    }
    task.get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "done" | "succeeded"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_guard_fires_after_repeated_read_only_iterations() {
        let progress = TaskExecutionProgressState::default();

        assert!(!progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET - 1));
        assert!(progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET));
        assert!(!progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET + 1));
        assert!(progress
            .should_inject_guidance(READ_ONLY_PROGRESS_BUDGET + PROGRESS_GUIDANCE_REPEAT_INTERVAL));
    }

    #[test]
    fn successful_code_write_resets_read_only_budget() {
        let progress = TaskExecutionProgressState::default();
        assert!(progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET));
        assert!(progress.inspection_tools_are_restricted());
        progress.begin_iteration(9);
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_write_apply_patch",
            "success": true,
            "is_error": false,
            "result": {
                "changed_files": [{ "path": "src/lib.rs" }],
            },
        }));

        assert!(!progress.inspection_tools_are_restricted());
        let restored_after_write = progress.restricted_tool_names(10);
        assert!(restored_after_write.contains(&"ask_user_prompt_key_values"));
        assert!(!restored_after_write.contains(&"terminal_controller_execute_command"));
        assert!(!restored_after_write.contains(&"code_maintainer_read_read_file_raw"));
        assert!(!progress.should_inject_guidance(16));
        assert!(progress.should_inject_guidance(17));
    }

    #[test]
    fn runtime_and_task_manager_tools_do_not_count_as_engineering_progress() {
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
    fn progress_guard_restricts_terminal_inspection_until_a_write_occurs() {
        let progress = TaskExecutionProgressState::default();
        assert!(progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET));
        let first_stage = progress.restricted_tool_names(READ_ONLY_PROGRESS_BUDGET);
        assert!(first_stage.contains(&"terminal_controller_execute_command"));
        assert!(first_stage.contains(&"terminal_controller_get_recent_logs"));
        assert!(first_stage.contains(&"terminal_controller_process_log"));
        assert!(first_stage.contains(&"ask_user_prompt_mixed_form"));
        assert!(!first_stage.contains(&"code_maintainer_read_read_file_raw"));

        let second_stage = progress.restricted_tool_names(READ_ONLY_PROGRESS_BUDGET * 2);
        assert!(second_stage.contains(&"code_maintainer_read_read_file_raw"));
    }

    #[test]
    fn missing_targeted_reads_after_progress_guard_restrict_targeted_reads_early() {
        let progress = TaskExecutionProgressState::default();
        assert!(progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET));

        progress.begin_iteration(READ_ONLY_PROGRESS_BUDGET + 1);
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": false,
            "is_error": true,
            "content": "No such file or directory: pnpm-lock.yaml",
        }));
        assert!(!progress
            .restricted_tool_names(READ_ONLY_PROGRESS_BUDGET + 2)
            .contains(&"code_maintainer_read_read_file_raw"));

        progress.begin_iteration(READ_ONLY_PROGRESS_BUDGET + 2);
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": false,
            "is_error": true,
            "result": {
                "message": "pnpm-lock.yaml not found",
            },
        }));
        let restricted = progress.restricted_tool_names(READ_ONLY_PROGRESS_BUDGET + 3);

        assert!(restricted.contains(&"code_maintainer_read_read_file_raw"));
        assert!(progress.should_inject_missing_read_guidance(READ_ONLY_PROGRESS_BUDGET + 3));
        assert!(!progress.should_inject_missing_read_guidance(READ_ONLY_PROGRESS_BUDGET + 4));
    }

    #[test]
    fn source_file_write_clears_missing_targeted_read_guard() {
        let progress = TaskExecutionProgressState::default();
        assert!(progress.should_inject_guidance(READ_ONLY_PROGRESS_BUDGET));
        for iteration in [READ_ONLY_PROGRESS_BUDGET + 1, READ_ONLY_PROGRESS_BUDGET + 2] {
            progress.begin_iteration(iteration);
            progress.observe_tool_result(&json!({
                "name": "code_maintainer_read_read_file",
                "success": false,
                "is_error": true,
                "content": "ENOENT: package-lock.json does not exist",
            }));
        }
        assert!(progress.targeted_reads_restricted_by_missing_files());

        progress.begin_iteration(READ_ONLY_PROGRESS_BUDGET + 3);
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_write_apply_patch",
            "success": true,
            "is_error": false,
            "result": {
                "changed_files": [{ "path": "services/orders/src/lib.rs" }],
            },
        }));

        assert!(!progress.targeted_reads_restricted_by_missing_files());
        assert!(!progress
            .restricted_tool_names(READ_ONLY_PROGRESS_BUDGET + 4)
            .contains(&"code_maintainer_read_read_file"));
    }

    #[test]
    fn missing_targeted_read_detection_requires_failed_read_tool() {
        assert!(tool_result_is_missing_targeted_read(&json!({
            "name": "code_maintainer_read_read_file_range",
            "success": false,
            "is_error": true,
            "content": "file not found: src/main.rs",
        })));
        assert!(!tool_result_is_missing_targeted_read(&json!({
            "name": "code_maintainer_read_read_file_range",
            "success": true,
            "is_error": false,
            "content": "comment says not found",
        })));
        assert!(!tool_result_is_missing_targeted_read(&json!({
            "name": "code_maintainer_read_search_files",
            "success": false,
            "is_error": true,
            "content": "not found",
        })));
    }

    #[test]
    fn temporary_unlock_file_does_not_count_as_engineering_progress() {
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
                "result": {
                    "result": { "path": path },
                },
            });

            assert!(
                !tool_result_is_meaningful_engineering_action(&payload),
                "{path} must not count as implementation progress",
            );
        }
    }

    #[test]
    fn source_file_write_counts_as_engineering_progress() {
        let payload = json!({
            "name": "code_maintainer_write_apply_patch",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "harness": {
                    "commit": {
                        "changed_files": [{ "path": "mdm-service/src/mdm_service/server.py" }],
                    },
                },
            })).expect("content"),
        });

        assert!(tool_result_is_meaningful_engineering_action(&payload));
    }

    #[test]
    fn targeted_test_command_counts_as_engineering_progress() {
        let payload = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "python -m unittest discover -s tests -v",
            })).expect("content"),
        });

        assert!(tool_result_is_meaningful_engineering_action(&payload));
    }

    #[test]
    fn browser_session_event_is_extracted_from_nested_tool_result() {
        let payload = json!({
            "name": "browser_tools_browser_navigate",
            "result": {
                "success": true,
                "browser_session": {
                    "id": "h_session_123",
                    "mode": "managed",
                    "status": "active",
                    "event": "updated"
                }
            }
        });

        let session = browser_session_event_payload(&payload).expect("browser session");
        assert_eq!(session["id"], "h_session_123");
        assert_eq!(session["status"], "active");
    }

    #[test]
    fn tool_result_marks_root_task_done_for_complete_result() {
        let payload = json!({
            "name": "task_manager_complete_task",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "completed": true,
                "task": { "id": "task-1", "status": "done" },
            })).expect("content"),
        });

        assert!(tool_result_marks_root_task_done(&payload, "task-1"));
    }

    #[test]
    fn tool_result_ignores_non_root_task_completion() {
        let payload = json!({
            "name": "task_manager_complete_task",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "completed": true,
                "task": { "id": "child-1", "status": "done" },
            })).expect("content"),
        });

        assert!(!tool_result_marks_root_task_done(&payload, "task-1"));
    }

    #[test]
    fn tool_result_marks_root_task_done_for_update_result() {
        let payload = json!({
            "name": "task_manager_update_task",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "updated": true,
                "task": { "id": "task-1", "status": "done" },
            })).expect("content"),
        });

        assert!(tool_result_marks_root_task_done(&payload, "task-1"));
    }

    #[test]
    fn sanitize_runtime_event_payload_redacts_ask_user_tool_results() {
        let payload = json!({
            "name": "ask_user_prompt_mixed_form",
            "success": true,
            "content": serde_json::to_string(&json!({
                "status": "submitted",
                "values": {
                    "public_port_policy": "direct_open_defaults",
                    "admin_password": "super-secret"
                },
                "selection": "proceed"
            })).expect("content"),
            "result": {
                "status": "submitted",
                "values": {
                    "token": "secret-token"
                },
                "selection": "proceed"
            }
        });

        let sanitized = sanitize_runtime_event_payload(payload);
        let content = sanitized["content"].as_str().expect("content");

        assert!(!content.contains("super-secret"));
        assert!(content.contains(EVENT_SECRET_VALUE_MASK));
        assert_eq!(
            sanitized["result"]["values"]["token"],
            EVENT_SECRET_VALUE_MASK
        );
        assert_eq!(sanitized["result"]["selection"], "proceed");
    }

    #[test]
    fn sanitize_runtime_event_payload_redacts_ask_user_output_in_model_input() {
        let payload = json!({
            "input": [
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": serde_json::to_string(&json!({
                        "status": "submitted",
                        "values": {
                            "admin_password": "super-secret"
                        },
                        "selection": "proceed"
                    })).expect("output")
                }
            ]
        });

        let sanitized = sanitize_runtime_event_payload(payload);
        let output = sanitized["input"][0]["output"].as_str().expect("output");

        assert!(!output.contains("super-secret"));
        assert!(output.contains(EVENT_SECRET_VALUE_MASK));
    }

    #[test]
    fn sanitize_runtime_event_payload_keeps_unrelated_status_values_objects() {
        let payload = json!({
            "status": "ok",
            "values": {
                "debug": "keep-me"
            }
        });

        let sanitized = sanitize_runtime_event_payload(payload);

        assert_eq!(sanitized["values"]["debug"], "keep-me");
    }
}
