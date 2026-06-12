use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chatos_ai_runtime::{
    AiRuntimeOptions, AiTurnReport, MemoryRecordScope, MemoryScope, RuntimeCallbacks,
    RuntimeRecordOptions, SaveRecordInput, TaskMemoryRuntimeConfig, TaskRunExecution,
    TaskRunReport, TaskRunSpec, TaskRuntimeConfig,
};
use chatos_mcp_runtime::{builtin_servers_from_kinds, BuiltinMcpServerOptions, McpExecutorBuilder};
use serde_json::json;
use tracing::{info, warn};

use crate::models::{
    now_rfc3339, ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskStatus,
};

use super::prerequisite_context::{
    attach_prerequisite_context_to_run, build_task_prompt, PrerequisiteTaskContext,
};
use super::stream_events::{
    append_pending_stream_event, flush_pending_stream_event, PendingRunStreamEvent,
};
use super::task_process_log::{
    task_process_log_builtin_server, task_process_log_prefixed_input_items,
    task_process_logging_enabled, TaskProcessLogBuiltinProvider,
    TASK_PROCESS_LOG_INTERNAL_SERVER_NAME,
};
use super::workspace_mcp::selected_builtin_kinds;
use super::{build_builtin_registry, summarized_report_content, RunService, TaskService};

impl RunService {
    pub(super) async fn execute_run_model_phase(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
        prerequisite_context: Vec<PrerequisiteTaskContext>,
    ) {
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            model_config_id = model_config.id.as_str(),
            model = model_config.model.as_str(),
            provider = model_config.provider.as_str(),
            workspace_dir = effective_workspace_dir.as_str(),
            prompt_override = input.prompt_override.as_deref().unwrap_or(""),
            "task runner begin execute_run"
        );
        if self.store.is_cancel_requested(&run.id) {
            self.finish_cancelled_before_start(&task, &mut run).await;
            return;
        }

        run.status = TaskRunStatus::Running;
        run.started_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist running task run {}: {}", run.id, err);
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "running",
                Some("任务开始执行".to_string()),
                None,
            ))
            .await
        {
            warn!("failed to append running event for run {}: {}", run.id, err);
        }

        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Running;
            task_record.updated_at = now_rfc3339();
            task_record.last_run_id = Some(run.id.clone());
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist running task {}: {}", task.id, err);
            }
        }
        if !prerequisite_context.is_empty() {
            attach_prerequisite_context_to_run(&mut run, &prerequisite_context);
            run.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to persist prerequisite context for run {}: {}",
                    run.id, err
                );
            }
        }

        let prompt = build_task_prompt(
            &task,
            input.prompt_override.as_deref(),
            &prerequisite_context,
        );
        let mut effective_model_config = model_config.clone();
        effective_model_config.request_cwd = Some(effective_workspace_dir.clone());
        let model_runtime_config =
            effective_model_config.to_runtime_config(Some(effective_workspace_dir.clone()));
        let metadata = json!({
            "task_id": task.id,
            "run_id": run.id,
            "model_config_id": model_config.id,
            "service": "task_runner_service",
        });
        let task_process_logging_enabled = task_process_logging_enabled(&task.mcp_config);

        let mut run_spec = TaskRunSpec::new(
            task.id.clone(),
            run.id.clone(),
            model_runtime_config.clone(),
            prompt.clone(),
        )
        .with_model_config_id(model_config.id.clone())
        .with_metadata(Some(metadata.clone()))
        .with_record_options(
            RuntimeRecordOptions::persist_all()
                .with_assistant_message_mode("task_run")
                .with_assistant_message_source("task_runner")
                .with_tool_message_mode("task_tool")
                .with_tool_message_source("task_runner")
                .with_assistant_metadata(metadata.clone())
                .with_tool_metadata(metadata.clone()),
        )
        .with_user_record(Some(
            SaveRecordInput::user_message(run.id.clone(), prompt.clone())
                .with_conversation_turn_id(run.id.clone())
                .with_message_mode("task_run")
                .with_message_source("task_runner")
                .with_metadata(metadata.clone()),
        ));
        if task_process_logging_enabled {
            run_spec = run_spec.with_prefixed_input_items(task_process_log_prefixed_input_items(
                task.mcp_config.locale(),
            ));
        }

        let memory_scope = MemoryScope::thread(
            task.tenant_id.clone(),
            self.config.memory_engine_source_id.clone(),
            task.memory_thread_id.clone(),
        )
        .with_subject_id(task.subject_id.clone());
        run_spec = run_spec.with_memory_scope(Some(memory_scope));

        let max_iterations = match self.effective_task_execution_max_iterations().await {
            Ok(value) => value,
            Err(err) => {
                self.finish_failed_before_execution(
                    &task,
                    &mut run,
                    format!("加载运行时配置失败: {err}"),
                )
                .await;
                return;
            }
        };
        let tool_result_model_budget_limits =
            match self.effective_tool_result_model_budget_limits().await {
                Ok(value) => value,
                Err(err) => {
                    self.finish_failed_before_execution(
                        &task,
                        &mut run,
                        format!("加载运行时配置失败: {err}"),
                    )
                    .await;
                    return;
                }
            };

        let mut runtime_config = TaskRuntimeConfig::new().with_max_iterations(Some(max_iterations));
        if let Some(memory_engine_base_url) = self.config.memory_engine_base_url.clone() {
            runtime_config = runtime_config.with_memory_engine(Some(
                TaskMemoryRuntimeConfig::new(
                    memory_engine_base_url,
                    self.config.memory_engine_source_id.clone(),
                )
                .with_timeout_ms(self.config.memory_timeout.as_millis() as u64)
                .with_record_scope(Some(MemoryRecordScope::message_thread(
                    task.tenant_id.clone(),
                    task.memory_thread_id.clone(),
                ))),
            ));
        }

        let runtime_config = self.apply_task_mcp_config(runtime_config, &task.mcp_config);
        if let Some(snapshot) = self
            .compose_context_snapshot(run_spec.memory_scope.as_ref())
            .await
        {
            run.context_snapshot = Some(snapshot);
            run.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to persist context snapshot for run {}: {}",
                    run.id, err
                );
            }
        }
        let selected_builtin_kinds = selected_builtin_kinds(&task.mcp_config);
        let mut server_options = BuiltinMcpServerOptions::new(effective_workspace_dir)
            .with_user_id(task.subject_id.clone())
            .with_project_id(task.id.clone())
            .with_auto_create_task(true);
        if let Some(remote_server_id) = task.mcp_config.default_remote_server_id.clone() {
            server_options = server_options.with_remote_connection_id(remote_server_id);
        }
        let mut builtin_servers =
            builtin_servers_from_kinds(selected_builtin_kinds.clone(), &server_options);
        if task_process_logging_enabled {
            builtin_servers.push(task_process_log_builtin_server());
        }
        let (builtin_registry, builtin_init_errors) = build_builtin_registry(
            &builtin_servers,
            TaskService::new(self.config.clone(), self.store.clone()),
            self.ui_prompt_service.clone(),
        );
        let mut builtin_registry = builtin_registry;
        if task_process_logging_enabled {
            builtin_registry.register(TaskProcessLogBuiltinProvider::new(
                TASK_PROCESS_LOG_INTERNAL_SERVER_NAME,
                TaskService::new(self.config.clone(), self.store.clone()),
                task.id.clone(),
                run.id.clone(),
            ));
        }
        for err in builtin_init_errors {
            if let Err(event_err) = self
                .store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    "builtin_provider_warning",
                    Some(err.clone()),
                    None,
                ))
                .await
            {
                warn!(
                    "failed to append builtin warning event for run {}: {}",
                    run.id, event_err
                );
            }
            warn!("task runner builtin provider warning: {err}");
        }
        let mcp_builder = McpExecutorBuilder::new()
            .with_builtin_servers(builtin_servers)
            .with_builtin_registry(builtin_registry);

        let store_for_callbacks = self.store.clone();
        let run_id_for_chunk = run.id.clone();
        let pending_stream_event =
            Arc::new(parking_lot::Mutex::new(PendingRunStreamEvent::default()));

        let callbacks = RuntimeCallbacks {
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
                let run_id = run.id.clone();
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
                let run_id = run.id.clone();
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
                let run_id = run.id.clone();
                move |payload| {
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
                let run_id = run.id.clone();
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
                let store = store_for_callbacks.clone();
                let run_id = run.id.clone();
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
        };

        let cancel_requested = Arc::new(AtomicBool::new(self.store.is_cancel_requested(&run.id)));
        let stop_cancel_poll = Arc::new(AtomicBool::new(false));
        let cancel_poll_handle = tokio::spawn({
            let store = self.store.clone();
            let run_id = run.id.clone();
            let cancel_requested = Arc::clone(&cancel_requested);
            let stop_cancel_poll = Arc::clone(&stop_cancel_poll);
            async move {
                while !stop_cancel_poll.load(Ordering::Relaxed) {
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
                    tokio::time::sleep(super::RUN_CANCEL_POLL_INTERVAL).await;
                }
            }
        });

        let runtime_options = AiRuntimeOptions::new(Some(run.id.clone()), Some(run.id.clone()))
            .with_caller_model(Some(model_config.model.clone()))
            .with_record_options(run_spec.record_options.clone())
            .with_tool_result_model_budget_limits(Some(tool_result_model_budget_limits))
            .with_callbacks(callbacks)
            .with_abort_checker(Some(Arc::new({
                let cancel_requested = Arc::clone(&cancel_requested);
                move |_| cancel_requested.load(Ordering::Relaxed)
            })));

        let execution = TaskRunExecution::new(runtime_config, run_spec);
        let report = match tokio::time::timeout(
            self.config.execution_timeout,
            execution.run_report_with_mcp_builder_and_options(mcp_builder, runtime_options),
        )
        .await
        {
            Ok(report) => report,
            Err(_) => TaskRunReport::from_ai_report(
                task.id.clone(),
                run.id.clone(),
                Some(model_config.id.clone()),
                AiTurnReport::failed(format!(
                    "execution timed out after {} seconds",
                    self.config.execution_timeout.as_secs()
                )),
            ),
        };
        stop_cancel_poll.store(true, Ordering::Relaxed);
        cancel_poll_handle.abort();
        flush_pending_stream_event(&self.store, run.id.as_str(), &pending_stream_event);

        let report_json = serde_json::to_value(&report).ok();
        let result_summary = summarized_report_content(&report.content);
        run.updated_at = now_rfc3339();
        run.finished_at = Some(report.completed_at.clone());
        run.result_summary = result_summary.clone();
        run.error_message = report.error.clone();
        run.usage = report.usage.clone();
        run.report = report_json.clone();
        run.cancel_requested = false;
        run.status = match report.status {
            chatos_ai_runtime::AiTurnStatus::Completed => TaskRunStatus::Succeeded,
            chatos_ai_runtime::AiTurnStatus::Failed => TaskRunStatus::Failed,
            chatos_ai_runtime::AiTurnStatus::Aborted => TaskRunStatus::Cancelled,
        };
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist completed task run {}: {}", run.id, err);
        }

        let event_type = match run.status {
            TaskRunStatus::Succeeded => "completed",
            TaskRunStatus::Failed => "failed",
            TaskRunStatus::Cancelled => "cancelled",
            TaskRunStatus::Blocked => "blocked",
            TaskRunStatus::Queued | TaskRunStatus::Running => "finished",
        };
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                event_type,
                Some(report.user_message()),
                report_json.clone(),
            ))
            .await
        {
            warn!(
                "failed to append completion event for run {}: {}",
                run.id, err
            );
        }

        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = match run.status {
                TaskRunStatus::Succeeded => TaskStatus::Succeeded,
                TaskRunStatus::Failed => TaskStatus::Failed,
                TaskRunStatus::Cancelled => TaskStatus::Cancelled,
                TaskRunStatus::Blocked => TaskStatus::Blocked,
                TaskRunStatus::Queued | TaskRunStatus::Running => TaskStatus::Running,
            };
            task_record.result_summary = result_summary;
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist completed task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), &run)
            .await;

        if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && self.config.auto_memory_summary
        {
            if let Err(err) = self.trigger_memory_summary(&task, &mut run).await {
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "memory_summary_error",
                        Some(format!("触发 Memory Engine 总结失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append memory summary error event for run {}: {}",
                        run.id, event_err
                    );
                }
                warn!(
                    "failed to trigger memory summary for run {}: {}",
                    run.id, err
                );
            }
        } else if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && !self.config.auto_memory_summary
        {
            info!(
                run_id = run.id.as_str(),
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                memory_thread_id = task.memory_thread_id.as_str(),
                "task runner skipped automatic memory summary because TASK_RUNNER_AUTO_MEMORY_SUMMARY is disabled"
            );
        }

        self.store.clear_cancel_requested(&run.id);
    }
}
