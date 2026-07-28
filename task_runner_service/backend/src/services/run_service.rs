// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

use crate::services::task_manager_lifecycle::{
    append_task_session_finalized_event, finalize_task_session_entries,
};

const MIN_WORKER_CLAIM_EXPIRY_GRACE: Duration = Duration::from_secs(120);
const WORKER_CLAIM_EXPIRED_ERROR: &str = "worker claim expired";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RejectedRunClaimHeartbeatAction {
    Continue,
    Stop,
    Abort,
}

pub(crate) fn worker_claim_expiry_grace(claim_ttl: Duration) -> Duration {
    claim_ttl
        .checked_mul(2)
        .unwrap_or(Duration::MAX)
        .max(MIN_WORKER_CLAIM_EXPIRY_GRACE)
}

impl RunService {
    #[cfg(test)]
    pub(crate) fn new(
        config: AppConfig,
        store: AppStore,
        ask_user_prompt_service: AskUserPromptService,
    ) -> Self {
        Self {
            config,
            store,
            plugin_management_client: None,
            ask_user_prompt_service,
            start_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            callback_delivery_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn new_with_plugin_management(
        config: AppConfig,
        store: AppStore,
        ask_user_prompt_service: AskUserPromptService,
        plugin_management_client: PluginManagementClient,
    ) -> Self {
        Self {
            config,
            store,
            plugin_management_client: Some(plugin_management_client),
            ask_user_prompt_service,
            start_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            callback_delivery_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    pub(super) async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        let snapshot = load_managed_config_snapshot().await;
        Ok(snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .usize(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY)
                    .or_else(|| snapshot.usize(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY))
            })
            .unwrap_or(self.config.default_task_execution_max_iterations)
            .max(2))
    }

    pub(super) async fn effective_execution_timeout(&self) -> Result<Duration, String> {
        Ok(Duration::from_millis(
            load_managed_config_snapshot()
                .await
                .and_then(|snapshot| snapshot.u64(TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY))
                .unwrap_or(self.config.execution_timeout.as_millis() as u64)
                .max(1),
        ))
    }

    pub(super) async fn effective_tool_result_model_budget_limits(
        &self,
    ) -> Result<ToolResultModelBudgetLimits, String> {
        let snapshot = load_managed_config_snapshot().await;
        Ok(ToolResultModelBudgetLimits::new(
            snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.usize(TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY))
                .unwrap_or(self.config.default_tool_result_model_max_chars),
            snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot.usize(TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY)
                })
                .unwrap_or(self.config.default_tool_results_model_total_max_chars),
        ))
    }

    pub(super) async fn effective_execution_environment_mode(&self) -> Result<String, String> {
        let snapshot = load_managed_config_snapshot().await;
        Ok(normalize_execution_environment_mode(
            snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot.string(TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY)
                })
                .as_deref()
                .or(Some(
                    self.config.default_execution_environment_mode.as_str(),
                )),
        ))
    }

    pub(super) async fn effective_sandbox_enabled(&self) -> Result<bool, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.bool("task_runner.sandbox.enabled"))
            .unwrap_or(false))
    }

    pub(super) async fn effective_sandbox_manager_base_url(&self) -> Result<String, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.string("task_runner.sandbox.manager_base_url"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.config.default_sandbox_manager_base_url.clone())
            .trim_end_matches('/')
            .to_string())
    }

    pub(super) async fn effective_sandbox_lease_ttl_seconds(&self) -> Result<u64, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.u64("task_runner.sandbox.lease_ttl_seconds"))
            .unwrap_or(self.config.default_sandbox_lease_ttl_seconds)
            .max(1))
    }

    pub async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        self.store.list_runs(task_id).await
    }

    pub async fn list_runs_filtered(
        &self,
        filters: RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let filters = sanitize_run_list_filters(filters);
        self.store.list_runs_filtered(&filters).await
    }

    pub async fn list_runs_page(
        &self,
        filters: RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let mut filters = sanitize_run_list_filters(filters);
        filters.limit = Some(filters.limit.unwrap_or(20));
        filters.offset = Some(filters.offset.unwrap_or(0));
        self.store.list_runs_page(&filters).await
    }

    pub async fn run_index(
        &self,
        filters: RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let filters = sanitize_run_list_filters(filters);
        self.store.list_run_summaries_filtered(&filters).await
    }

    pub async fn get_run_summaries_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let ids = sanitize_id_list(ids);
        self.store.get_run_summaries_by_ids(&ids).await
    }

    pub async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        self.store.get_run(id).await
    }

    pub async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        self.store.has_active_run_for_task(task_id).await
    }

    pub async fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_ttl: Duration,
    ) -> Result<Option<TaskRunRecord>, String> {
        let claim_token = Uuid::new_v4().to_string();
        let claim_until = (chrono::Utc::now()
            + chrono::Duration::from_std(claim_ttl).map_err(|err| err.to_string())?)
        .to_rfc3339();
        self.store
            .claim_next_queued_run(worker_id, claim_token.as_str(), claim_until.as_str())
            .await
    }

    pub async fn renew_run_claim(
        &self,
        run: &TaskRunRecord,
        worker_id: &str,
        claim_ttl: Duration,
    ) -> Result<bool, String> {
        let Some(claim_token) = run.claim_token.as_deref() else {
            return Ok(false);
        };
        let claim_until = (chrono::Utc::now()
            + chrono::Duration::from_std(claim_ttl).map_err(|err| err.to_string())?)
        .to_rfc3339();
        self.store
            .renew_run_claim(&run.id, worker_id, claim_token, claim_until.as_str())
            .await
    }

    pub async fn fail_expired_run_claims(&self, claim_ttl: Duration) -> Result<usize, String> {
        let now = now_rfc3339();
        let expiry_cutoff = (chrono::Utc::now()
            - chrono::Duration::from_std(worker_claim_expiry_grace(claim_ttl))
                .map_err(|err| err.to_string())?)
        .to_rfc3339();
        let failed_runs = self
            .store
            .fail_expired_run_claims(expiry_cutoff.as_str(), now.as_str())
            .await?;
        for run in &failed_runs {
            self.store.signal_local_run_abort(run.id.as_str());
            if let Err(err) = self
                .ask_user_prompt_service
                .cancel_pending_prompts_for_run(run.id.as_str(), WORKER_CLAIM_EXPIRED_ERROR)
                .await
            {
                tracing::warn!(
                    run_id = run.id.as_str(),
                    error = err.as_str(),
                    "failed to cancel pending ask user prompts after worker claim expired"
                );
            }
            if let Some(mut task) = self.store.get_task(run.task_id.as_str()).await? {
                if task.last_run_id.as_deref() == Some(run.id.as_str()) {
                    task.status = TaskStatus::Failed;
                    task.result_summary = run.result_summary.clone();
                    task.updated_at = now.clone();
                    self.store.save_task(task).await?;
                }
            }
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    "run.claim.expired".to_string(),
                    run.result_summary.clone(),
                    Some(serde_json::json!({
                        "reason": "worker_claim_expired",
                        "previous_worker_id": run.worker_id,
                    })),
                ))
                .await?;
            match finalize_task_session_entries(
                &self.store,
                run.task_id.as_str(),
                run.id.as_str(),
                run.status,
            )
            .await
            {
                Ok(summary) => {
                    append_task_session_finalized_event(&self.store, run, &summary).await
                }
                Err(err) => tracing::warn!(
                    run_id = run.id.as_str(),
                    error = err.as_str(),
                    "failed to finalize Task Manager session after worker claim expiry"
                ),
            }
            if let Err(err) = self.release_sandboxes_for_terminal_run(run).await {
                tracing::warn!(
                    run_id = run.id.as_str(),
                    error = err.as_str(),
                    "failed to release sandboxes after worker claim expired"
                );
            }
            self.try_send_terminal_callback(run.task_id.as_str(), run)
                .await;
        }
        self.store.refresh_runtime_guards().await?;
        Ok(failed_runs.len())
    }

    pub(crate) fn signal_local_run_abort(&self, run_id: &str) {
        self.store.signal_local_run_abort(run_id);
    }

    pub(crate) fn clear_local_run_abort(&self, run_id: &str) {
        self.store.clear_local_run_abort(run_id);
    }

    pub(crate) async fn run_claim_is_current(&self, run: &TaskRunRecord) -> bool {
        let Some(current) = self.store.get_run(run.id.as_str()).await.ok().flatten() else {
            return false;
        };
        current.status == TaskRunStatus::Running
            && current.worker_id.as_deref() == run.worker_id.as_deref()
            && current.claim_token.as_deref() == run.claim_token.as_deref()
    }

    pub(crate) async fn handle_rejected_run_claim_heartbeat(
        &self,
        claimed_run: &TaskRunRecord,
        worker_id: &str,
    ) -> Result<RejectedRunClaimHeartbeatAction, String> {
        let current = self.store.get_run(claimed_run.id.as_str()).await?;
        let action = match current.as_ref() {
            None => RejectedRunClaimHeartbeatAction::Abort,
            Some(current) if current.status == TaskRunStatus::Running => {
                if current.worker_id.as_deref() == Some(worker_id)
                    && current.claim_token.as_deref() == claimed_run.claim_token.as_deref()
                {
                    RejectedRunClaimHeartbeatAction::Continue
                } else {
                    RejectedRunClaimHeartbeatAction::Abort
                }
            }
            Some(current)
                if current.error_message.as_deref() == Some(WORKER_CLAIM_EXPIRED_ERROR) =>
            {
                RejectedRunClaimHeartbeatAction::Abort
            }
            Some(_) => RejectedRunClaimHeartbeatAction::Stop,
        };
        if action != RejectedRunClaimHeartbeatAction::Abort {
            return Ok(action);
        }

        self.store.signal_local_run_abort(claimed_run.id.as_str());
        if let Err(err) = self
            .ask_user_prompt_service
            .cancel_pending_prompts_for_run(
                claimed_run.id.as_str(),
                "run claim lost while task was executing",
            )
            .await
        {
            tracing::warn!(
                run_id = claimed_run.id.as_str(),
                error = err.as_str(),
                "failed to cancel pending ask user prompts after run claim was lost"
            );
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                claimed_run.id.clone(),
                "run.claim.execution_abort_requested".to_string(),
                Some("运行租约已丢失，旧 Worker 已停止继续执行".to_string()),
                Some(serde_json::json!({
                    "reason": "run_claim_lost",
                    "worker_id": worker_id,
                })),
            ))
            .await
        {
            tracing::warn!(
                run_id = claimed_run.id.as_str(),
                error = err.as_str(),
                "failed to append lost-claim execution abort event"
            );
        }
        Ok(RejectedRunClaimHeartbeatAction::Abort)
    }

    pub async fn batch_start_runs(
        &self,
        request: BatchTaskRunRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        self.batch_start_runs_with_user(request, None).await
    }

    pub async fn batch_start_runs_for_user(
        &self,
        request: BatchTaskRunRequest,
        current_user: &CurrentUser,
    ) -> Result<BatchTaskOperationResponse, String> {
        self.batch_start_runs_with_user(request, Some(current_user))
            .await
    }

    async fn batch_start_runs_with_user(
        &self,
        request: BatchTaskRunRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            let run_result = if let Some(current_user) = current_user {
                self.start_run_for_user(
                    &task_id,
                    StartTaskRunRequest {
                        model_config_id: request.model_config_id.clone(),
                        prompt_override: request.prompt_override.clone(),
                        retry_instruction: None,
                    },
                    current_user,
                )
                .await
            } else {
                self.start_run(
                    &task_id,
                    StartTaskRunRequest {
                        model_config_id: request.model_config_id.clone(),
                        prompt_override: request.prompt_override.clone(),
                        retry_instruction: None,
                    },
                )
                .await
            };
            match run_result {
                Ok(run) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: Some(run.id),
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub fn subscribe_run_events(&self) -> broadcast::Receiver<TaskRunEventRecord> {
        self.store.subscribe_run_events()
    }

    pub async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        self.store.list_run_events(run_id).await
    }
}

#[cfg(test)]
mod worker_claim_tests {
    use super::*;

    #[test]
    fn claim_expiry_grace_is_at_least_two_lease_periods() {
        assert_eq!(
            worker_claim_expiry_grace(Duration::from_secs(120)),
            Duration::from_secs(240)
        );
    }

    #[test]
    fn short_claim_ttl_still_gets_minimum_expiry_grace() {
        assert_eq!(
            worker_claim_expiry_grace(Duration::from_secs(30)),
            MIN_WORKER_CLAIM_EXPIRY_GRACE
        );
    }
}
