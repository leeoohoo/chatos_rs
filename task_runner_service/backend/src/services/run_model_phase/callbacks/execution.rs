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
        let supply_chain_policy = match self.effective_node_supply_chain_policy().await {
            Ok(policy) => policy,
            Err(err) => {
                let report = TaskRunReport::from_ai_report(
                    task.id.clone(),
                    run.id.clone(),
                    Some(model_config.id.clone()),
                    AiTurnReport::failed(format!("failed to resolve supply-chain policy: {err}")),
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
            run,
            model_config,
            &prepared_execution.run_spec,
            prepared_execution.tool_result_model_budget_limits,
            max_iterations,
            review_policy,
            task.mcp_config.requires_execution,
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
        if task.mcp_config.requires_execution {
            run_spec.prefixed_input_items.push(
                crate::services::run_model_phase::supply_chain::policy_guidance(
                    &supply_chain_policy,
                ),
            );
        }
        let agent = prepared_execution.agent;
        let runtime_config = prepared_execution.runtime_config;
        let mcp_builder = prepared_execution.mcp_builder;
        let runtime_options = runtime_execution.runtime_options;
        let mut report = match tokio::time::timeout(execution_timeout, async {
            let runtime_init_started_at = Instant::now();
            let runtime = match runtime_config
                .build_runtime_with_mcp_builder_and_memory_http_client(
                    mcp_builder,
                    self.config.memory_engine_http_client.clone(),
                )
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
        if report.is_completed() {
            report.execution_outcome = runtime_execution.execution_outcome.lock().clone();
        }
        if task.mcp_config.requires_execution {
            let supply_chain_report = runtime_execution
                .supply_chain_evidence
                .lock()
                .evaluate(&supply_chain_policy);
            if supply_chain_report.applicable {
                apply_supply_chain_outcome_gate(&mut report, &supply_chain_report);
                self.store.append_run_event_sync(TaskRunEventRecord::new(
                    run.id.clone(),
                    "supply_chain_audit",
                    Some(match supply_chain_report.status {
                        "passed" => "Node.js 供应链审计通过".to_string(),
                        _ => "Node.js 供应链审计未通过".to_string(),
                    }),
                    Some(supply_chain_report.event_payload()),
                ));
            }
        }
        self.unregister_runtime_abort_token(run.id.as_str());
        flush_pending_stream_event(
            &self.store,
            run.id.as_str(),
            &runtime_execution.pending_stream_event,
            Some(&path_redactor),
        );
        if report.is_aborted() && self.task_is_already_succeeded(task.id.as_str()).await {
            let content = self
                .store
                .get_task(&task.id)
                .await
                .ok()
                .flatten()
                .and_then(|task| task.result_summary)
                .unwrap_or_else(|| "任务已完成。".to_string());
            report.status = chatos_ai_runtime::AiTurnStatus::Completed;
            report.execution_outcome = Some(chatos_ai_runtime::TaskExecutionOutcome::succeeded(
                content.clone(),
                vec!["task status was already persisted as succeeded".to_string()],
            ));
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

fn apply_supply_chain_outcome_gate(
    report: &mut TaskRunReport,
    supply_chain: &crate::services::run_model_phase::supply_chain::SupplyChainAuditReport,
) {
    let evidence = supply_chain.evidence_summary();
    let Some(outcome) = report.execution_outcome.as_mut() else {
        return;
    };
    if !outcome
        .verification_evidence
        .iter()
        .any(|item| item == &evidence)
    {
        outcome.verification_evidence.push(evidence);
    }
    if supply_chain.status == "passed"
        || outcome.status != chatos_ai_runtime::TaskExecutionOutcomeStatus::Succeeded
    {
        return;
    }
    outcome.status = chatos_ai_runtime::TaskExecutionOutcomeStatus::Blocked;
    outcome.blocking_reason = Some(supply_chain.blocking_reasons.join("; "));
    outcome.unmet_acceptance_criteria = supply_chain.blocking_reasons.clone();
}

#[cfg(test)]
mod supply_chain_gate_tests {
    use super::*;
    use crate::services::run_model_phase::supply_chain::{
        NodeVulnerabilityCounts, SupplyChainAuditReport,
    };

    fn report(status: &'static str, blocking_reasons: Vec<String>) -> SupplyChainAuditReport {
        SupplyChainAuditReport {
            applicable: true,
            status,
            baseline_revision: "baseline-2026-08".to_string(),
            audit_level: "high".to_string(),
            package_manager: Some("npm".to_string()),
            lockfile_observed: true,
            install_command: Some("npm ci --ignore-scripts".to_string()),
            install_exit_code: Some(0),
            approved_install_script_packages: vec!["esbuild".to_string()],
            audit_command: Some("npm audit --audit-level=high --json".to_string()),
            audit_exit_code: Some(if status == "passed" { 0 } else { 1 }),
            vulnerabilities: Some(NodeVulnerabilityCounts {
                total: if status == "passed" { 0 } else { 1 },
                critical: if status == "passed" { 0 } else { 1 },
                ..NodeVulnerabilityCounts::default()
            }),
            blocking_reasons,
        }
    }

    fn task_report() -> TaskRunReport {
        let mut report = TaskRunReport::from_ai_report(
            "task-1",
            "run-1",
            Some("model-1".to_string()),
            AiTurnReport::completed(chatos_ai_runtime::AiRuntimeResult {
                content: "implemented".to_string(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("stop".to_string()),
                usage: None,
                response_id: None,
            }),
        );
        report.execution_outcome = Some(chatos_ai_runtime::TaskExecutionOutcome::succeeded(
            "implemented",
            vec!["tests passed".to_string()],
        ));
        report
    }

    #[test]
    fn blocked_supply_chain_report_overrides_claimed_success() {
        let mut task_report = task_report();
        apply_supply_chain_outcome_gate(
            &mut task_report,
            &report(
                "blocked",
                vec![
                    "Node.js dependency audit found 0 high and 1 critical vulnerabilities"
                        .to_string(),
                ],
            ),
        );

        let outcome = task_report.execution_outcome.unwrap();
        assert_eq!(
            outcome.status,
            chatos_ai_runtime::TaskExecutionOutcomeStatus::Blocked
        );
        assert!(outcome
            .blocking_reason
            .as_deref()
            .unwrap()
            .contains("critical"));
        assert!(outcome
            .verification_evidence
            .iter()
            .any(|evidence| evidence.contains("audit status: blocked")));
    }

    #[test]
    fn passed_supply_chain_report_adds_receipt_evidence() {
        let mut task_report = task_report();
        apply_supply_chain_outcome_gate(&mut task_report, &report("passed", Vec::new()));

        let outcome = task_report.execution_outcome.unwrap();
        assert_eq!(
            outcome.status,
            chatos_ai_runtime::TaskExecutionOutcomeStatus::Succeeded
        );
        assert!(outcome
            .verification_evidence
            .iter()
            .any(|evidence| evidence.contains("high=0, critical=0")));
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
            "[External MCP unavailable]\nThe Agent binding resolved external MCP resources for this run, but no corresponding tools were registered. Do not claim that the external system was searched. Report this as a runtime MCP availability problem.\n\nUnavailable MCP servers:\n{unavailable_summary}"
        )
    } else {
        format!(
            "[外部 MCP 不可用]\nAgent Binding 为本次运行解析出了外部 MCP 资源，但没有注册到对应工具。不要声称已经检索过外部系统；请把它作为运行时 MCP 可用性问题说明。\n\n不可用 MCP 服务：\n{unavailable_summary}"
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
