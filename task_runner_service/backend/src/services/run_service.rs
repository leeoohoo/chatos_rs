// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
        }
    }

    pub(super) async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        let snapshot = load_managed_config_snapshot().await;
        Ok(chatos_agent::resolve_agent_max_iterations(
            snapshot.as_ref(),
            self.config.default_task_execution_max_iterations,
        ))
    }

    pub(super) async fn effective_execution_timeout(&self) -> Result<Duration, String> {
        Ok(Duration::from_millis(
            load_managed_config_snapshot()
                .await
                .and_then(|snapshot| snapshot.u64("task_runner.execution.timeout_ms"))
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
                .and_then(|snapshot| snapshot.usize("task_runner.ai.tool_result_max_chars"))
                .unwrap_or(self.config.default_tool_result_model_max_chars),
            snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.usize("task_runner.ai.tool_results_total_max_chars"))
                .unwrap_or(self.config.default_tool_results_model_total_max_chars),
        ))
    }

    pub(super) async fn effective_execution_environment_mode(&self) -> Result<String, String> {
        Ok(normalize_execution_environment_mode(Some(
            self.config.default_execution_environment_mode.as_str(),
        )))
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

    pub async fn fail_expired_run_claims(&self) -> Result<usize, String> {
        let now = now_rfc3339();
        self.store.fail_expired_run_claims(now.as_str()).await
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
