// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(super) fn build_runtime_execution_state(
        &self,
        task_id: &str,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        run_spec: &TaskRunSpec,
        tool_result_model_budget_limits: ToolResultModelBudgetLimits,
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
        let callbacks = self.build_runtime_callbacks(
            task_id.to_string(),
            run.id.clone(),
            Arc::clone(&pending_stream_event),
            Arc::clone(&task_completed_abort),
            abort_token.clone(),
            path_redactor.clone(),
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

    pub(super) fn build_runtime_callbacks(
        &self,
        task_id: String,
        run_id: String,
        pending_stream_event: PendingRunStreamState,
        task_completed_abort: Arc<AtomicBool>,
        abort_token: tokio_util::sync::CancellationToken,
        path_redactor: crate::services::path_redaction::WorkspacePathRedactor,
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
                move |payload| {
                    if tool_result_marks_root_task_done(&payload, task_id.as_str()) {
                        task_completed_abort.store(true, Ordering::Relaxed);
                        abort_token.cancel();
                    }
                    let mut payload = sanitize_runtime_event_payload(payload);
                    path_redactor.redact_value(&mut payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tool_stream",
                        None,
                        Some(payload),
                    ));
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
