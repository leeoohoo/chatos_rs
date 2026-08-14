// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[cfg(test)]
#[path = "completion/tests.rs"]
mod tests;

impl RunService {
    pub(in crate::services) async fn finalize_model_phase(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        report: TaskRunReport,
        effective_workspace_dir: &str,
    ) {
        let path_redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace(
            self.config.default_workspace_dir.as_str(),
            effective_workspace_dir,
        );
        let mut report = report;
        run.bind_current_attempt_model_response(report.response_id.as_deref());
        fail_report_when_outcome_references_invalid(&mut report);
        report.content = report
            .content
            .map(|content| path_redactor.redact_text(content.as_str()));
        report.error = report
            .error
            .map(|error| path_redactor.redact_text(error.as_str()));
        let terminal_status = resolve_report_terminal_status(&mut report);
        let report_json = serde_json::to_value(&report).ok().map(|mut value| {
            path_redactor.redact_value(&mut value);
            value
        });
        let existing_task = self.store.get_task(&task.id).await.ok().flatten();
        let task_already_succeeded = existing_task
            .as_ref()
            .is_some_and(|task| task.status == TaskStatus::Succeeded);
        let outcome_summary = report
            .execution_outcome
            .as_ref()
            .map(|outcome| {
                structured_outcome_result_summary(
                    outcome,
                    task.mcp_config.locale().is_english(),
                    &path_redactor,
                )
            })
            .filter(|summary| !summary.is_empty());
        let mut result_summary = match terminal_status {
            TaskRunStatus::Succeeded | TaskRunStatus::Blocked => outcome_summary
                .clone()
                .or_else(|| summarized_report_content(&report.content)),
            TaskRunStatus::Failed => report
                .error
                .clone()
                .or(outcome_summary)
                .or_else(|| summarized_report_content(&report.content)),
            TaskRunStatus::Cancelled => report.error.clone(),
            TaskRunStatus::Queued | TaskRunStatus::Running => None,
        };
        run.updated_at = now_rfc3339();
        run.finished_at = Some(report.completed_at.clone());
        run.result_summary = result_summary.clone();
        run.error_message = match terminal_status {
            TaskRunStatus::Blocked | TaskRunStatus::Failed => report
                .execution_outcome
                .as_ref()
                .and_then(|outcome| outcome.blocking_reason.as_deref())
                .map(|reason| path_redactor.redact_text(reason.trim()))
                .filter(|reason| !reason.is_empty())
                .or_else(|| report.error.clone())
                .or_else(|| result_summary.clone()),
            _ => report.error.clone(),
        };
        run.usage = report.usage.clone();
        run.report = report_json.clone();
        run.cancel_requested = false;
        run.status = terminal_status;
        if task_already_succeeded && run.status != TaskRunStatus::Succeeded {
            run.status = TaskRunStatus::Succeeded;
            run.error_message = None;
            result_summary = existing_task
                .as_ref()
                .and_then(|task| task.result_summary.clone())
                .or_else(|| Some("任务已完成。".to_string()));
            run.result_summary = result_summary.clone();
        }
        match self.store.save_run(run.clone()).await {
            Ok(saved) => {
                *run = saved;
            }
            Err(err) => {
                warn!("failed to persist completed task run {}: {}", run.id, err);
                return;
            }
        }
        self.notify_mcp_management_run_finalized(task, run).await;

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
                Some(match run.status {
                    TaskRunStatus::Blocked => format!(
                        "任务执行受阻：{}",
                        run.error_message.as_deref().unwrap_or("存在终态阻塞子任务")
                    ),
                    TaskRunStatus::Failed => format!(
                        "任务执行失败：{}",
                        run.error_message.as_deref().unwrap_or("未知错误")
                    ),
                    TaskRunStatus::Cancelled => "任务已取消。".to_string(),
                    _ => report.user_message(),
                }),
                report_json.clone(),
            ))
            .await
        {
            warn!(
                "failed to append completion event for run {}: {}",
                run.id, err
            );
        }
        let mut task_already_cancelled = false;
        if let Some(mut task_record) = existing_task {
            task_already_cancelled = task_record.status == TaskStatus::Cancelled;
            if !task_already_cancelled {
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
        }
        if !task_already_cancelled {
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        self.enqueue_terminal_side_effects(run).await;
        self.store.clear_cancel_requested(&run.id);
    }
}

impl RunService {
    pub(in crate::services) async fn notify_mcp_management_run_finalized(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) {
        if let Err(error) = self.finalize_mcp_management_run(task, run).await {
            warn!(
                task_id = task.id.as_str(),
                run_id = run.id.as_str(),
                error = %error,
                "notify MCP Management that run finalized failed; durable post-process will retry"
            );
        }
    }

    pub(in crate::services) async fn finalize_mcp_management_run(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) -> Result<(), String> {
        use chatos_mcp_management_sdk::{McpManagementClient, McpManagementClientConfig};

        let _ = task;
        if !matches!(
            run.status,
            TaskRunStatus::Succeeded
                | TaskRunStatus::Failed
                | TaskRunStatus::Cancelled
                | TaskRunStatus::Blocked
        ) {
            return Ok(());
        }
        let Some(session_id) = run
            .mcp_runtime_session_ref
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let config = match McpManagementClientConfig::from_env("task-runner").await {
            Ok(config) => config,
            Err(error) => {
                return Err(format!(
                    "{}: load client config: {error}",
                    crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX
                ));
            }
        };
        let client = match McpManagementClient::new(config) {
            Ok(client) => client,
            Err(error) => {
                return Err(format!(
                    "{}: initialize client: {error}",
                    crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX
                ));
            }
        };
        match client.close_runtime_session(session_id).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                if message.contains("404")
                    || message.contains("not found")
                    || message.contains("already closed")
                {
                    Ok(())
                } else {
                    Err(format!(
                        "{}: {message}",
                        crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX
                    ))
                }
            }
        }
    }
}

fn fail_report_when_outcome_references_invalid(report: &mut TaskRunReport) {
    if report.status != chatos_ai_runtime::AiTurnStatus::Completed {
        return;
    }
    let Some(outcome) = report.execution_outcome.as_mut() else {
        return;
    };
    if outcome.status != chatos_ai_runtime::TaskExecutionOutcomeStatus::Succeeded {
        return;
    }
    if let Err(err) = validate_task_execution_outcome_references(outcome) {
        report.status = chatos_ai_runtime::AiTurnStatus::Failed;
        report.error = Some(format!(
            "structured task execution outcome reference validation failed: {err}"
        ));
    }
}

fn validate_task_execution_outcome_references(
    outcome: &mut chatos_ai_runtime::TaskExecutionOutcome,
) -> Result<(), String> {
    for path in &mut outcome.referenced_paths {
        *path = validate_workspace_reference(path)?;
    }
    for endpoint in &outcome.referenced_endpoints {
        validate_endpoint_reference(endpoint)?;
    }
    Ok(())
}

fn validate_workspace_reference(reference: &str) -> Result<String, String> {
    let reference = reference.trim();
    let relative_path = std::path::Path::new(reference);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "referenced path must stay inside the workspace: {reference}"
        ));
    }
    Ok(reference.to_string())
}

fn validate_endpoint_reference(reference: &str) -> Result<(), String> {
    let reference = reference.trim();
    let endpoint = reqwest::Url::parse(reference)
        .map_err(|err| format!("referenced endpoint is not a valid URL: {reference}: {err}"))?;
    if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
        return Err(format!(
            "referenced endpoint must be an absolute HTTP/HTTPS URL: {reference}"
        ));
    }
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        return Err(format!(
            "referenced endpoint must not contain credentials: {reference}"
        ));
    }
    Ok(())
}

fn structured_outcome_result_summary(
    outcome: &chatos_ai_runtime::TaskExecutionOutcome,
    english: bool,
    path_redactor: &crate::services::path_redaction::WorkspacePathRedactor,
) -> String {
    let mut sections = vec![path_redactor.redact_text(outcome.summary.trim())];
    if outcome.status == chatos_ai_runtime::TaskExecutionOutcomeStatus::Succeeded {
        if !outcome.referenced_paths.is_empty() {
            sections.push(format!(
                "{}：{}",
                if english {
                    "Verified paths"
                } else {
                    "已验证路径"
                },
                outcome
                    .referenced_paths
                    .join(if english { ", " } else { "、" })
            ));
        }
        if !outcome.referenced_endpoints.is_empty() {
            sections.push(format!(
                "{}：{}",
                if english {
                    "Verified endpoints"
                } else {
                    "已验证端点"
                },
                outcome
                    .referenced_endpoints
                    .join(if english { ", " } else { "、" })
            ));
        }
    }
    let supply_chain_evidence = outcome
        .verification_evidence
        .iter()
        .filter(|evidence| evidence.starts_with("Node.js supply-chain audit status:"))
        .map(|evidence| path_redactor.redact_text(evidence))
        .collect::<Vec<_>>();
    if !supply_chain_evidence.is_empty() {
        sections.push(format!(
            "{}：{}",
            if english {
                "Supply-chain audit"
            } else {
                "供应链审计"
            },
            supply_chain_evidence.join(if english { "; " } else { "；" })
        ));
    }
    sections
        .into_iter()
        .filter(|section| !section.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve_report_terminal_status(report: &mut TaskRunReport) -> TaskRunStatus {
    match report.status {
        chatos_ai_runtime::AiTurnStatus::Failed => TaskRunStatus::Failed,
        chatos_ai_runtime::AiTurnStatus::Aborted => TaskRunStatus::Cancelled,
        chatos_ai_runtime::AiTurnStatus::Completed => {
            let Some(outcome) = report.execution_outcome.as_ref() else {
                report.status = chatos_ai_runtime::AiTurnStatus::Failed;
                report.error = Some(
                    "task runtime completed without a structured execution outcome".to_string(),
                );
                return TaskRunStatus::Failed;
            };
            if let Err(err) = outcome.validate() {
                report.status = chatos_ai_runtime::AiTurnStatus::Failed;
                report.error = Some(format!("invalid structured task execution outcome: {err}"));
                return TaskRunStatus::Failed;
            }
            match outcome.status {
                chatos_ai_runtime::TaskExecutionOutcomeStatus::Succeeded => {
                    TaskRunStatus::Succeeded
                }
                chatos_ai_runtime::TaskExecutionOutcomeStatus::Blocked => TaskRunStatus::Blocked,
                chatos_ai_runtime::TaskExecutionOutcomeStatus::Failed => TaskRunStatus::Failed,
                chatos_ai_runtime::TaskExecutionOutcomeStatus::Cancelled => {
                    TaskRunStatus::Cancelled
                }
            }
        }
    }
}
