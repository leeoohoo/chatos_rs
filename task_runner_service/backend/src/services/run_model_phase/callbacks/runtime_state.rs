// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::run_model_phase::supply_chain::SupplyChainEvidenceState;
use async_trait::async_trait;
use chatos_ai_runtime::{
    AiResponse, RuntimeBeforeModelRequest, RuntimeFinalResponseAction, RuntimeFinalResponseContext,
    RuntimeIterationContext, RuntimeLifecycleHook, TaskExecutionOutcome,
    TaskExecutionOutcomeStatus, TaskExecutionProgressState, TaskExecutionReviewCheckpoint,
    TaskExecutionReviewPolicy, TaskExecutionReviewTrigger,
};
#[cfg(test)]
#[path = "runtime_state/tests.rs"]
mod tests;

const TASK_OUTCOME_REVIEW_REASON: &str = "task_execution_outcome_review";
const TASK_EXECUTION_OUTCOME_METADATA_KEY: &str = "task_execution_outcome";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(in crate::services) struct TaskRunnerLifecycleState {
    pub(in crate::services) visible_response: Option<AiResponse>,
    pub(in crate::services) execution_outcome: Option<TaskExecutionOutcome>,
}

struct TaskRunnerLifecycleHook {
    finalization: TaskFinalizationLifecycleHook,
    progress: Arc<TaskExecutionProgressState>,
    active_review: parking_lot::Mutex<Option<TaskExecutionReviewCheckpoint>>,
    state: Arc<parking_lot::Mutex<TaskRunnerLifecycleState>>,
    requires_execution: bool,
    store: crate::store::AppStore,
    run_id: String,
}

impl TaskRunnerLifecycleHook {
    fn new(
        max_iterations: usize,
        progress: Arc<TaskExecutionProgressState>,
        state: Arc<parking_lot::Mutex<TaskRunnerLifecycleState>>,
        requires_execution: bool,
        store: crate::store::AppStore,
        run_id: String,
    ) -> Self {
        Self {
            finalization: TaskFinalizationLifecycleHook::new(max_iterations),
            progress,
            active_review: parking_lot::Mutex::new(None),
            state,
            requires_execution,
            store,
            run_id,
        }
    }

    fn record_review_checkpoint(
        &self,
        checkpoint: TaskExecutionReviewCheckpoint,
        confirmed_project_paths: &[String],
    ) {
        let payload = json!({
            "iteration": checkpoint.iteration,
            "trigger": checkpoint.trigger.as_str(),
            "read_only_iterations": checkpoint.read_only_iterations,
            "missing_read_failures": checkpoint.missing_read_failures,
            "checkpoints_since_action": checkpoint.checkpoints_since_action,
            "policy": {
                "read_only_iterations": checkpoint.policy.read_only_iterations,
                "missing_read_failures": checkpoint.policy.missing_read_failures,
                "repeat_interval_iterations": checkpoint.policy.repeat_interval_iterations,
            },
            "context_action": "persistent_guidance",
            "disabled_tool_names": [],
            "review_contract": "evidence_driven_next_action",
            "confirmed_project_paths": confirmed_project_paths,
        });
        self.store.append_run_event_sync(TaskRunEventRecord::new(
            self.run_id.clone(),
            "execution_review_checkpoint",
            Some("已进入证据驱动的工程决策复盘".to_string()),
            Some(payload),
        ));
    }
}

#[async_trait]
impl RuntimeLifecycleHook for TaskRunnerLifecycleHook {
    async fn before_model_request(
        &self,
        context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        self.progress.begin_iteration(context.iteration);
        if context.reason == TASK_OUTCOME_REVIEW_REASON {
            return Ok(RuntimeBeforeModelRequest::unchanged()
                .with_stream_output(false)
                .with_tools_enabled(false));
        }
        let iteration = context.iteration;
        let mut before = self.finalization.before_model_request(context).await?;
        if !before.tools_enabled {
            return Ok(before);
        }

        let detected_checkpoint = self.progress.should_trigger_review(iteration);
        let confirmed_project_paths = self.progress.confirmed_project_paths();
        if let Some(checkpoint) = detected_checkpoint {
            self.record_review_checkpoint(checkpoint, &confirmed_project_paths);
        }
        if let Some(checkpoint) =
            persistent_review_checkpoint(&self.active_review, detected_checkpoint)
        {
            // Keep the decision contract present after each tool result. Otherwise a bounded
            // locate/edit action can return to an unconstrained exploration loop on the next turn.
            before.input_items.push(checkpoint_guidance_message(
                checkpoint,
                &confirmed_project_paths,
            ));
        }
        Ok(before)
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        if context.reason == TASK_OUTCOME_REVIEW_REASON {
            let outcome = parse_task_execution_outcome(context.response.content.as_str())?;
            let mut state = self.state.lock();
            state.execution_outcome = Some(outcome);
            let visible_response = state
                .visible_response
                .take()
                .ok_or_else(|| "task execution outcome review lost visible response".to_string())?;
            return Ok(RuntimeFinalResponseAction::Replace(Box::new(
                visible_response,
            )));
        }

        self.state.lock().visible_response = Some(context.response.clone());
        Ok(RuntimeFinalResponseAction::Continue {
            input_items: vec![
                json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": context.response.content
                    }]
                }),
                task_execution_outcome_review_message(self.requires_execution),
            ],
            reason: TASK_OUTCOME_REVIEW_REASON.to_string(),
        })
    }

    async fn final_response_metadata(
        &self,
        _context: RuntimeFinalResponseContext,
    ) -> Result<Option<Value>, String> {
        self.state
            .lock()
            .execution_outcome
            .clone()
            .map(|outcome| {
                serde_json::to_value(outcome)
                    .map(|outcome| json!({(TASK_EXECUTION_OUTCOME_METADATA_KEY): outcome}))
                    .map_err(|err| format!("failed to serialize task execution outcome: {err}"))
            })
            .transpose()
    }
}

fn task_execution_outcome_review_message(requires_execution: bool) -> Value {
    let evidence_rule = if requires_execution {
        "This task requires execution. Success evidence must cite actual tool results, changed project files, and necessary command or test results; prose claims are not evidence."
    } else {
        "This is a non-execution planning task. Success evidence may cite concrete sections of the delivered planning response that satisfy the requested artifacts; do not require file changes, a sandbox, or command execution unless the task explicitly requested them."
    };
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": format!("[Task Execution Outcome Review]\nReview the task objective, acceptance criteria, tool results, command exit codes, file changes, and the assistant's proposed final response. {evidence_rule} Return exactly one JSON object and no markdown or explanatory text:\n{{\"status\":\"succeeded|blocked\",\"summary\":\"concise user-facing result without paths, ports, or URLs\",\"blocking_reason\":null,\"unmet_acceptance_criteria\":[],\"verification_evidence\":[\"specific evidence\"],\"referenced_paths\":[\"workspace-relative/path\"],\"referenced_endpoints\":[\"http://127.0.0.1:4000/health\"]}}\nSet status to succeeded only when every required acceptance criterion has concrete evidence and all necessary verification has passed. For succeeded, blocking_reason must be null, unmet_acceptance_criteria must be empty, and verification_evidence must be non-empty. Put every user-facing file or directory reference in referenced_paths using workspace-relative paths only. Put every user-facing URL or port-bearing address in referenced_endpoints as an absolute HTTP/HTTPS URL without credentials. Keep summary free of paths, ports, and URLs because the platform builds those receipt sections from validated references. Otherwise set status to blocked, provide the concrete blocker, list every unmet acceptance criterion, and include the failed or missing verification evidence. Do not use failed or cancelled; transport failures and cancellation are determined by the runtime.")
        }]
    })
}

fn parse_task_execution_outcome(content: &str) -> Result<TaskExecutionOutcome, String> {
    let outcome = serde_json::from_str::<TaskExecutionOutcome>(content.trim())
        .map_err(|err| format!("invalid task execution outcome JSON: {err}"))?;
    if !matches!(
        outcome.status,
        TaskExecutionOutcomeStatus::Succeeded | TaskExecutionOutcomeStatus::Blocked
    ) {
        return Err(
            "task execution outcome review may only return succeeded or blocked".to_string(),
        );
    }
    outcome.validate()?;
    Ok(outcome)
}

fn persistent_review_checkpoint(
    active_review: &parking_lot::Mutex<Option<TaskExecutionReviewCheckpoint>>,
    detected_checkpoint: Option<TaskExecutionReviewCheckpoint>,
) -> Option<TaskExecutionReviewCheckpoint> {
    let mut active_review = active_review.lock();
    if let Some(checkpoint) = detected_checkpoint {
        *active_review = Some(checkpoint);
    }
    *active_review
}

fn checkpoint_guidance_message(
    checkpoint: TaskExecutionReviewCheckpoint,
    confirmed_project_paths: &[String],
) -> Value {
    let decision_focus = match checkpoint.trigger {
        TaskExecutionReviewTrigger::ReadOnlyLoop =>
            "归并现有文件与命令证据：如果它们已覆盖全部硬性验收项就 COMPLETE；否则锁定依赖顺序中第一项未满足要求，并选择 IMPLEMENT 或 VERIFY。",
        TaskExecutionReviewTrigger::MissingTargetedReads =>
            "已有工具结果否定了当前路径假设。选择 LOCATE，只执行一次限定目录、关键词和预期命中的定位动作；定位成功后直接转入 IMPLEMENT。",
        TaskExecutionReviewTrigger::PlaceholderProgressWrite =>
            "占位产物不是验收证据。忽略它并锁定第一项真实业务缺口；修改对应项目文件，或在既有证据完整时 COMPLETE。",
        TaskExecutionReviewTrigger::StaleProjectWrite =>
            "已有失败结果表明最近一次代码写入未生效。选择 IMPLEMENT，把最近一次成功读取的目标内容作为权威版本，直接生成基于该文本的精确编辑；写入成功后转入必要验证。",
    };
    let confirmed_path_guidance = if confirmed_project_paths.is_empty() {
        "当前还没有来自成功读取、搜索或修改结果的已确认项目路径。只有现有证据确实无法定位实现时，只允许执行一次限定目录、关键词和预期命中的 LOCATE 动作；该动作返回后必须使用新证据进入 IMPLEMENT、VERIFY、COMPLETE 或 BLOCKED，不得继续扩大搜索范围。".to_string()
    } else {
        format!(
            "以下路径已由成功的读取、搜索或修改工具结果确认，可直接作为后续动作的项目路径索引：{}。必须直接复用这些路径，不得从任务描述或自然语言摘要重新猜测路径，也不得仅为确认它们存在而重复全仓搜索。",
            serde_json::to_string(confirmed_project_paths)
                .expect("confirmed project paths must serialize")
        )
    };
    json!({
        "role": "system",
        "content": format!(
            "[工程决策复盘]\n\
             你现在承担工程复盘决策角色。当前上下文已经提供任务目标、验收标准、已读取文件、已执行命令及其结果。你的职责是依据这些证据替执行过程选定并推进下一步，而不是评价先前行为、提醒发生了重复，或输出复盘说明。\n\
             本次决策重点：{decision_focus}\n\
             路径证据：{confirmed_path_guidance}\n\
             \n\
             请在内部完成决策，不向用户展示分析草稿：\n\
             1. 重建验收契约：从任务目标和验收标准中提取硬性要求，使用现有文件内容、函数实现、命令、退出码和错误结果逐项判断。每个结论都必须有具体证据；没有证据的要求视为未满足。\n\
             2. 选择依赖顺序中第一项未满足要求。若有多个候选，选择一次动作最能直接关闭的缺口；若没有缺口且必要验证已通过，选择 COMPLETE。\n\
             3. 只选择一种状态：\n\
                - IMPLEMENT：已确认存在实现缺口。指令必须点明目标文件或函数、要修改的行为以及修改后的完成判据；下一步直接修改真实项目文件。\n\
                - LOCATE：只有现有证据无法定位真实实现时才能选择。指令必须限定一次目录列举或文本搜索的范围、关键词和期望定位结果。\n\
                - VERIFY：实现证据已存在，但最近一次真实修改之后仍缺一项必要验证。指令必须给出唯一的验证命令和明确通过条件。\n\
                - COMPLETE：全部硬性验收项已有代码证据，且相关必要验证已经通过。立即停止调用工具并输出最终结果。\n\
                - BLOCKED：只有外部输入、权限或服务状态确实阻止继续时才能选择。指令必须引用具体失败证据并说明需要什么变化；不得用 BLOCKED 代替困难实现。\n\
             4. 在内部形成唯一指令：`状态 + 证据依据 + 未满足的验收项 + 目标文件/函数或唯一命令 + 具体动作 + 完成判据`。若已有证据足以定位修改点，直接编辑；若已有成功结果足以证明验证项，直接采用该结果。\n\
             5. 做出判断后，下一条可见输出只能是执行该指令的一次工具调用，或 COMPLETE/BLOCKED 对应的最终结论；禁止输出批评、警告、过程复述或“需要继续检查”之类没有动作参数的文字。\n\
             6. 工具结果返回后继续使用同一决策契约：完成判据满足就进入 VERIFY 或 COMPLETE；失败就引用新失败的具体原因给出纠正后的 IMPLEMENT、LOCATE、VERIFY 或 BLOCKED 动作，不得只报告失败。\n\
             7. 相同代码状态下，已经成功的等价命令结果就是有效证据；除非代码发生变化或验证目标不同，不得重复运行或仅改写命令形式。工具始终完整可用，不得创建假进展文件或要求用户代为完成工程动作。"
        ),
    })
}

impl RunService {
    pub(super) fn build_runtime_execution_state(
        &self,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        run_spec: &TaskRunSpec,
        tool_result_model_budget_limits: ToolResultModelBudgetLimits,
        max_iterations: usize,
        review_policy: TaskExecutionReviewPolicy,
        requires_execution: bool,
        effective_workspace_dir: &str,
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
                requires_execution,
                self.store.clone(),
                run.id.clone(),
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
