// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[path = "completion/harness.rs"]
mod harness;

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
        report.content = report
            .content
            .map(|content| path_redactor.redact_text(content.as_str()));
        report.error = report
            .error
            .map(|error| path_redactor.redact_text(error.as_str()));
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
        let mut result_summary = summarized_report_content(&report.content);
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
        self.cleanup_task_terminals(task, run, effective_workspace_dir)
            .await;
        self.maybe_trigger_auto_memory_summary(task, run).await;
        self.spawn_chatos_async_followup_dispatch(task, run);
        self.store.clear_cancel_requested(&run.id);
    }

    fn spawn_chatos_async_followup_dispatch(&self, task: &TaskRecord, run: &TaskRunRecord) {
        if run.status != TaskRunStatus::Succeeded {
            return;
        }
        let service = self.clone();
        let task = task.clone();
        let run_id = run.id.clone();
        crate::auth::spawn_with_current_access_token(async move {
            service
                .dispatch_chatos_async_followup_tasks(task, run_id)
                .await;
        });
    }

    async fn dispatch_chatos_async_followup_tasks(&self, task: TaskRecord, run_id: String) {
        match self
            .dispatch_ready_chatos_async_tasks_for_source_task(&task)
            .await
        {
            Ok(runs) => {
                if !runs.is_empty() {
                    info!(
                        task_id = task.id.as_str(),
                        run_id = run_id.as_str(),
                        dispatched_count = runs.len(),
                        "task runner dispatched ready Chatos async follow-up tasks"
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run_id.as_str(),
                    error = err.as_str(),
                    "task runner failed to dispatch Chatos async follow-up tasks"
                );
            }
        }
    }

    async fn maybe_trigger_auto_memory_summary(&self, task: &TaskRecord, run: &mut TaskRunRecord) {
        if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && self.config.auto_memory_summary
        {
            if let Err(err) = self.trigger_memory_summary(task, run).await {
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
                memory_thread_id = run.memory_thread_id.as_str(),
                "task runner skipped automatic memory summary because TASK_RUNNER_AUTO_MEMORY_SUMMARY is disabled"
            );
        }
    }
}

#[cfg(test)]
fn terminal_blocked_tasks_summary(tasks: &[TaskRecord]) -> String {
    let mut reason_groups = Vec::<(String, Vec<String>)>::new();
    for task in tasks {
        let raw_reason = task
            .task_tool_state
            .closure_reason
            .as_deref()
            .or(task.task_tool_state.blocker_reason.as_deref())
            .unwrap_or("未提供阻塞原因");
        let reason = user_facing_terminal_block_reason(raw_reason);
        if let Some((_, titles)) = reason_groups
            .iter_mut()
            .find(|(existing, _)| existing == &reason)
        {
            titles.push(task.title.trim().to_string());
        } else {
            reason_groups.push((reason, vec![task.title.trim().to_string()]));
        }
    }

    if let [(reason, titles)] = reason_groups.as_slice() {
        return format!(
            "本次运行未完成：{} 个必需步骤未完成。共同原因：{}。未完成步骤：{}",
            tasks.len(),
            reason,
            summarized_blocked_titles(titles)
        );
    }

    let details = reason_groups
        .iter()
        .take(3)
        .map(|(reason, titles)| {
            format!(
                "{}（{} 个步骤：{}）",
                reason,
                titles.len(),
                summarized_blocked_titles(titles)
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    format!(
        "本次运行未完成：{} 个必需步骤未完成。阻塞明细：{}",
        tasks.len(),
        details
    )
}

#[cfg(test)]
fn user_facing_terminal_block_reason(reason: &str) -> String {
    if reason_looks_like_internal_tool_availability_blocker(reason) {
        return "当前运行没有可继续使用的仓库读取或终端能力，模型未完成必要实现或验证；这属于执行收口原因，不代表业务任务已完成或沙箱初始化失败"
            .to_string();
    }
    if reason.contains("连续两次") && reason.contains("校验没有状态进展") {
        return "模型结束前仍未完成必要的代码修改或验证，系统连续两次检查都没有看到实际进展"
            .to_string();
    }
    if reason.contains("达到") && reason.contains("最大收口轮次") {
        return "模型在允许的收尾轮次内仍未完成必要步骤".to_string();
    }
    reason.trim().to_string()
}

#[cfg(test)]
fn reason_looks_like_internal_tool_availability_blocker(reason: &str) -> bool {
    let normalized = reason.to_ascii_lowercase();
    let mentions_tool_surface = [
        "仓库读取",
        "读取能力",
        "文件读取",
        "源码读取",
        "未读取",
        "读取工具",
        "读文件工具",
        "代码读取",
        "真实文件",
        "schema",
        "测试执行能力",
        "执行能力",
        "验证能力",
        "运行测试",
        "终端",
        "执行工具",
        "工具清单",
        "read tool",
        "read-file",
        "read_file",
        "terminal",
        "tool surface",
        "tool list",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    let mentions_availability = [
        "未暴露",
        "未能",
        "没有",
        "缺少",
        "无法",
        "不能",
        "不可用",
        "临时限制",
        "临时不可用",
        "被限制",
        "not exposed",
        "unavailable",
        "temporarily unavailable",
        "disabled",
        "rate-limited",
        "rate limited",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    mentions_tool_surface && mentions_availability
}

#[cfg(test)]
fn summarized_blocked_titles(titles: &[String]) -> String {
    let shown = titles
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if titles.len() > 5 {
        format!("{} 等 {} 个", shown, titles.len())
    } else {
        shown
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::{CreateTaskRequest, TaskClosureState, TaskManagerScope};
    use crate::services::TaskService;
    use crate::store::AppStore;
    use chatos_ai_runtime::AiTurnStatus;
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://run-completion-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            local_connector_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_services() -> (TaskService, RunService) {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        let task_service = TaskService::new(config.clone(), store.clone());
        let run_service = RunService::new(config, store.clone(), AskUserPromptService::new(store));
        (task_service, run_service)
    }

    async fn create_task(service: &TaskService, title: &str, status: TaskStatus) -> TaskRecord {
        service
            .create_task(
                CreateTaskRequest {
                    title: title.to_string(),
                    description: None,
                    objective: format!("do {title}"),
                    input_payload: None,
                    status: Some(status),
                    priority: None,
                    tags: None,
                    default_model_config_id: None,
                    project_id: None,
                    task_profile: None,
                    tenant_id: None,
                    subject_id: None,
                    schedule: None,
                    plugin_config: Default::default(),
                    mcp_config: None,
                    prerequisite_task_ids: None,
                },
                None,
                None,
            )
            .await
            .expect("create task")
    }

    fn run_record(task: &TaskRecord) -> TaskRunRecord {
        let now = now_rfc3339();
        TaskRunRecord {
            id: "run-1".to_string(),
            task_id: task.id.clone(),
            model_config_id: "model-1".to_string(),
            memory_thread_id: task.memory_thread_id.clone(),
            status: TaskRunStatus::Running,
            started_at: Some(now.clone()),
            finished_at: None,
            input_snapshot: json!({}),
            plugin_snapshots: Vec::new(),
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            dispatch_paused: false,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            chatos_callback_delivery: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn completed_run_persists_success_when_report_completed() {
        let (task_service, run_service) = test_services().await;
        let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;

        let mut run = run_record(&parent);
        run_service
            .store
            .save_run(run.clone())
            .await
            .expect("save run");
        let report = TaskRunReport {
            task_id: parent.id.clone(),
            run_id: run.id.clone(),
            model_config_id: Some(run.model_config_id.clone()),
            status: AiTurnStatus::Completed,
            content: Some("done".to_string()),
            reasoning: None,
            error: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: None,
            completed_at: now_rfc3339(),
        };

        run_service
            .finalize_model_phase(&parent, &mut run, report, ".", None, None)
            .await;

        let saved_run = run_service
            .store
            .get_run(run.id.as_str())
            .await
            .expect("get run")
            .expect("run");
        assert_eq!(saved_run.status, TaskRunStatus::Succeeded);

        let saved_parent = task_service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(saved_parent.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn harness_promotion_failure_fails_completed_model_run() {
        let (task_service, run_service) = test_services().await;
        let task = create_task(&task_service, "harness failure", TaskStatus::Ready).await;
        let mut run = run_record(&task);
        run_service
            .store
            .save_run(run.clone())
            .await
            .expect("save run");
        let report = TaskRunReport {
            task_id: task.id.clone(),
            run_id: run.id.clone(),
            model_config_id: Some(run.model_config_id.clone()),
            status: AiTurnStatus::Completed,
            content: Some("model completed".to_string()),
            reasoning: None,
            error: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: None,
            completed_at: now_rfc3339(),
        };
        let harness_output = HarnessRunOutputReport {
            enabled: true,
            project_id: "project-1".to_string(),
            repo_path: "owner/repo".to_string(),
            git_url: "https://example.invalid/repo.git".to_string(),
            base_branch: "main".to_string(),
            run_branch: "chatos/runs/run-1".to_string(),
            base_commit: "base".to_string(),
            result_commit: None,
            status: "failed".to_string(),
            message: Some("concurrent base update".to_string()),
        };

        run_service
            .finalize_model_phase(&task, &mut run, report, ".", None, Some(harness_output))
            .await;

        let saved_run = run_service
            .store
            .get_run(run.id.as_str())
            .await
            .expect("get run")
            .expect("run");
        assert_eq!(saved_run.status, TaskRunStatus::Failed);
        assert!(saved_run
            .error_message
            .as_deref()
            .is_some_and(|error| error.contains("concurrent base update")));
        let saved_task = task_service
            .get_task(task.id.as_str())
            .await
            .expect("get task")
            .expect("task");
        assert_eq!(saved_task.status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn completed_report_ignores_legacy_terminal_checklist_blocker() {
        let (task_service, run_service) = test_services().await;
        let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;
        let mut run = run_record(&parent);
        run_service
            .store
            .save_run(run.clone())
            .await
            .expect("save run");

        let now = now_rfc3339();
        let mut child = parent.clone();
        child.id = "blocked-child".to_string();
        child.title = "blocked child".to_string();
        child.status = TaskStatus::Blocked;
        child.parent_task_id = Some(parent.id.clone());
        child.source_run_id = Some(run.id.clone());
        child.last_run_id = None;
        child.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
        child.task_tool_state.task_session_id = Some(run.id.clone());
        child.task_tool_state.required_for_parent_completion = Some(true);
        child.task_tool_state.closure_state = Some(TaskClosureState::BlockedTerminal);
        child.task_tool_state.closure_reason = Some("upstream API unavailable".to_string());
        child.task_tool_state.lifecycle_updated_at = Some(now.clone());
        child.created_at = now.clone();
        child.updated_at = now;
        run_service
            .store
            .save_task(child)
            .await
            .expect("save blocked child");

        let report = TaskRunReport {
            task_id: parent.id.clone(),
            run_id: run.id.clone(),
            model_config_id: Some(run.model_config_id.clone()),
            status: AiTurnStatus::Completed,
            content: Some("done".to_string()),
            reasoning: None,
            error: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: None,
            completed_at: now_rfc3339(),
        };

        run_service
            .finalize_model_phase(&parent, &mut run, report, ".", None, None)
            .await;

        let saved_run = run_service
            .store
            .get_run(run.id.as_str())
            .await
            .expect("get run")
            .expect("run");
        assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
        assert_eq!(saved_run.error_message, None);
        let saved_parent = task_service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(saved_parent.status, TaskStatus::Succeeded);
        let saved_child = task_service
            .get_task("blocked-child")
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(saved_child.status, TaskStatus::Blocked);
        assert_eq!(
            saved_child.task_tool_state.closure_state,
            Some(TaskClosureState::BlockedTerminal)
        );
    }

    #[tokio::test]
    async fn completed_report_ignores_legacy_required_open_checklist() {
        let (task_service, run_service) = test_services().await;
        let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;
        let mut run = run_record(&parent);
        run_service
            .store
            .save_run(run.clone())
            .await
            .expect("save run");

        let now = now_rfc3339();
        let mut child = parent.clone();
        child.id = "forgotten-child".to_string();
        child.title = "forgotten child".to_string();
        child.status = TaskStatus::Ready;
        child.parent_task_id = Some(parent.id.clone());
        child.source_run_id = Some(run.id.clone());
        child.last_run_id = None;
        child.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
        child.task_tool_state.task_session_id = Some(run.id.clone());
        child.task_tool_state.required_for_parent_completion = Some(true);
        child.task_tool_state.closure_state = Some(TaskClosureState::Open);
        child.task_tool_state.lifecycle_updated_at = Some(now.clone());
        child.created_at = now.clone();
        child.updated_at = now;
        run_service
            .store
            .save_task(child.clone())
            .await
            .expect("save forgotten child");

        let report = TaskRunReport {
            task_id: parent.id.clone(),
            run_id: run.id.clone(),
            model_config_id: Some(run.model_config_id.clone()),
            status: AiTurnStatus::Completed,
            content: Some("done".to_string()),
            reasoning: None,
            error: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: None,
            completed_at: now_rfc3339(),
        };

        run_service
            .finalize_model_phase(&parent, &mut run, report, ".", None, None)
            .await;

        let saved_child = task_service
            .get_task(child.id.as_str())
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(
            saved_child.task_tool_state.closure_state,
            Some(TaskClosureState::Open)
        );
        assert_eq!(saved_child.status, TaskStatus::Ready);
        let saved_run = run_service
            .store
            .get_run(run.id.as_str())
            .await
            .expect("get run")
            .expect("run");
        assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
        assert_eq!(saved_run.error_message, None);
        let saved_parent = task_service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(saved_parent.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn successful_run_preserves_legacy_optional_open_checklist() {
        let (task_service, run_service) = test_services().await;
        let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;
        let mut run = run_record(&parent);
        run_service
            .store
            .save_run(run.clone())
            .await
            .expect("save run");

        let now = now_rfc3339();
        let mut child = parent.clone();
        child.id = "optional-child".to_string();
        child.title = "optional child".to_string();
        child.status = TaskStatus::Ready;
        child.parent_task_id = Some(parent.id.clone());
        child.source_run_id = Some(run.id.clone());
        child.last_run_id = None;
        child.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
        child.task_tool_state.task_session_id = Some(run.id.clone());
        child.task_tool_state.required_for_parent_completion = Some(false);
        child.task_tool_state.closure_state = Some(TaskClosureState::Open);
        child.task_tool_state.lifecycle_updated_at = Some(now.clone());
        child.created_at = now.clone();
        child.updated_at = now;
        run_service
            .store
            .save_task(child.clone())
            .await
            .expect("save optional child");

        let report = TaskRunReport {
            task_id: parent.id.clone(),
            run_id: run.id.clone(),
            model_config_id: Some(run.model_config_id.clone()),
            status: AiTurnStatus::Completed,
            content: Some("done".to_string()),
            reasoning: None,
            error: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: None,
            completed_at: now_rfc3339(),
        };

        run_service
            .finalize_model_phase(&parent, &mut run, report, ".", None, None)
            .await;

        let saved_child = task_service
            .get_task(child.id.as_str())
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(
            saved_child.task_tool_state.closure_state,
            Some(TaskClosureState::Open)
        );
        assert_eq!(saved_child.status, TaskStatus::Ready);
        let saved_run = run_service
            .store
            .get_run(run.id.as_str())
            .await
            .expect("get run")
            .expect("run");
        assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
    }

    #[tokio::test]
    async fn aborted_report_does_not_downgrade_already_succeeded_task() {
        let (task_service, run_service) = test_services().await;
        let mut parent = create_task(&task_service, "parent", TaskStatus::Succeeded).await;
        parent.result_summary = Some("completed before abort".to_string());
        run_service
            .store
            .save_task(parent.clone())
            .await
            .expect("save succeeded parent");

        let mut run = run_record(&parent);
        run_service
            .store
            .save_run(run.clone())
            .await
            .expect("save run");
        let report = TaskRunReport {
            task_id: parent.id.clone(),
            run_id: run.id.clone(),
            model_config_id: Some(run.model_config_id.clone()),
            status: AiTurnStatus::Aborted,
            content: None,
            reasoning: None,
            error: Some("aborted".to_string()),
            tool_calls: None,
            finish_reason: None,
            usage: None,
            response_id: None,
            completed_at: now_rfc3339(),
        };

        run_service
            .finalize_model_phase(&parent, &mut run, report, ".", None, None)
            .await;

        let saved_run = run_service
            .store
            .get_run(run.id.as_str())
            .await
            .expect("get run")
            .expect("run");
        assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
        assert_eq!(
            saved_run.result_summary.as_deref(),
            Some("completed before abort")
        );
        assert_eq!(saved_run.error_message, None);

        let saved_parent = task_service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(saved_parent.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn terminal_blocked_summary_groups_repeated_internal_reason() {
        let (task_service, _) = test_services().await;
        let reason = "模型完成声明后，连续两次系统收口校验没有状态进展；仍有必需清单未完成，父任务不能标记为成功";
        let mut first = create_task(&task_service, "实现后端", TaskStatus::Blocked).await;
        first.task_tool_state.closure_reason = Some(reason.to_string());
        let mut second = create_task(&task_service, "执行验证", TaskStatus::Blocked).await;
        second.task_tool_state.closure_reason = Some(reason.to_string());

        let summary = terminal_blocked_tasks_summary(&[first, second]);

        assert!(summary.contains("本次运行未完成：2 个必需步骤未完成"));
        assert!(summary.contains("共同原因"));
        assert!(summary.contains("实现后端、执行验证"));
        assert!(!summary.contains("系统收口"));
        assert_eq!(summary.matches("连续两次检查").count(), 1);
    }

    #[tokio::test]
    async fn terminal_blocked_summary_rewrites_internal_tool_availability_reason() {
        let (task_service, _) = test_services().await;
        let mut task = create_task(&task_service, "执行验证", TaskStatus::Blocked).await;
        task.task_tool_state.closure_reason =
            Some("当前运行未暴露仓库读取工具与终端执行工具，无法继续。".to_string());

        let summary = terminal_blocked_tasks_summary(&[task]);

        assert!(summary.contains("本次运行未完成：1 个必需步骤未完成"));
        assert!(summary.contains("不代表业务任务已完成或沙箱初始化失败"));
        assert!(!summary.contains("未暴露仓库读取工具"));
        assert!(!summary.contains("系统拦截"));
    }
}
