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
    ) -> RuntimeExecutionState {
        let task_completed_abort = Arc::new(AtomicBool::new(false));
        let pending_stream_event =
            Arc::new(parking_lot::Mutex::new(PendingRunStreamEvent::default()));
        let callbacks = self.build_runtime_callbacks(
            task_id.to_string(),
            run.id.clone(),
            Arc::clone(&pending_stream_event),
            Arc::clone(&task_completed_abort),
        );
        let cancel_requested = Arc::new(AtomicBool::new(self.store.is_cancel_requested(&run.id)));
        let (stop_cancel_poll, cancel_poll_handle) = self.start_runtime_abort_polling(
            task_id,
            run.id.as_str(),
            Arc::clone(&cancel_requested),
            Arc::clone(&task_completed_abort),
        );
        let runtime_options = AiRuntimeOptions::new(Some(run.id.clone()), Some(run.id.clone()))
            .with_caller_model(Some(model_config.model.clone()))
            .with_record_options(run_spec.record_options.clone())
            .with_tool_result_model_budget_limits(Some(tool_result_model_budget_limits))
            .with_callbacks(callbacks)
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
    ) -> RuntimeCallbacks {
        let store_for_callbacks = self.store.clone();
        let run_id_for_chunk = run_id.clone();

        RuntimeCallbacks {
            on_chunk: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id_for_chunk.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |chunk| {
                    if chunk.is_empty() {
                        return;
                    }
                    let flushed = {
                        let mut state = pending.lock();
                        state.push("chunk", &chunk)
                    };
                    if let Some(flushed) = flushed {
                        append_pending_stream_event(&store, run_id.as_str(), flushed);
                    }
                }
            })),
            on_thinking: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |chunk| {
                    if chunk.is_empty() {
                        return;
                    }
                    let flushed = {
                        let mut state = pending.lock();
                        state.push("thinking", &chunk)
                    };
                    if let Some(flushed) = flushed {
                        append_pending_stream_event(&store, run_id.as_str(), flushed);
                    }
                }
            })),
            on_tools_start: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |payload| {
                    flush_pending_stream_event(&store, run_id.as_str(), &pending);
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
                move |payload| {
                    if tool_result_marks_root_task_done(&payload, task_id.as_str()) {
                        task_completed_abort.store(true, Ordering::Relaxed);
                    }
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
                move |payload| {
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tools_end",
                        Some("工具调用结束".to_string()),
                        Some(payload),
                    ));
                }
            })),
            on_before_model_request: Some(Arc::new({
                let store = store_for_callbacks;
                let run_id = run_id.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |payload| {
                    flush_pending_stream_event(&store, run_id.as_str(), &pending);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "model_request",
                        Some("即将发起模型请求".to_string()),
                        Some(payload),
                    ));
                }
            })),
        }
    }

    pub(super) fn start_runtime_abort_polling(
        &self,
        task_id: &str,
        run_id: &str,
        cancel_requested: Arc<AtomicBool>,
        task_completed_abort: Arc<AtomicBool>,
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
}
