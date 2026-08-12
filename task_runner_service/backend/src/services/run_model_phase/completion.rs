// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[path = "completion/harness.rs"]
mod harness;
#[cfg(test)]
#[path = "completion/tests.rs"]
mod tests;

impl RunService {
    pub(super) async fn finalize_model_phase(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        report: TaskRunReport,
        effective_workspace_dir: &str,
        sandbox_output: Option<SandboxOutputReport>,
        harness_output: Option<HarnessRunOutputReport>,
    ) {
        let path_redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace(
            self.config.default_workspace_dir.as_str(),
            effective_workspace_dir,
        );
        let mut report = report;
        harness::fail_report_when_promotion_failed(&mut report, harness_output.as_ref());
        run.bind_current_attempt_model_response(report.response_id.as_deref());
        let reference_workspace_dir = sandbox_output
            .as_ref()
            .and_then(|output| output.output_workspace.as_deref())
            .unwrap_or(effective_workspace_dir);
        fail_report_when_outcome_references_invalid(
            &mut report,
            reference_workspace_dir,
            task.mcp_config.requires_execution,
        );
        report.content = report
            .content
            .map(|content| path_redactor.redact_text(content.as_str()));
        report.error = report
            .error
            .map(|error| path_redactor.redact_text(error.as_str()));
        let terminal_status = resolve_report_terminal_status(&mut report);
        let report_json =
            report_json_with_outputs(&report, sandbox_output.as_ref(), harness_output.as_ref())
                .map(|mut value| {
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
        self.request_task_terminal_cleanup(task, run);
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
        use chatos_mcp_management_sdk::{
            FinalizeRuntimeRunRequest, McpManagementClient, McpManagementClientConfig,
            RuntimeRunTerminalStatus,
        };

        let status = match run.status {
            TaskRunStatus::Succeeded => RuntimeRunTerminalStatus::Succeeded,
            TaskRunStatus::Failed => RuntimeRunTerminalStatus::Failed,
            TaskRunStatus::Cancelled => RuntimeRunTerminalStatus::Cancelled,
            TaskRunStatus::Blocked => RuntimeRunTerminalStatus::Blocked,
            TaskRunStatus::Queued | TaskRunStatus::Running => return Ok(()),
        };
        let owner_user_id = task
            .owner_user_id
            .as_deref()
            .or(task.creator_user_id.as_deref())
            .unwrap_or(task.subject_id.as_str())
            .trim();
        if owner_user_id.is_empty() {
            return Err(format!(
                "{}: owner identity is missing",
                crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX
            ));
        }
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
        client
            .finalize_runtime_run(&FinalizeRuntimeRunRequest {
                owner_user_id: owner_user_id.to_string(),
                project_id: crate::models::normalize_project_id(Some(task.project_id.clone())),
                run_id: run.id.clone(),
                status,
            })
            .await
            .map(|_| ())
            .map_err(|error| {
                format!(
                    "{}: {error}",
                    crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX
                )
            })
    }
}

fn fail_report_when_outcome_references_invalid(
    report: &mut TaskRunReport,
    workspace_dir: &str,
    requires_execution: bool,
) {
    if report.status != chatos_ai_runtime::AiTurnStatus::Completed {
        return;
    }
    let Some(outcome) = report.execution_outcome.as_mut() else {
        return;
    };
    if outcome.status != chatos_ai_runtime::TaskExecutionOutcomeStatus::Succeeded {
        return;
    }
    if let Err(err) =
        validate_task_execution_outcome_references(outcome, workspace_dir, requires_execution)
    {
        report.status = chatos_ai_runtime::AiTurnStatus::Failed;
        report.error = Some(format!(
            "structured task execution outcome reference validation failed: {err}"
        ));
    }
}

fn validate_task_execution_outcome_references(
    outcome: &mut chatos_ai_runtime::TaskExecutionOutcome,
    workspace_dir: &str,
    require_existing_paths: bool,
) -> Result<(), String> {
    for path in &mut outcome.referenced_paths {
        *path = validate_workspace_reference(workspace_dir, path, require_existing_paths)?;
    }
    for endpoint in &outcome.referenced_endpoints {
        validate_endpoint_reference(endpoint)?;
    }
    Ok(())
}

fn validate_workspace_reference(
    workspace_dir: &str,
    reference: &str,
    require_exists: bool,
) -> Result<String, String> {
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
    if !require_exists {
        return Ok(reference.to_string());
    }

    let workspace_root = std::fs::canonicalize(workspace_dir)
        .map_err(|err| format!("failed to resolve workspace root {workspace_dir}: {err}"))?;
    let resolved = match std::fs::canonicalize(workspace_root.join(relative_path)) {
        Ok(resolved) => resolved,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            resolve_unique_single_edit_workspace_reference(&workspace_root, relative_path).map_err(
                |correction_error| {
                    format!(
                        "referenced path does not exist: {reference}: {err}; {correction_error}"
                    )
                },
            )?
        }
        Err(err) => {
            return Err(format!(
                "referenced path does not exist: {reference}: {err}"
            ));
        }
    };
    if !resolved.starts_with(&workspace_root) {
        return Err(format!(
            "referenced path resolves outside the workspace: {reference}"
        ));
    }
    resolved
        .strip_prefix(&workspace_root)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|_| format!("referenced path resolves outside the workspace: {reference}"))
}

fn resolve_unique_single_edit_workspace_reference(
    workspace_root: &std::path::Path,
    relative_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let mut current = workspace_root.to_path_buf();
    let mut correction_used = false;

    for component in relative_path.components() {
        let std::path::Component::Normal(requested_name) = component else {
            continue;
        };
        let requested_name = requested_name
            .to_str()
            .ok_or_else(|| "reference contains a non-UTF-8 path component".to_string())?;
        let exact = current.join(requested_name);
        let next = if exact.exists() {
            exact
        } else {
            if correction_used {
                return Err("reference requires more than one one-character correction".to_string());
            }
            let mut candidates = std::fs::read_dir(&current)
                .map_err(|err| format!("failed to inspect referenced parent directory: {err}"))?
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let candidate_name = entry.file_name().to_str()?.to_string();
                    is_single_edit_away(requested_name, candidate_name.as_str())
                        .then(|| entry.path())
                });
            let candidate = candidates
                .next()
                .ok_or_else(|| "no unique one-character correction exists".to_string())?;
            if candidates.next().is_some() {
                return Err("more than one one-character correction exists".to_string());
            }
            correction_used = true;
            candidate
        };
        current = std::fs::canonicalize(next)
            .map_err(|err| format!("failed to resolve corrected workspace reference: {err}"))?;
        if !current.starts_with(workspace_root) {
            return Err("corrected reference resolves outside the workspace".to_string());
        }
    }

    if !correction_used {
        return Err("no one-character correction was applied".to_string());
    }
    Ok(current)
}

fn is_single_edit_away(left: &str, right: &str) -> bool {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left == right || left.len().abs_diff(right.len()) > 1 {
        return false;
    }

    let (mut left_index, mut right_index, mut edits) = (0, 0, 0);
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            left_index += 1;
            right_index += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        match left.len().cmp(&right.len()) {
            std::cmp::Ordering::Less => right_index += 1,
            std::cmp::Ordering::Greater => left_index += 1,
            std::cmp::Ordering::Equal => {
                left_index += 1;
                right_index += 1;
            }
        }
    }
    edits + usize::from(left_index < left.len() || right_index < right.len()) == 1
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

fn report_json_with_outputs(
    report: &TaskRunReport,
    sandbox_output: Option<&SandboxOutputReport>,
    harness_output: Option<&HarnessRunOutputReport>,
) -> Option<Value> {
    let mut report_json = serde_json::to_value(report).ok()?;
    if sandbox_output.is_none() && harness_output.is_none() {
        return Some(report_json);
    }
    if let Some(object) = report_json.as_object_mut() {
        let mut output = serde_json::Map::new();
        if let Some(sandbox) = sandbox_output {
            output.insert("sandbox".to_string(), serde_json::to_value(sandbox).ok()?);
        }
        if let Some(harness) = harness_output {
            output.insert("harness".to_string(), serde_json::to_value(harness).ok()?);
        }
        object.insert("output".to_string(), Value::Object(output));
    }
    Some(report_json)
}
