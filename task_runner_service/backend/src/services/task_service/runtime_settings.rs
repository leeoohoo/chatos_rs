use super::*;

impl TaskService {
    pub async fn get_runtime_settings(&self) -> Result<Option<RuntimeSettingsRecord>, String> {
        self.store.get_runtime_settings().await
    }

    pub async fn update_runtime_settings(
        &self,
        input: UpdateRuntimeSettingsRequest,
    ) -> Result<RuntimeSettingsRecord, String> {
        if input.task_execution_max_iterations == Some(0) {
            return Err("task_execution_max_iterations 必须大于 0".to_string());
        }
        if input.tool_result_model_max_chars == Some(0) {
            return Err("tool_result_model_max_chars 必须大于 0".to_string());
        }
        if input.tool_results_model_total_max_chars == Some(0) {
            return Err("tool_results_model_total_max_chars 必须大于 0".to_string());
        }

        let now = now_rfc3339();
        let mut settings = self
            .get_runtime_settings()
            .await?
            .unwrap_or(RuntimeSettingsRecord {
                id: SYSTEM_RUNTIME_SETTINGS_ID.to_string(),
                task_execution_max_iterations: self.config.default_task_execution_max_iterations,
                tool_result_model_max_chars: self.config.default_tool_result_model_max_chars,
                tool_results_model_total_max_chars: self
                    .config
                    .default_tool_results_model_total_max_chars,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        if let Some(task_execution_max_iterations) = input.task_execution_max_iterations {
            settings.task_execution_max_iterations = task_execution_max_iterations;
        }
        if let Some(tool_result_model_max_chars) = input.tool_result_model_max_chars {
            settings.tool_result_model_max_chars = tool_result_model_max_chars;
        }
        if let Some(tool_results_model_total_max_chars) = input.tool_results_model_total_max_chars {
            settings.tool_results_model_total_max_chars = tool_results_model_total_max_chars;
        }
        settings.updated_at = now;
        self.store.save_runtime_settings(settings).await
    }

    pub async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        Ok(self
            .get_runtime_settings()
            .await?
            .map(|settings| settings.task_execution_max_iterations.max(1))
            .unwrap_or(self.config.default_task_execution_max_iterations.max(1)))
    }

    pub async fn effective_tool_result_model_budget_limits(
        &self,
    ) -> Result<ToolResultModelBudgetLimits, String> {
        Ok(self
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
}
