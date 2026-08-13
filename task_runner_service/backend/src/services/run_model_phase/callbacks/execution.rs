// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(in crate::services) async fn prepare_single_model_step(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        prepared_execution: PreparedModelExecution,
    ) -> Result<PreparedSingleModelStep, String> {
        let max_iterations = prepared_execution
            .runtime_config
            .max_iterations
            .unwrap_or(DEFAULT_TASK_RUN_MAX_ITERATIONS);
        let runtime_settings = self.effective_task_runner_runtime_settings().await?;
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
        let runtime_config = prepared_execution.runtime_config;
        let runtime = runtime_config
            .build_runtime_with_mcp_builder_and_memory_http_client(
                prepared_execution.mcp_builder,
                self.config.memory_engine_http_client.clone(),
            )
            .await?;
        self.persist_mcp_runtime_snapshot(task, run, &runtime_config, &runtime)
            .await;
        let mut run_spec = prepared_execution.run_spec;
        if task.mcp_config.requires_execution
            && run_spec.model_config.previous_response_id.is_none()
        {
            let policy = self.effective_node_supply_chain_policy().await?;
            run_spec
                .prefixed_input_items
                .push(super::supply_chain::policy_guidance(&policy));
        }
        append_external_mcp_runtime_notice(&mut run_spec, task, &runtime);
        Ok(PreparedSingleModelStep {
            agent: prepared_execution.agent,
            run_spec,
            runtime,
            runtime_options: runtime_execution.runtime_options,
            mcp_runtime_session_ref: prepared_execution
                .mcp_management_runtime_session
                .session_id()
                .to_string(),
            mcp_command_queue: prepared_execution.mcp_command_queue,
            lifecycle_state: runtime_execution.lifecycle_state,
            progress: runtime_execution.progress,
            pending_stream_event: runtime_execution.pending_stream_event,
            plugin_sessions: prepared_execution.plugin_sessions,
            supply_chain_evidence: runtime_execution.supply_chain_evidence,
        })
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

pub(in crate::services) fn apply_supply_chain_outcome_gate(
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
    use chatos_ai_runtime::AiTurnReport;

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
            dependency_baseline_verified: status == "passed",
            dependency_baseline_violations: Vec::new(),
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
