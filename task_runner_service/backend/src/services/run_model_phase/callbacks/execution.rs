// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::time::Instant;

impl RunService {
    pub(in crate::services) async fn execute_prepared_model_run(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        prepared_execution: PreparedModelExecution,
    ) -> TaskRunReport {
        let mcp_management_runtime_session =
            prepared_execution.mcp_management_runtime_session.clone();
        let max_iterations = prepared_execution
            .runtime_config
            .max_iterations
            .unwrap_or(DEFAULT_TASK_RUN_MAX_ITERATIONS);
        let runtime_settings = match self.effective_task_runner_runtime_settings().await {
            Ok(settings) => settings,
            Err(err) => {
                let report = TaskRunReport::from_ai_report(
                    task.id.clone(),
                    run.id.clone(),
                    Some(model_config.id.clone()),
                    AiTurnReport::failed(format!(
                        "failed to resolve Task Runner runtime settings: {err}"
                    )),
                );
                close_mcp_management_runtime_session(mcp_management_runtime_session, task, run)
                    .await;
                return report;
            }
        };
        let review_policy = TaskExecutionReviewPolicy::new(
            runtime_settings.review_read_only_iterations,
            runtime_settings.review_missing_read_failures,
            runtime_settings.review_repeat_interval_iterations,
        );
        let runtime_execution = self.build_runtime_execution_state(
            task.id.as_str(),
            run,
            model_config,
            &prepared_execution.run_spec,
            prepared_execution.tool_result_model_budget_limits,
            max_iterations,
            review_policy,
            prepared_execution.effective_workspace_dir.as_str(),
        );
        let path_redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace(
            self.config.default_workspace_dir.as_str(),
            prepared_execution.effective_workspace_dir.as_str(),
        );
        let execution_timeout = match self.effective_execution_timeout().await {
            Ok(timeout) => timeout,
            Err(err) => {
                let report = TaskRunReport::from_ai_report(
                    task.id.clone(),
                    run.id.clone(),
                    Some(model_config.id.clone()),
                    AiTurnReport::failed(format!("failed to resolve execution timeout: {err}")),
                );
                close_mcp_management_runtime_session(mcp_management_runtime_session, task, run)
                    .await;
                return report;
            }
        };
        let mut run_spec = prepared_execution.run_spec;
        let agent = prepared_execution.agent;
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
                        AiTurnReport::failed(
                            path_redactor
                                .redact_text(format!("runtime init failed: {err}").as_str()),
                        ),
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
            agent
                .run_report_with_runtime_options(
                    runtime_config.clone(),
                    run_spec.clone(),
                    &runtime,
                    runtime_options.clone(),
                )
                .await
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
            Some(&path_redactor),
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
                .unwrap_or_else(|| "任务已完成。".to_string());
            report.status = chatos_ai_runtime::AiTurnStatus::Completed;
            report.content = Some(path_redactor.redact_text(content.as_str()));
            report.error = None;
        }
        close_mcp_management_runtime_session(mcp_management_runtime_session, task, run).await;
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
            .filter(|&(_name, info)| is_user_configured_external_tool(info))
            .map(|(name, _info)| name.clone())
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

async fn close_mcp_management_runtime_session(
    runtime_session: McpManagementRuntimeSessionHandle,
    task: &TaskRecord,
    run: &TaskRunRecord,
) {
    let mcp_session_id = runtime_session.session_id().to_string();
    if let Err(error) = runtime_session.close().await {
        warn!(
            task_id = task.id.as_str(),
            run_id = run.id.as_str(),
            mcp_session_id,
            error = %error,
            "close Task Runner MCP Management runtime session failed"
        );
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
        .filter(|&(_name, info)| is_user_configured_external_tool(info))
        .map(|(name, _info)| name.clone())
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
        && info.server_name
            != chatos_mcp::system_mcp_descriptor(
                chatos_plugin_management_sdk::SystemMcpKey::ProjectManagement,
            )
            .server_name
        && info.server_name != crate::services::sandbox_runtime::SANDBOX_MCP_SERVER_NAME
}
