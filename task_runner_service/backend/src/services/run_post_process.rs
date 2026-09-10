// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::{info, warn};

use crate::models::{
    now_rfc3339, TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskStatus,
    WorkspaceIntegrationStatus,
};

use super::RunService;

fn ensure_model_phase_terminal_for_post_process(run: &TaskRunRecord) -> Result<(), String> {
    if matches!(
        run.model_phase_status,
        crate::models::ModelPhaseStatus::Pending | crate::models::ModelPhaseStatus::Running
    ) {
        return Err(super::RUN_POST_PROCESS_MODEL_PHASE_PENDING_ERROR.to_string());
    }
    Ok(())
}

impl RunService {
    pub async fn waive_run_workspace_integration(
        &self,
        run_id: &str,
        reason: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err("放弃代码必须填写原因".to_string());
        }
        if reason.chars().count() > 2_000 {
            return Err("放弃代码原因不能超过 2000 个字符".to_string());
        }
        let Some(existing) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        let frozen_mcp_config = existing
            .input_snapshot
            .get("mcp_config")
            .cloned()
            .ok_or_else(|| "运行缺少冻结的 MCP 配置，不能放弃代码变更".to_string())
            .and_then(|value| {
                serde_json::from_value::<crate::models::TaskMcpConfig>(value)
                    .map_err(|error| format!("运行的冻结 MCP 配置无效：{error}"))
            })?;
        if frozen_mcp_config.workspace_changes_required {
            return Err(
                "该任务的代码变更属于必需结果，不能放弃；请重新集成或重新执行任务".to_string(),
            );
        }
        let mut task = self
            .store
            .get_task(existing.task_id.as_str())
            .await?
            .ok_or_else(|| {
                format!(
                    "Task not found for integration waiver: {}",
                    existing.task_id
                )
            })?;
        let Some(run) = self
            .store
            .waive_run_workspace_integration(run_id, reason)
            .await?
        else {
            return Ok(None);
        };
        task.status = TaskStatus::Succeeded;
        task.result_summary = run.result_summary.clone();
        task.last_run_id = Some(run.id.clone());
        task.updated_at = now_rfc3339();
        self.store.save_task(task.clone()).await?;
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "integration_waived",
                Some(format!("已放弃该可选任务的代码变更：{reason}")),
                Some(serde_json::json!({
                    "reason": reason,
                    "execution_group_id": run.workspace_execution.as_ref()
                        .and_then(|execution| execution.execution_group_id.clone()),
                    "result_commit": run.workspace_execution.as_ref()
                        .and_then(|execution| execution.result_commit.clone()),
                })),
            ))
            .await?;
        self.try_send_terminal_callback(task.id.as_str(), &run)
            .await;
        if let Err(error) = self.enqueue_run_post_process_if_needed(&run).await {
            warn!(
                run_id = run.id.as_str(),
                error = error.as_str(),
                "failed to publish waived Run post-process event; Outbox reconciliation will retry"
            );
        }
        Ok(Some(self.store.get_run(run_id).await?.unwrap_or(run)))
    }

    pub async fn retry_run_workspace_integration(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let Some(mut run) = self.store.rearm_run_workspace_integration(run_id).await? else {
            return Ok(None);
        };
        if let Some(mut task) = self.store.get_task(run.task_id.as_str()).await? {
            if task.status != TaskStatus::Cancelled {
                task.status = TaskStatus::Running;
                task.updated_at = now_rfc3339();
                self.store.save_task(task).await?;
            }
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "integration_retry_requested",
                Some("已请求重新集成任务代码".to_string()),
                None,
            ))
            .await?;
        let _ = self.enqueue_run_post_process_if_needed(&run).await?;
        run = self
            .store
            .get_run(run_id)
            .await?
            .ok_or_else(|| format!("Run disappeared after integration retry: {run_id}"))?;
        Ok(Some(run))
    }

    pub async fn replay_run_post_process_dead_letter(
        &self,
        run_id: &str,
    ) -> Result<(TaskRunRecord, bool), String> {
        if !self
            .store
            .rearm_run_post_process_dead_letter(run_id)
            .await?
        {
            return Err(format!(
                "Run {run_id} is not an eligible dead-lettered post-process"
            ));
        }
        let run =
            self.store.get_run(run_id).await?.ok_or_else(|| {
                format!("Run not found after post-process replay rearm: {run_id}")
            })?;
        self.enqueue_run_post_process_if_needed(&run).await?;
        let replayed =
            self.store.get_run(run_id).await?.ok_or_else(|| {
                format!("Run not found after post-process replay publish: {run_id}")
            })?;
        let archived = match crate::run_post_process_queue::archive_run_post_process_dead_letter(
            &self.task_queue_topology,
            run_id,
            1_000,
        )
        .await
        {
            Ok(archived) => archived,
            Err(error) => {
                warn!(
                    run_id,
                    error = error.as_str(),
                    "Run post-process replay succeeded but old DLQ message archival failed"
                );
                false
            }
        };
        Ok((replayed, archived))
    }

    pub(crate) async fn enqueue_run_post_process_if_needed(
        &self,
        run: &TaskRunRecord,
    ) -> Result<bool, String> {
        if !run.requires_post_process()
            || !run.post_process_event_pending
            || run.post_process_completed
        {
            return Ok(false);
        }
        crate::run_post_process_queue::enqueue_run_post_process(
            &self.task_queue_topology,
            run.id.as_str(),
        )
        .await?;
        self.store
            .acknowledge_run_post_process_event(run.id.as_str())
            .await?;
        Ok(true)
    }

    pub(crate) async fn publish_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self.store.list_pending_run_post_processes(limit).await?;
        let mut published = 0usize;
        for run in pending {
            if self.enqueue_run_post_process_if_needed(&run).await? {
                published += 1;
            }
        }
        Ok(published)
    }

    pub(crate) async fn record_run_post_process_failure(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<u32, String> {
        self.store
            .record_run_post_process_failure(run_id, error)
            .await?;
        self.store
            .get_run(run_id)
            .await?
            .map(|run| run.post_process_attempt_count)
            .ok_or_else(|| format!("Run not found after post-process failure: {run_id}"))
    }

    pub(crate) async fn mark_run_post_process_dead_lettered(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<(), String> {
        let updated = self
            .store
            .mark_run_post_process_dead_lettered(run_id, error)
            .await?;
        if !updated {
            return Ok(());
        }
        if let Err(event_error) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                "post_process_dead_lettered",
                Some(format!("Run 后处理达到最大重试次数并进入死信队列: {error}")),
                None,
            ))
            .await
        {
            warn!(
                run_id,
                error = event_error.as_str(),
                "failed to append Run post-process dead-letter event"
            );
        }
        Ok(())
    }

    pub(crate) async fn process_run_post_process(&self, run_id: &str) -> Result<(), String> {
        let Some(mut run) = self.store.get_run(run_id).await? else {
            return Ok(());
        };
        if run.post_process_completed || run.post_process_dead_lettered {
            return Ok(());
        }
        ensure_model_phase_terminal_for_post_process(&run)?;
        let task = self
            .store
            .get_task(run.task_id.as_str())
            .await?
            .ok_or_else(|| format!("Run post-process task not found: {}", run.task_id))?;
        let close_response = self.finalize_mcp_management_run(&task, &run).await?;
        crate::services::workspace_execution::apply_runtime_provider_finalization(
            self,
            &mut run,
            close_response
                .as_ref()
                .and_then(|response| response.provider_finalization.as_ref()),
        )
        .await?;
        crate::services::workspace_execution::finalize_task_run_workspace(self, &task, &mut run)
            .await?;

        if run.status == TaskRunStatus::Running {
            match run
                .workspace_execution
                .as_ref()
                .map(|execution| execution.integration_status)
            {
                Some(WorkspaceIntegrationStatus::Integrated) => {
                    self.finish_run_after_integration(&task, &mut run, TaskRunStatus::Succeeded)
                        .await?;
                }
                Some(WorkspaceIntegrationStatus::Waived) => {}
                Some(WorkspaceIntegrationStatus::Conflict) => {
                    self.finish_run_after_integration(&task, &mut run, TaskRunStatus::Blocked)
                        .await?;
                    self.store.mark_run_post_process_completed(run_id).await?;
                    return Ok(());
                }
                Some(
                    WorkspaceIntegrationStatus::Pending
                    | WorkspaceIntegrationStatus::Integrating
                    | WorkspaceIntegrationStatus::Failed,
                ) => {
                    return Err(
                        "Task Run workspace integration has not reached a terminal result"
                            .to_string(),
                    );
                }
                Some(WorkspaceIntegrationStatus::NotRequired) | None => {}
            }
        }

        if run.status == TaskRunStatus::Blocked {
            self.ensure_verification_repair_chain(&task, &run).await?;
        }

        if run.status != TaskRunStatus::Succeeded {
            self.store.mark_run_post_process_completed(run_id).await?;
            return Ok(());
        }

        self.promote_execution_group_if_complete(&task, &mut run)
            .await?;

        if !run.memory_summary_processed {
            self.store
                .mark_run_memory_summary_processed(
                    run.id.as_str(),
                    run.summary_job_run_id.as_deref(),
                )
                .await?;
        }

        if !run.chatos_followup_processed {
            let dispatched = self
                .dispatch_ready_chatos_async_tasks_for_source_task(&task)
                .await?;
            if !dispatched.is_empty() {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    dispatched_count = dispatched.len(),
                    "task runner post-processor dispatched ready Chatos follow-up tasks"
                );
            }
            self.store
                .mark_run_chatos_followup_processed(run.id.as_str())
                .await?;
        }

        self.store
            .mark_run_post_process_completed(run.id.as_str())
            .await?;
        Ok(())
    }

    async fn finish_run_after_integration(
        &self,
        task: &crate::models::TaskRecord,
        run: &mut TaskRunRecord,
        terminal_status: TaskRunStatus,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        run.status = terminal_status;
        run.finished_at = Some(now.clone());
        run.updated_at = now.clone();
        if terminal_status == TaskRunStatus::Blocked {
            let conflict_message = run
                .workspace_execution
                .as_ref()
                .and_then(|execution| execution.conflict_message.clone())
                .unwrap_or_else(|| "代码集成发生冲突".to_string());
            run.error_message = Some(conflict_message);
        } else {
            run.error_message = None;
        }
        *run = self.store.save_run(run.clone()).await?;
        let mut task = self
            .store
            .get_task(task.id.as_str())
            .await?
            .ok_or_else(|| format!("Task not found after workspace integration: {}", task.id))?;
        if task.status != TaskStatus::Cancelled {
            task.status = match terminal_status {
                TaskRunStatus::Succeeded => TaskStatus::Succeeded,
                TaskRunStatus::Blocked => TaskStatus::Blocked,
                _ => TaskStatus::Running,
            };
            task.result_summary = run.result_summary.clone();
            task.last_run_id = Some(run.id.clone());
            task.updated_at = now;
            self.store.save_task(task.clone()).await?;
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        let (event_type, message) = if terminal_status == TaskRunStatus::Succeeded {
            (
                "integration_succeeded",
                "任务代码已成功集成到执行批次分支".to_string(),
            )
        } else {
            (
                "integration_conflict",
                run.error_message
                    .clone()
                    .unwrap_or_else(|| "任务代码集成冲突".to_string()),
            )
        };
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                event_type,
                Some(message),
                run.workspace_execution
                    .as_ref()
                    .and_then(|execution| serde_json::to_value(execution).ok()),
            ))
            .await?;
        Ok(())
    }

    async fn promote_execution_group_if_complete(
        &self,
        _task: &crate::models::TaskRecord,
        _run: &mut TaskRunRecord,
    ) -> Result<(), String> {
        // Local Connector owns Git finalization and integration. Task Runner
        // records the returned immutable result but never promotes branches itself.
        Ok(())
    }
    pub(in crate::services) async fn enqueue_terminal_side_effects(&self, run: &TaskRunRecord) {
        if let Err(err) = self.enqueue_run_post_process_if_needed(run).await {
            warn!(
                run_id = run.id.as_str(),
                error = err.as_str(),
                "failed to enqueue Run post-processing; Outbox reconciliation will retry"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::{CreateTaskRequest, TaskRunWorkspaceExecution, WorkspacePreparationStatus};
    use crate::services::TaskService;
    use crate::store::AppStore;
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    #[test]
    fn post_process_rejects_non_terminal_model_phase_before_resource_finalization() {
        let mut pending = run("pending-model", "2026-08-15T10:00:00Z", None);
        pending.model_phase_status = crate::models::ModelPhaseStatus::Pending;
        assert!(ensure_model_phase_terminal_for_post_process(&pending).is_err());

        pending.model_phase_status = crate::models::ModelPhaseStatus::Running;
        assert!(ensure_model_phase_terminal_for_post_process(&pending).is_err());

        pending.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
        assert!(ensure_model_phase_terminal_for_post_process(&pending).is_ok());
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://run-post-process-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            memory_engine_http_client: reqwest::Client::new(),
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1_000),
            execution_timeout: Duration::from_millis(1_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            worker_id: "test-worker".to_string(),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 2_000,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
        }
    }

    async fn waiver_services() -> (TaskService, RunService, AppStore) {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        let task_service = TaskService::new(config.clone(), store.clone());
        let run_service = RunService::new(
            config,
            store.clone(),
            AskUserPromptService::new(store.clone()),
        );
        (task_service, run_service, store)
    }

    async fn create_waiver_task(
        task_service: &TaskService,
        store: &AppStore,
        workspace_changes_required: bool,
    ) -> crate::models::TaskRecord {
        let mut task = task_service
            .create_task(
                CreateTaskRequest {
                    title: "integration waiver task".to_string(),
                    description: None,
                    objective: "test integration waiver".to_string(),
                    input_payload: None,
                    status: Some(TaskStatus::Blocked),
                    priority: None,
                    tags: None,
                    default_model_config_id: None,
                    project_id: None,
                    project_context: None,
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
            .expect("create task");
        task.mcp_config.workspace_changes_required = workspace_changes_required;
        store.save_task(task).await.expect("save waiver policy")
    }

    async fn save_conflicted_run(
        store: &AppStore,
        task_id: &str,
        workspace_changes_required: bool,
    ) -> TaskRunRecord {
        let mut run = run(
            "waiver-run",
            "2026-08-15T10:00:00Z",
            Some(WorkspaceIntegrationStatus::Conflict),
        );
        run.task_id = task_id.to_string();
        run.status = TaskRunStatus::Blocked;
        run.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
        run.finished_at = Some("2026-08-15T10:01:00Z".to_string());
        run.result_summary = Some("model output retained".to_string());
        run.input_snapshot = json!({
            "mcp_config": {
                "workspace_changes_required": workspace_changes_required,
            },
        });
        let execution = run.workspace_execution.as_mut().expect("workspace");
        execution.result_commit = Some("result-commit-1".to_string());
        execution.conflict_files = vec!["src/main.rs".to_string()];
        store.save_run(run).await.expect("save conflicted run")
    }

    fn run(
        id: &str,
        created_at: &str,
        integration_status: Option<WorkspaceIntegrationStatus>,
    ) -> TaskRunRecord {
        let mut run = TaskRunRecord::queued(
            id.to_string(),
            format!("task-{id}"),
            "model-1".to_string(),
            "thread-1".to_string(),
            json!({}),
            created_at.to_string(),
        );
        run.status = TaskRunStatus::Succeeded;
        run.workspace_execution =
            integration_status.map(|integration_status| TaskRunWorkspaceExecution {
                status: WorkspacePreparationStatus::Ready,
                route: None,
                branch_target: None,
                execution_group_id: Some("group-1".to_string()),
                execution_branch_ref: Some("chatos/executions/group-1".to_string()),
                execution_base_commit: None,
                integration_status,
                integration_ready_at: None,
                integration_started_at: None,
                integrated_at: None,
                integration_attempt_count: 0,
                integration_base_commit: None,
                result_commit: None,
                integrated_commit: None,
                promoted_commit: None,
                waived_at: None,
                waiver_reason: None,
                local_changed_files: Vec::new(),
                local_patch: None,
                local_patch_truncated: false,
                conflict_files: Vec::new(),
                conflict_message: None,
                integration_last_error: None,
                prepared_at: None,
                finalized_at: None,
                lease_retained_for_diagnostics: false,
                finalization_error: None,
                error: None,
            });
        run
    }

    #[tokio::test]
    async fn required_workspace_changes_cannot_be_waived() {
        let (task_service, run_service, store) = waiver_services().await;
        let task = create_waiver_task(&task_service, &store, false).await;
        save_conflicted_run(&store, task.id.as_str(), true).await;

        let error = run_service
            .waive_run_workspace_integration("waiver-run", "skip it")
            .await
            .expect_err("required changes must fail closed");
        assert!(error.contains("不能放弃"));

        let unchanged = store
            .get_run("waiver-run")
            .await
            .expect("load run")
            .expect("run");
        assert_eq!(unchanged.status, TaskRunStatus::Blocked);
        assert_eq!(
            unchanged
                .workspace_execution
                .expect("workspace")
                .integration_status,
            WorkspaceIntegrationStatus::Conflict
        );
    }

    #[tokio::test]
    async fn optional_workspace_changes_can_be_waived_and_continue_post_process() {
        let (task_service, run_service, store) = waiver_services().await;
        let task = create_waiver_task(&task_service, &store, true).await;
        save_conflicted_run(&store, task.id.as_str(), false).await;

        let waived = run_service
            .waive_run_workspace_integration("waiver-run", "optional output is not required")
            .await
            .expect("waive optional integration")
            .expect("waived run");
        let integration = waived.workspace_execution.as_ref().expect("workspace");
        assert_eq!(waived.status, TaskRunStatus::Succeeded);
        assert_eq!(
            integration.integration_status,
            WorkspaceIntegrationStatus::Waived
        );
        assert_eq!(
            integration.result_commit.as_deref(),
            Some("result-commit-1")
        );
        assert_eq!(
            integration.waiver_reason.as_deref(),
            Some("optional output is not required")
        );
        assert!(integration.waived_at.is_some());
        assert!(waived.post_process_event_enqueued || waived.post_process_event_pending);

        let saved_task = task_service
            .get_task(task.id.as_str())
            .await
            .expect("load task")
            .expect("task");
        assert_eq!(saved_task.status, TaskStatus::Succeeded);
        assert_eq!(
            saved_task.result_summary.as_deref(),
            Some("model output retained")
        );

        let events = store
            .list_run_events("waiver-run")
            .await
            .expect("list events");
        assert!(events
            .iter()
            .any(|event| event.event_type == "integration_waived"));
    }
}
