// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod completion;

use std::sync::Arc;

use async_trait::async_trait;
use chatos_ai_runtime::{
    AiRuntimeOptions, RuntimeBeforeModelRequest, RuntimeIterationContext, RuntimeLifecycleHook,
    RuntimeRecordOptions, TaskExecutionProgressState, TaskExecutionReviewCheckpoint,
    TaskExecutionReviewPolicy, TaskExecutionReviewTrigger, TaskFinalizationLifecycleHook,
    TaskMcpInitMode, TaskRunExecution, TaskRunSpec, TaskRuntime, TaskRuntimeConfig,
};
use chatos_plugin_management_sdk::{required_agent_prompt_vendor, SystemAgentKey};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::local_runtime::capabilities::merge_system_prompts;
use crate::local_runtime::chat::{
    build_local_memory_context_input_items, prepare_local_chat_tools, LocalChatEventStream,
    LocalChatRecordWriter,
};
use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::{
    AppendLocalRuntimeEventInput, BeginLocalBackgroundTurnInput, BeginLocalTurnInput,
    BeginLocalTurnResult, LocalDatabase,
};
use crate::local_runtime::task_runner::LocalTaskRunRecord;
use crate::local_runtime::{
    load_installed_agent_prompt, managed_task_runner_runtime_settings, run_active_task_review,
};
use crate::model_configs::resolve_local_model_runtime;
use crate::terminal::controller::{
    local_terminal_controller_context_for_task_run, LocalConnectorTerminalControllerStore,
};
use crate::LocalRuntime;

#[cfg(test)]
pub(super) use self::completion::complete_requirement_if_done;
use self::completion::finish_task_run;
pub(crate) use self::completion::user_visible_task_run_failure_receipt;
pub(super) use self::completion::{
    finalize_task_manager_session, persist_task_run_receipt, set_requirement_status,
    set_work_item_status,
};

pub(super) async fn execute_local_task_run(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    abort_token: CancellationToken,
) -> Result<(), String> {
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    database
        .get_session(run.session_id.as_str(), run.owner_user_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Task Runner session was not found".to_string())?;
    let project = database
        .get_project(run.project_id.as_str(), run.owner_user_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Task Runner project was not found".to_string())?;
    let settings = database
        .get_runtime_settings(run.owner_user_id.as_str(), run.session_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Task Runner settings were not found".to_string())?;
    let conversation_task = if run.task_kind == "conversation_task" {
        Some(
            database
                .get_local_task_board_task(
                    run.owner_user_id.as_str(),
                    run.session_id.as_str(),
                    run.task_id.as_str(),
                )
                .await
                .map_err(|error| error.to_string())?
                .filter(|task| task.task_kind == "task_runner")
                .ok_or_else(|| "Local Task Runner conversation task was not found".to_string())?,
        )
    } else {
        None
    };
    let work_item = if conversation_task.is_none() {
        Some(
            database
                .get_local_work_item(run.owner_user_id.as_str(), run.task_id.as_str())
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Local Task Runner work item was not found".to_string())?,
        )
    } else {
        None
    };
    let is_planning_task = conversation_task
        .as_ref()
        .map(|task| task.is_planning_task)
        .or_else(|| work_item.as_ref().map(|task| task.is_planning_task))
        .unwrap_or(false);
    let agent_key = if is_planning_task {
        SystemAgentKey::TaskRunnerPlanPhase
    } else {
        SystemAgentKey::TaskRunnerRunPhase
    };
    let mut task_settings = settings.clone();
    task_settings.selected_model_id = Some(run.model_config_id.clone());
    task_settings.plan_mode_enabled = is_planning_task;
    if let Some(task) = conversation_task.as_ref() {
        task_settings.mcp_enabled = true;
        task_settings.enabled_mcp_ids_json = serde_json::to_string(&conversation_task_mcp_ids(
            task.enabled_builtin_kinds.as_slice(),
            task.external_mcp_config_ids.as_slice(),
        )?)
        .map_err(|error| error.to_string())?;
        task_settings.selected_skill_ids_json =
            serde_json::to_string(&task.selected_skill_ids).map_err(|error| error.to_string())?;
    }
    begin_task_turn(database, run).await?;
    set_work_item_status(runtime, run, "in_progress").await?;
    let prepared = prepare_local_chat_tools(
        runtime,
        run.owner_user_id.as_str(),
        run.id.as_str(),
        &project,
        &task_settings,
        agent_key,
        conversation_task.is_none(),
        &[],
    )
    .await?;
    let resolved_model = {
        let state = runtime.state.read().await;
        resolve_local_model_runtime(
            &state,
            run.owner_user_id.as_str(),
            run.model_config_id.as_str(),
        )
        .map_err(|error| error.to_string())?
    };
    let model_name = resolved_model.model.clone();
    let prompt_vendor = required_agent_prompt_vendor(
        resolved_model.prompt_vendor.as_deref(),
        resolved_model.provider.as_str(),
    )
    .map_err(|error| error.to_string())?;
    let installed_prompt = load_installed_agent_prompt(runtime, agent_key, prompt_vendor)
        .await
        .map_err(|error| error.to_string())?;
    let model = build_local_model_config(
        resolved_model,
        merge_system_prompts(
            Some(installed_prompt.content),
            prepared.capability_prompt.clone(),
        ),
        task_settings.selected_thinking_level.clone(),
        None,
        true,
        Some(prepared.project_root.display().to_string()),
    );
    let task_runner_runtime_settings = managed_task_runner_runtime_settings(runtime).await;
    let mut builder = TaskRuntime::builder()
        .with_record_writer_arc(Arc::new(LocalChatRecordWriter::new(
            database.clone(),
            run.owner_user_id.as_str(),
            run.session_id.as_str(),
            run.turn_id.as_str(),
        )))
        .with_max_iterations(task_runner_runtime_settings.max_iterations);
    if let Some(executor) = prepared.executor {
        builder = builder.with_tool_executor_arc(executor);
    }
    let spec = TaskRunSpec::new(
        run.task_id.clone(),
        run.id.clone(),
        model.clone(),
        run.prompt.clone(),
    )
    .with_model_config_id(run.model_config_id.clone())
    .with_tools(prepared.available_tools);
    let execution = TaskRunExecution::new(
        TaskRuntimeConfig::new().with_mcp_init_mode(TaskMcpInitMode::Disabled),
        spec,
    );
    let events = LocalChatEventStream::start(
        database.clone(),
        run.owner_user_id.as_str(),
        run.session_id.as_str(),
        run.turn_id.as_str(),
    );
    events.publish(
        "task.run.started",
        Some("status"),
        json!({ "run_id": run.id }),
    );
    let progress = Arc::new(TaskExecutionProgressState::new(
        TaskExecutionReviewPolicy::new(
            task_runner_runtime_settings.review_read_only_iterations,
            task_runner_runtime_settings.review_missing_read_failures,
            task_runner_runtime_settings.review_repeat_interval_iterations,
        ),
    ));
    let lifecycle_hook = Arc::new(LocalTaskRunnerLifecycleHook::new(
        task_runner_runtime_settings.max_iterations,
        Arc::clone(&progress),
        runtime.clone(),
        database.clone(),
        run.owner_user_id.clone(),
        run.session_id.clone(),
        run.turn_id.clone(),
        settings.memory_recall_limit,
    ));
    let mut callbacks = events.callbacks();
    callbacks.on_tools_stream = Some(Arc::new({
        let progress = Arc::clone(&progress);
        let original = callbacks.on_tools_stream.clone();
        move |payload| {
            progress.observe_tool_result(&payload);
            if let Some(callback) = &original {
                callback(payload);
            }
        }
    }));
    let report = execution
        .run_report_with_runtime_options(
            &builder.build(),
            AiRuntimeOptions::new(Some(run.session_id.clone()), Some(run.turn_id.clone()))
                .with_caller_model(Some(model_name))
                .with_caller_model_runtime(Some(model.to_tool_caller_model_runtime()))
                .with_abort_token(Some(abort_token.clone()))
                .with_lifecycle_hook(Some(lifecycle_hook))
                .with_callbacks(callbacks)
                .with_record_options(RuntimeRecordOptions::persist_all()),
        )
        .await;
    let _ = events.finish().await;
    let cleanup_context = local_terminal_controller_context_for_task_run(
        prepared.project_root.as_path(),
        run.owner_user_id.as_str(),
        run.id.as_str(),
        30_000,
    );
    if let Err(error) = LocalConnectorTerminalControllerStore
        .kill_sessions_for_context(cleanup_context)
        .await
    {
        crate::tracing_stdout(
            format!("local task run {} terminal cleanup failed: {error}", run.id).as_str(),
        );
    }
    let cancel_requested = database
        .local_task_run_cancel_requested(run.id.as_str())
        .await
        .unwrap_or(false);
    finish_task_run(runtime, run, report, cancel_requested).await
}

const LOCAL_TASK_RUNNER_REVIEW_TRIGGER_TYPE: &str = "task_runner_execution_review_checkpoint";

struct LocalTaskRunnerLifecycleHook {
    finalization: TaskFinalizationLifecycleHook,
    progress: Arc<TaskExecutionProgressState>,
    runtime: LocalRuntime,
    database: LocalDatabase,
    owner_user_id: String,
    session_id: String,
    turn_id: String,
    memory_recall_limit: i64,
}

impl LocalTaskRunnerLifecycleHook {
    fn new(
        max_iterations: usize,
        progress: Arc<TaskExecutionProgressState>,
        runtime: LocalRuntime,
        database: LocalDatabase,
        owner_user_id: String,
        session_id: String,
        turn_id: String,
        memory_recall_limit: i64,
    ) -> Self {
        Self {
            finalization: TaskFinalizationLifecycleHook::new(max_iterations),
            progress,
            runtime,
            database,
            owner_user_id,
            session_id,
            turn_id,
            memory_recall_limit,
        }
    }

    async fn stable_memory_input_items(&self) -> Result<Vec<Value>, String> {
        let context = self
            .database
            .load_memory_context(
                self.owner_user_id.as_str(),
                self.session_id.as_str(),
                self.memory_recall_limit,
            )
            .await
            .map_err(|error| error.to_string())?;
        let task_board = self
            .database
            .local_task_board_prompt(self.owner_user_id.as_str(), self.session_id.as_str())
            .await
            .map_err(|error| error.to_string())?;
        Ok(build_local_memory_context_input_items(
            context.summary,
            context.recalls,
            context
                .messages
                .into_iter()
                .filter(|message| message.turn_id.as_deref() != Some(self.turn_id.as_str()))
                .collect(),
            task_board,
        ))
    }

    async fn full_memory_input_items(&self) -> Result<Vec<Value>, String> {
        let context = self
            .database
            .load_memory_context(
                self.owner_user_id.as_str(),
                self.session_id.as_str(),
                self.memory_recall_limit,
            )
            .await
            .map_err(|error| error.to_string())?;
        let task_board = self
            .database
            .local_task_board_prompt(self.owner_user_id.as_str(), self.session_id.as_str())
            .await
            .map_err(|error| error.to_string())?;
        Ok(build_local_memory_context_input_items(
            context.summary,
            context.recalls,
            context
                .messages
                .into_iter()
                .filter(|message| message.turn_id.as_deref() != Some(self.turn_id.as_str()))
                .collect(),
            task_board,
        ))
    }

    async fn checkpoint_input_items(
        &self,
        checkpoint: TaskExecutionReviewCheckpoint,
    ) -> Vec<Value> {
        self.append_runtime_event(
            "task.execution_review.summary.started",
            json!({
                "iteration": checkpoint.iteration,
                "trigger": checkpoint.trigger.as_str(),
            }),
        )
        .await;
        match run_active_task_review(
            &self.runtime,
            self.owner_user_id.as_str(),
            self.session_id.as_str(),
            self.turn_id.as_str(),
            LOCAL_TASK_RUNNER_REVIEW_TRIGGER_TYPE,
        )
        .await
        {
            Ok(result) => match self.full_memory_input_items().await {
                Ok(memory_items) => {
                    self.append_runtime_event(
                        "task.execution_review.summary.completed",
                        json!({
                            "iteration": checkpoint.iteration,
                            "trigger": checkpoint.trigger.as_str(),
                            "generated_summaries": result.generated_summaries,
                            "marked_messages": result.marked_messages,
                            "pending_message_count": result.pending_message_count,
                            "refreshed_memory_item_count": memory_items.len(),
                        }),
                    )
                    .await;
                    let mut items = vec![local_checkpoint_guidance_message(checkpoint, true, None)];
                    items.extend(memory_items);
                    items
                }
                Err(error) => {
                    self.append_runtime_event(
                        "task.execution_review.summary.failed",
                        json!({
                            "iteration": checkpoint.iteration,
                            "trigger": checkpoint.trigger.as_str(),
                            "error": error.as_str(),
                        }),
                    )
                    .await;
                    vec![local_checkpoint_guidance_message(
                        checkpoint,
                        false,
                        Some(error.as_str()),
                    )]
                }
            },
            Err(error) => {
                let error = error.to_string();
                self.append_runtime_event(
                    "task.execution_review.summary.failed",
                    json!({
                        "iteration": checkpoint.iteration,
                        "trigger": checkpoint.trigger.as_str(),
                        "error": error.as_str(),
                    }),
                )
                .await;
                vec![local_checkpoint_guidance_message(
                    checkpoint,
                    false,
                    Some(error.as_str()),
                )]
            }
        }
    }

    async fn append_runtime_event(&self, event_name: &str, payload: Value) {
        let _ = self
            .database
            .append_runtime_event(AppendLocalRuntimeEventInput {
                owner_user_id: self.owner_user_id.clone(),
                session_id: self.session_id.clone(),
                turn_id: self.turn_id.clone(),
                event_name: event_name.to_string(),
                stream_type: Some("status".to_string()),
                payload,
            })
            .await;
    }
}

#[async_trait]
impl RuntimeLifecycleHook for LocalTaskRunnerLifecycleHook {
    async fn before_model_request(
        &self,
        context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        self.progress.begin_iteration(context.iteration);
        let iteration = context.iteration;
        let mut before = self.finalization.before_model_request(context).await?;

        if before.tools_enabled {
            if let Some(checkpoint) = self.progress.should_trigger_review(iteration) {
                self.append_runtime_event(
                    "task.execution_review.checkpoint",
                    json!({
                        "iteration": iteration,
                        "trigger": checkpoint.trigger.as_str(),
                        "read_only_iterations": checkpoint.read_only_iterations,
                        "missing_read_failures": checkpoint.missing_read_failures,
                        "policy": {
                            "read_only_iterations": checkpoint.policy.read_only_iterations,
                            "missing_read_failures": checkpoint.policy.missing_read_failures,
                            "repeat_interval_iterations": checkpoint.policy.repeat_interval_iterations,
                        },
                        "disabled_tool_names": [],
                    }),
                )
                .await;
                before
                    .input_items
                    .extend(self.checkpoint_input_items(checkpoint).await);
                return Ok(before);
            }
        }

        match self.stable_memory_input_items().await {
            Ok(items) => before.input_items.extend(items),
            Err(error) => {
                self.append_runtime_event(
                    "task.memory_context_refresh.failed",
                    json!({
                        "iteration": iteration,
                        "error": error,
                    }),
                )
                .await;
            }
        }
        Ok(before)
    }
}

fn local_checkpoint_guidance_message(
    checkpoint: TaskExecutionReviewCheckpoint,
    memory_refreshed: bool,
    summary_error: Option<&str>,
) -> Value {
    let trigger = match checkpoint.trigger {
        TaskExecutionReviewTrigger::ReadOnlyLoop => "连续多轮只读/观察，没有真实工程改动",
        TaskExecutionReviewTrigger::MissingTargetedReads => {
            "连续读取不存在的精确文件路径，疑似路径假设错误或相对路径理解错误"
        }
        TaskExecutionReviewTrigger::PlaceholderProgressWrite => {
            "写入了 progress/unlock/placeholder 这类不能解决任务本身的占位文件"
        }
    };
    let memory_state = if memory_refreshed {
        "已先触发本地历史动作复盘，并已把复盘后的 Memory 上下文刷新进本次请求。"
    } else {
        "尝试触发本地历史动作复盘但未能刷新 Memory；仍需立刻基于已有上下文自我校准。"
    };
    let error_detail = summary_error
        .map(|error| format!("\n- 复盘刷新错误：{error}"))
        .unwrap_or_default();
    json!({
        "role": "system",
        "content": format!(
            "[Task Runner 自动复盘 checkpoint]\n\
             检测原因：{trigger}。\n\
             {memory_state}{error_detail}\n\
             \n\
             现在先在心里复盘：用户目标是什么、当前已经做了哪些真实动作、哪些动作偏离航线、真实路径/工具结果已经证明了什么。\n\
             然后继续执行，不要因为这次 checkpoint 自行退出、不要把它当成权限限制、不要要求用户替你改代码。\n\
             工具没有被禁用；如果文件不存在，把它当作路径证据，不要重复读同一个不存在路径。所有代码工具路径都按仓库根目录相对路径理解，没有隐式 cwd。\n\
             不要创建 TASK_RUNNER_PROGRESS_NOTE、unlock、placeholder、probe 之类的假进展文件；只有修改真实项目文件、运行必要验证、或给出有证据的终态结论才算进展。\n\
             当前计数：read_only_iterations={}, missing_read_failures={}。"
            ,
            checkpoint.read_only_iterations,
            checkpoint.missing_read_failures
        ),
    })
}

async fn begin_task_turn(
    database: &crate::local_runtime::LocalDatabase,
    run: &LocalTaskRunRecord,
) -> Result<(), String> {
    if run.task_kind == "conversation_task" {
        let task = database
            .get_local_task_board_task(
                run.owner_user_id.as_str(),
                run.session_id.as_str(),
                run.task_id.as_str(),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Local Task Runner conversation task was not found".to_string())?;
        return match database
            .begin_background_turn(BeginLocalBackgroundTurnInput {
                session_id: run.session_id.clone(),
                owner_user_id: run.owner_user_id.clone(),
                source_turn_id: task.source_turn_id,
                turn_id: run.turn_id.clone(),
                idempotency_key: task_run_idempotency_key(run.id.as_str(), run.attempt),
            })
            .await
            .map_err(|error| error.to_string())?
        {
            BeginLocalTurnResult::Started(_) => Ok(()),
            BeginLocalTurnResult::Existing(snapshot) if snapshot.turn.status == "completed" => {
                Ok(())
            }
            BeginLocalTurnResult::Existing(_) => {
                Err("Local Task Runner background turn already exists".to_string())
            }
        };
    }
    match database
        .begin_turn(BeginLocalTurnInput {
            session_id: run.session_id.clone(),
            owner_user_id: run.owner_user_id.clone(),
            turn_id: run.turn_id.clone(),
            idempotency_key: task_run_idempotency_key(run.id.as_str(), run.attempt),
            content: run.prompt.clone(),
            metadata_json: Some(
                json!({
                    "runtime_origin": "local_device", "message_mode": "task_run",
                    "task_id": run.task_id, "run_id": run.id,
                })
                .to_string(),
            ),
        })
        .await
        .map_err(|error| error.to_string())?
    {
        BeginLocalTurnResult::Started(_) => Ok(()),
        BeginLocalTurnResult::Existing(snapshot) if snapshot.turn.status == "completed" => Ok(()),
        BeginLocalTurnResult::Existing(_) => {
            Err("Local Task Runner turn already exists".to_string())
        }
    }
}

fn conversation_task_mcp_ids(
    builtin_kinds: &[String],
    external_mcp_config_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for value in builtin_kinds {
        let kind = chatos_mcp_runtime::builtin_kind_by_any(value.as_str())
            .ok_or_else(|| format!("Unknown local Task Runner builtin capability: {value}"))?;
        let descriptor = chatos_mcp::system_mcp_descriptor_by_embedded_kind(kind)
            .ok_or_else(|| format!("Missing system MCP descriptor for {value}"))?;
        ids.push(descriptor.resource_id.to_string());
    }
    for id in external_mcp_config_ids {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    Ok(ids)
}

fn task_run_idempotency_key(run_id: &str, attempt: i64) -> String {
    format!("{run_id}:attempt:{}", attempt.max(1))
}

#[cfg(test)]
mod execution_policy_tests {
    use chatos_ai_runtime::{
        RuntimeIterationContext, RuntimeLifecycleHook, TaskFinalizationLifecycleHook,
        DEFAULT_TASK_RUN_MAX_ITERATIONS,
    };
    use serde_json::Value;

    use super::task_run_idempotency_key;

    #[tokio::test]
    async fn implementation_tasks_reserve_a_tool_free_finalization_round() {
        assert_eq!(DEFAULT_TASK_RUN_MAX_ITERATIONS, 600);
        let hook = TaskFinalizationLifecycleHook::new(DEFAULT_TASK_RUN_MAX_ITERATIONS);

        let normal = hook
            .before_model_request(iteration_context(hook.finalization_iteration() - 1))
            .await
            .expect("normal task iteration");
        assert!(normal.tools_enabled);
        assert!(normal.input_items.is_empty());

        let finalization = hook
            .before_model_request(iteration_context(hook.finalization_iteration()))
            .await
            .expect("task finalization iteration");
        assert!(!finalization.tools_enabled);
        assert_eq!(finalization.input_items.len(), 1);
        assert!(finalization.input_items[0]
            .to_string()
            .contains("不要再调用任何工具"));
    }

    #[test]
    fn retry_attempts_use_distinct_turn_idempotency_keys() {
        assert_ne!(
            task_run_idempotency_key("run-1", 1),
            task_run_idempotency_key("run-1", 2),
        );
    }

    fn iteration_context(iteration: usize) -> RuntimeIterationContext {
        RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration,
            reason: "tool_results".to_string(),
            input: Value::Array(Vec::new()),
        }
    }
}
