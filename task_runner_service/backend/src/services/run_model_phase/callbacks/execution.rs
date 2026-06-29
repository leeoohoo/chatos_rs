use super::*;
use std::time::Instant;

const MAX_COMPLETION_GATE_FOLLOWUPS: usize = 3;

impl RunService {
    pub(in crate::services) async fn execute_prepared_model_run(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        prepared_execution: PreparedModelExecution,
    ) -> TaskRunReport {
        let runtime_execution = self.build_runtime_execution_state(
            task.id.as_str(),
            run,
            model_config,
            &prepared_execution.run_spec,
            prepared_execution.tool_result_model_budget_limits,
        );
        let execution_timeout = match self.effective_execution_timeout().await {
            Ok(timeout) => timeout,
            Err(err) => {
                return TaskRunReport::from_ai_report(
                    task.id.clone(),
                    run.id.clone(),
                    Some(model_config.id.clone()),
                    AiTurnReport::failed(format!("failed to resolve execution timeout: {err}")),
                );
            }
        };
        let mut run_spec = prepared_execution.run_spec;
        let runtime_config = prepared_execution.runtime_config;
        let mcp_builder = prepared_execution.mcp_builder;
        let runtime_options = runtime_execution.runtime_options;
        let mut report = match tokio::time::timeout(execution_timeout, async {
            let runtime_init_started_at = Instant::now();
            let runtime = match runtime_config
                .build_runtime_with_mcp_builder(mcp_builder)
                .await
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    warn!(
                        run_id = run.id.as_str(),
                        task_id = task.id.as_str(),
                        model_config_id = model_config.id.as_str(),
                        runtime_init_ms = runtime_init_started_at.elapsed().as_millis(),
                        error = err.as_str(),
                        "task runner runtime init failed"
                    );
                    return TaskRunReport::from_ai_report(
                        task.id.clone(),
                        run.id.clone(),
                        Some(model_config.id.clone()),
                        AiTurnReport::failed(format!("runtime init failed: {err}")),
                    );
                }
            };
            info!(
                run_id = run.id.as_str(),
                task_id = task.id.as_str(),
                model_config_id = model_config.id.as_str(),
                runtime_init_ms = runtime_init_started_at.elapsed().as_millis(),
                "task runner runtime initialized"
            );
            self.persist_mcp_runtime_snapshot(task, run, &runtime_config, &runtime)
                .await;
            append_external_mcp_runtime_notice(&mut run_spec, task, &runtime);
            let mut completion_gate_attempts = 0usize;
            loop {
                let execution = TaskRunExecution::new(runtime_config.clone(), run_spec.clone());
                let report = execution
                    .run_report_with_runtime_options(&runtime, runtime_options.clone())
                    .await;
                if !report.is_completed() {
                    return report;
                }
                let task_for_validation = self
                    .store
                    .get_task(&task.id)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| task.clone());
                let unfinished =
                    match unfinished_subtasks_for_task(&self.store, &task_for_validation).await {
                        Ok(unfinished) => unfinished,
                        Err(err) => {
                            return TaskRunReport::from_ai_report(
                                task.id.clone(),
                                run.id.clone(),
                                Some(model_config.id.clone()),
                                AiTurnReport::failed(format!(
                                    "failed to validate subtasks before completion: {err}"
                                )),
                            );
                        }
                    };
                if unfinished.is_empty() {
                    return report;
                }
                completion_gate_attempts += 1;
                let message = unfinished_subtasks_error(&task_for_validation, &unfinished);
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "completion_gate",
                        Some(format!(
                            "父任务暂不能完成，本轮继续处理未完成子任务：{message}"
                        )),
                        Some(json!({
                            "task_id": task.id,
                            "unfinished_subtask_ids": unfinished
                                .iter()
                                .map(|subtask| subtask.id.clone())
                                .collect::<Vec<_>>(),
                            "attempt": completion_gate_attempts,
                        })),
                    ))
                    .await
                {
                    warn!(
                        "failed to append completion gate event for run {}: {}",
                        run.id, err
                    );
                }
                if completion_gate_attempts >= MAX_COMPLETION_GATE_FOLLOWUPS {
                    return TaskRunReport::from_ai_report(
                        task.id.clone(),
                        run.id.clone(),
                        Some(model_config.id.clone()),
                        AiTurnReport::failed(format!(
                            "父任务暂不能完成，已连续 {} 次要求继续处理子任务但仍未完成：{message}",
                            completion_gate_attempts
                        )),
                    );
                }
                Self::append_completion_gate_feedback(
                    &mut run_spec,
                    message.as_str(),
                    completion_gate_attempts,
                );
            }
        })
        .await
        {
            Ok(report) => report,
            Err(_) => TaskRunReport::from_ai_report(
                task.id.clone(),
                run.id.clone(),
                Some(model_config.id.clone()),
                AiTurnReport::failed(format!(
                    "execution timed out after {} seconds",
                    execution_timeout.as_secs()
                )),
            ),
        };
        runtime_execution
            .stop_cancel_poll
            .store(true, Ordering::Relaxed);
        runtime_execution.cancel_poll_handle.abort();
        flush_pending_stream_event(
            &self.store,
            run.id.as_str(),
            &runtime_execution.pending_stream_event,
        );
        if report.is_aborted()
            && (runtime_execution
                .task_completed_abort
                .load(Ordering::Relaxed)
                || self.task_is_already_succeeded(task.id.as_str()).await)
        {
            let content = self
                .store
                .get_task(&task.id)
                .await
                .ok()
                .flatten()
                .and_then(|task| task.result_summary)
                .unwrap_or_else(|| "任务已通过 TaskManager 标记为成功。".to_string());
            report.status = chatos_ai_runtime::AiTurnStatus::Completed;
            report.content = Some(content);
            report.error = None;
        }
        report
    }

    async fn task_is_already_succeeded(&self, task_id: &str) -> bool {
        self.store
            .get_task(task_id)
            .await
            .ok()
            .flatten()
            .is_some_and(|task| task.status == TaskStatus::Succeeded)
    }

    fn completion_gate_feedback_item(message: &str, attempt: usize) -> Value {
        json!({
            "role": "system",
            "content": format!(
                "[Task Runner completion gate]\n{message}\n这是第 {attempt} 次完成前校验反馈。不要结束当前任务；请继续调用内置 TaskManager 工具查看/更新/完成这些子任务。只有所有子任务都成功后，父任务才能给出最终完成答复。"
            )
        })
    }

    fn append_completion_gate_feedback(run_spec: &mut TaskRunSpec, message: &str, attempt: usize) {
        run_spec
            .current_input_items
            .push(Self::completion_gate_feedback_item(message, attempt));
        run_spec.user_record = None;
    }

    async fn persist_mcp_runtime_snapshot(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        runtime_config: &TaskRuntimeConfig,
        runtime: &TaskRuntime,
    ) {
        if !task.mcp_config.enabled {
            return;
        }
        let Some(executor) = runtime.mcp_executor() else {
            return;
        };
        let tool_names = executor
            .available_tools()
            .into_iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        let external_tool_names = executor
            .tool_metadata()
            .iter()
            .filter_map(|(name, info)| is_user_configured_external_tool(info).then(|| name.clone()))
            .collect::<Vec<_>>();
        let unavailable_tools = executor.unavailable_tools();
        let payload = json!({
            "task_id": task.id,
            "run_id": run.id,
            "mcp_enabled": runtime_config.mcp_init_mode != chatos_ai_runtime::TaskMcpInitMode::Disabled,
            "external_mcp_config_ids": task.mcp_config.external_mcp_config_ids,
            "available_tool_count": tool_names.len(),
            "available_tools": tool_names,
            "external_tool_count": external_tool_names.len(),
            "external_tools": external_tool_names,
            "unavailable_tool_count": unavailable_tools.len(),
            "unavailable_tools": unavailable_tools,
        });
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "mcp_runtime",
                Some("MCP runtime initialized".to_string()),
                Some(payload),
            ))
            .await
        {
            warn!(
                run_id = run.id.as_str(),
                task_id = task.id.as_str(),
                "failed to persist MCP runtime snapshot: {err}"
            );
        }
    }
}

fn append_external_mcp_runtime_notice(
    run_spec: &mut TaskRunSpec,
    task: &TaskRecord,
    runtime: &TaskRuntime,
) {
    if task.mcp_config.external_mcp_config_ids.is_empty() {
        return;
    }
    let Some(executor) = runtime.mcp_executor() else {
        return;
    };
    let external_tool_names = executor
        .tool_metadata()
        .iter()
        .filter_map(|(name, info)| is_user_configured_external_tool(info).then(|| name.clone()))
        .collect::<Vec<_>>();
    if !external_tool_names.is_empty() {
        return;
    }
    let unavailable_tools = executor.unavailable_tools();
    if unavailable_tools.is_empty() {
        return;
    }

    let unavailable_summary = unavailable_tools
        .iter()
        .filter_map(|item| {
            let server_name = item.get("server_name").and_then(Value::as_str)?;
            let server_type = item
                .get("server_type")
                .and_then(Value::as_str)
                .unwrap_or("-");
            let reason = item.get("reason").and_then(Value::as_str).unwrap_or("-");
            Some(format!("- {server_name} ({server_type}): {reason}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    if unavailable_summary.is_empty() {
        return;
    }

    let text = if task.mcp_config.locale().is_english() {
        format!(
            "[External MCP unavailable]\nThis task is bound to external MCP configs, but no external MCP tools were registered for this run. Do not claim that the external system was searched. Report this as a runtime MCP availability problem.\n\nUnavailable MCP servers:\n{unavailable_summary}"
        )
    } else {
        format!(
            "[外部 MCP 不可用]\n当前任务绑定了外部 MCP 配置，但本次运行没有注册到任何外部 MCP 工具。不要声称已经检索过外部系统；请把它作为运行时 MCP 可用性问题说明。\n\n不可用 MCP 服务：\n{unavailable_summary}"
        )
    };
    run_spec.prefixed_input_items.push(json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    }));
}

fn is_user_configured_external_tool(info: &chatos_mcp_runtime::ToolInfo) -> bool {
    matches!(info.server_type.as_str(), "http" | "stdio")
        && info.server_name != PROJECT_MANAGEMENT_MCP_SERVER_NAME
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_ai_runtime::ModelRuntimeConfig;

    #[test]
    fn completion_gate_feedback_keeps_same_run_context() {
        let model_config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let mut run_spec = TaskRunSpec::new("task-1", "run-1", model_config, "do the task")
            .with_model_config_id("model-1");

        RunService::append_completion_gate_feedback(
            &mut run_spec,
            "父任务还有未完成子任务 1 个：child(ready)。",
            2,
        );

        assert_eq!(run_spec.task_id, "task-1");
        assert_eq!(run_spec.run_id, "run-1");
        assert!(run_spec.user_record.is_none());
        let feedback = run_spec
            .current_input_items
            .last()
            .expect("completion gate feedback");
        assert_eq!(feedback.get("role").and_then(Value::as_str), Some("system"));
        let content = feedback
            .get("content")
            .and_then(Value::as_str)
            .expect("feedback content");
        assert!(content.contains("第 2 次完成前校验反馈"));
        assert!(content.contains("不要结束当前任务"));
        assert!(content.contains("继续调用内置 TaskManager 工具"));
    }
}
