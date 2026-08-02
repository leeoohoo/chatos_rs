// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use async_trait::async_trait;
use chatos_ai_runtime::{
    RuntimeBeforeModelRequest, RuntimeIterationContext, RuntimeLifecycleHook,
    TaskExecutionProgressState, TaskExecutionReviewCheckpoint, TaskExecutionReviewPolicy,
    TaskExecutionReviewTrigger,
};

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

    fn checkpoint_input_items(&self, checkpoint: TaskExecutionReviewCheckpoint) -> Vec<Value> {
        let payload = json!({
            "iteration": checkpoint.iteration,
            "trigger": checkpoint.trigger.as_str(),
            "read_only_iterations": checkpoint.read_only_iterations,
            "missing_read_failures": checkpoint.missing_read_failures,
            "policy": {
                "read_only_iterations": checkpoint.policy.read_only_iterations,
                "missing_read_failures": checkpoint.policy.missing_read_failures,
                "repeat_interval_iterations": checkpoint.policy.repeat_interval_iterations,
            },
            "context_action": "guidance_only",
            "disabled_tool_names": [],
        });
        self.store.append_run_event_sync(TaskRunEventRecord::new(
            self.run_id.clone(),
            "execution_review_checkpoint",
            Some("检测到疑似偏航，已注入轻量校准提示并继续执行".to_string()),
            Some(payload),
        ));
        vec![checkpoint_guidance_message(checkpoint)]
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

        let Some(checkpoint) = self.progress.should_trigger_review(iteration) else {
            return Ok(before);
        };
        let items = self.checkpoint_input_items(checkpoint);
        before.input_items.extend(items);
        Ok(before)
    }
}

fn checkpoint_guidance_message(checkpoint: TaskExecutionReviewCheckpoint) -> Value {
    let trigger = match checkpoint.trigger {
        TaskExecutionReviewTrigger::ReadOnlyLoop => "连续多轮只读/观察，没有真实工程改动",
        TaskExecutionReviewTrigger::MissingTargetedReads => {
            "连续读取不存在的精确文件路径，疑似路径假设错误或相对路径理解错误"
        }
        TaskExecutionReviewTrigger::PlaceholderProgressWrite => {
            "写入了 progress/unlock/placeholder 这类不能解决任务本身的占位文件"
        }
    };
    json!({
        "role": "system",
        "content": format!(
            "[Task Runner 自动复盘 checkpoint]\n\
             检测原因：{trigger}。\n\
             现在先在心里复盘：用户目标是什么、当前已经做了哪些真实动作、哪些动作偏离航线、真实路径/工具结果已经证明了什么。\n\
             然后继续执行，不要因为这次 checkpoint 自行退出、不要把它当成权限限制、不要要求用户替你改代码。\n\
             工具没有被禁用；如果文件不存在，把它当作路径证据，不要重复读同一个不存在路径。所有项目文件工具路径都按项目根目录相对路径理解。\n\
             不要创建 TASK_RUNNER_PROGRESS_NOTE、unlock、placeholder、probe 之类的假进展文件；只有修改真实项目文件、运行必要验证、或给出有证据的终态结论才算进展。\n\
             当前计数：read_only_iterations={}, missing_read_failures={}。"
            ,
            checkpoint.read_only_iterations,
            checkpoint.missing_read_failures
        ),
    })
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
        review_policy: TaskExecutionReviewPolicy,
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
        let progress = Arc::new(TaskExecutionProgressState::new(review_policy));
        let callbacks = self.build_runtime_callbacks(
            run.id.clone(),
            Arc::clone(&pending_stream_event),
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
        run_id: String,
        pending_stream_event: PendingRunStreamState,
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
                let path_redactor = path_redactor.clone();
                let progress = Arc::clone(&progress);
                move |payload| {
                    progress.observe_tool_result(&payload);
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

#[cfg(test)]
mod tests {
    use super::*;

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
