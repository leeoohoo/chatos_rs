use super::*;

impl RunService {
    pub(crate) fn new(
        config: AppConfig,
        store: AppStore,
        ui_prompt_service: UiPromptService,
    ) -> Self {
        Self {
            config,
            store,
            ui_prompt_service,
            start_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    pub(super) async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        Ok(self
            .store
            .get_runtime_settings()
            .await?
            .map(|settings| settings.task_execution_max_iterations.max(1))
            .unwrap_or(self.config.default_task_execution_max_iterations.max(1)))
    }

    pub(super) async fn effective_tool_result_model_budget_limits(
        &self,
    ) -> Result<ToolResultModelBudgetLimits, String> {
        Ok(self
            .store
            .get_runtime_settings()
            .await?
            .map(|settings| {
                ToolResultModelBudgetLimits::new(
                    settings.tool_result_model_max_chars,
                    settings.tool_results_model_total_max_chars,
                )
            })
            .unwrap_or_else(|| {
                ToolResultModelBudgetLimits::new(
                    self.config.default_tool_result_model_max_chars,
                    self.config.default_tool_results_model_total_max_chars,
                )
            }))
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

    pub async fn batch_start_runs(
        &self,
        request: BatchTaskRunRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            match self
                .start_run(
                    &task_id,
                    StartTaskRunRequest {
                        model_config_id: request.model_config_id.clone(),
                        prompt_override: request.prompt_override.clone(),
                    },
                )
                .await
            {
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
