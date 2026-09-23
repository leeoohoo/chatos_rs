impl RunService {
    pub(super) fn build_runtime_execution_state(
        &self,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        run_spec: &TaskRunSpec,
        tool_result_model_budget_limits: ToolResultModelBudgetLimits,
        max_iterations: usize,
        review_policy: TaskExecutionReviewPolicy,
        effective_workspace_dir: &str,
        expected_acceptance_criteria: Vec<String>,
        mcp_runtime_session: chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle,
    ) -> RuntimeExecutionState {
        let path_redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace(
            self.config.default_workspace_dir.as_str(),
            effective_workspace_dir,
        );
        let pending_stream_event =
            Arc::new(parking_lot::Mutex::new(PendingRunStreamEvent::default()));
        let abort_token = tokio_util::sync::CancellationToken::new();
        let progress = Arc::new(TaskExecutionProgressState::new(review_policy));
        let lifecycle_state =
            Arc::new(parking_lot::Mutex::new(TaskRunnerLifecycleState::default()));
        let supply_chain_evidence =
            Arc::new(parking_lot::Mutex::new(SupplyChainEvidenceState::default()));
        let callbacks = self.build_runtime_callbacks(
            run.id.clone(),
            Arc::clone(&pending_stream_event),
            path_redactor.clone(),
            Arc::clone(&progress),
            Arc::clone(&supply_chain_evidence),
        );
        let cancel_requested = Arc::new(AtomicBool::new(self.store.is_cancel_requested(&run.id)));
        if cancel_requested.load(Ordering::Relaxed) {
            abort_token.cancel();
        }
        self.register_runtime_abort_token(run.id.as_str(), abort_token.clone());
        let runtime_options = AiRuntimeOptions::new(Some(run.id.clone()), Some(run.id.clone()))
            .with_caller_model(Some(model_config.model.clone()))
            .with_record_options(run_spec.record_options.clone())
            .with_tool_result_model_budget_limits(Some(tool_result_model_budget_limits))
            .with_lifecycle_hook(Some(Arc::new(TaskRunnerLifecycleHook::new(
                max_iterations,
                Arc::clone(&progress),
                Arc::clone(&lifecycle_state),
                self.store.clone(),
                run.id.clone(),
                expected_acceptance_criteria,
                Some(mcp_runtime_session),
            ))))
            .with_callbacks(callbacks)
            .with_abort_token(Some(abort_token))
            .with_abort_checker(Some(Arc::new({
                let cancel_requested = Arc::clone(&cancel_requested);
                move |_| cancel_requested.load(Ordering::Relaxed)
            })));

        RuntimeExecutionState {
            runtime_options,
            pending_stream_event,
            lifecycle_state,
            progress,
            supply_chain_evidence,
        }
    }

    fn build_runtime_callbacks(
        &self,
        run_id: String,
        pending_stream_event: PendingRunStreamState,
        path_redactor: crate::services::path_redaction::WorkspacePathRedactor,
        progress: Arc<TaskExecutionProgressState>,
        supply_chain_evidence: Arc<parking_lot::Mutex<SupplyChainEvidenceState>>,
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
                let supply_chain_evidence = Arc::clone(&supply_chain_evidence);
                move |payload| {
                    supply_chain_evidence.lock().observe_tool_calls(&payload);
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
                let supply_chain_evidence = Arc::clone(&supply_chain_evidence);
                move |payload| {
                    progress.observe_tool_result(&payload);
                    supply_chain_evidence.lock().observe_tool_result(&payload);
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
            on_turn_phase: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id.clone();
                let path_redactor = path_redactor.clone();
                move |mut payload| {
                    path_redactor.redact_value(&mut payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "turn_phase",
                        None,
                        Some(sanitize_runtime_event_payload(payload)),
                    ));
                }
            })),
            on_runtime_guidance_applied: None,
            on_context_summarized_start: Some(context_event_callback(
                store_for_callbacks.clone(),
                run_id.clone(),
                path_redactor.clone(),
                "context_summary_start",
            )),
            on_context_summarized_stream: Some(context_event_callback(
                store_for_callbacks.clone(),
                run_id.clone(),
                path_redactor.clone(),
                "context_summary_stream",
            )),
            on_context_summarized_end: Some(context_event_callback(
                store_for_callbacks.clone(),
                run_id.clone(),
                path_redactor.clone(),
                "context_summary_end",
            )),
            on_before_model_input: None,
            on_before_model_request: Some(Arc::new({
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
                    let mut payload = summarize_model_request_event_payload(&payload);
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
            on_model_response: Some(Arc::new({
                let store = store_for_callbacks;
                let run_id = run_id.clone();
                let path_redactor = path_redactor.clone();
                move |mut payload| {
                    path_redactor.redact_value(&mut payload);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "model_response",
                        Some("模型请求已结束".to_string()),
                        Some(sanitize_runtime_event_payload(payload)),
                    ));
                }
            })),
        }
    }
}

fn context_event_callback(
    store: crate::store::AppStore,
    run_id: String,
    path_redactor: crate::services::path_redaction::WorkspacePathRedactor,
    event_type: &'static str,
) -> Arc<dyn Fn(Value) + Send + Sync> {
    Arc::new(move |payload| {
        let mut payload = summarize_context_event_payload(&payload);
        path_redactor.redact_value(&mut payload);
        store.append_run_event_sync(TaskRunEventRecord::new(
            run_id.clone(),
            event_type,
            None,
            Some(payload),
        ));
    })
}

fn summarize_context_event_payload(payload: &Value) -> Value {
    let text_chars = payload
        .get("chunk")
        .or_else(|| payload.get("content"))
        .and_then(Value::as_str)
        .map(|value| value.chars().count())
        .unwrap_or_default();
    json!({
        "phase": payload.get("phase").cloned().unwrap_or(Value::Null),
        "reason": payload.get("reason").cloned().unwrap_or(Value::Null),
        "running": payload.get("running").cloned().unwrap_or(Value::Null),
        "generated": payload.get("generated").cloned().unwrap_or(Value::Null),
        "compacted": payload.get("compacted").cloned().unwrap_or(Value::Null),
        "failed": payload.get("failed").cloned().unwrap_or(Value::Null),
        "text_chars": text_chars,
        "raw_summary_persisted": false,
    })
}

const EVENT_SECRET_VALUE_MASK: &str = "******";

/// Persist only operational diagnostics for a model request.  The callback
/// receives the exact outbound body, which can contain the entire accumulated
/// conversation and every tool schema. Storing that body once per iteration
/// makes run-event storage grow quadratically and causes detail reads to load
/// hundreds of megabytes. Durable run events therefore keep only bounded
/// operational diagnostics.
fn summarize_model_request_event_payload(payload: &Value) -> Value {
    let debug = payload.get("task_runner_debug").and_then(Value::as_object);
    let debug_value = |key: &str| debug.and_then(|value| value.get(key)).cloned();

    json!({
        "model": payload.get("model").cloned().unwrap_or(Value::Null),
        "iteration": debug_value("iteration").unwrap_or(Value::Null),
        "reason": debug_value("reason").unwrap_or(Value::Null),
        "request_attempt": debug_value("request_attempt").unwrap_or(Value::Null),
        "input_item_count": debug_value("input_item_count").unwrap_or(Value::Null),
        "input_bytes": debug_value("input_bytes").unwrap_or(Value::Null),
        "tool_count": debug_value("tool_count").unwrap_or(Value::Null),
        "supports_responses": debug_value("supports_responses").unwrap_or(Value::Null),
        "stream": debug_value("stream").unwrap_or(Value::Null),
        "connection_mode": debug_value("connection_mode").unwrap_or(Value::Null),
        "read_timeout_seconds": debug_value("read_timeout_seconds").unwrap_or(Value::Null),
        "request_body_persisted": false,
    })
}

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
