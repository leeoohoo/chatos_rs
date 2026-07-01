// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
        if input.execution_timeout_ms == Some(0) {
            return Err("execution_timeout_ms 必须大于 0".to_string());
        }
        if input.tool_result_model_max_chars == Some(0) {
            return Err("tool_result_model_max_chars 必须大于 0".to_string());
        }
        if input.tool_results_model_total_max_chars == Some(0) {
            return Err("tool_results_model_total_max_chars 必须大于 0".to_string());
        }
        if input.sandbox_lease_ttl_seconds == Some(0) {
            return Err("sandbox_lease_ttl_seconds 必须大于 0".to_string());
        }

        let now = now_rfc3339();
        let mut settings = self
            .get_runtime_settings()
            .await?
            .unwrap_or(RuntimeSettingsRecord {
                id: SYSTEM_RUNTIME_SETTINGS_ID.to_string(),
                task_execution_max_iterations: self.config.default_task_execution_max_iterations,
                execution_timeout_ms: Some(self.config.execution_timeout.as_millis() as u64),
                tool_result_model_max_chars: self.config.default_tool_result_model_max_chars,
                tool_results_model_total_max_chars: self
                    .config
                    .default_tool_results_model_total_max_chars,
                execution_environment_mode: self.config.default_execution_environment_mode.clone(),
                sandbox_enabled: false,
                sandbox_manager_base_url: self.config.default_sandbox_manager_base_url.clone(),
                sandbox_lease_ttl_seconds: self.config.default_sandbox_lease_ttl_seconds,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        if let Some(task_execution_max_iterations) = input.task_execution_max_iterations {
            settings.task_execution_max_iterations = task_execution_max_iterations;
        }
        if let Some(execution_timeout_ms) = input.execution_timeout_ms {
            settings.execution_timeout_ms = Some(execution_timeout_ms);
        }
        if let Some(tool_result_model_max_chars) = input.tool_result_model_max_chars {
            settings.tool_result_model_max_chars = tool_result_model_max_chars;
        }
        if let Some(tool_results_model_total_max_chars) = input.tool_results_model_total_max_chars {
            settings.tool_results_model_total_max_chars = tool_results_model_total_max_chars;
        }
        settings.execution_environment_mode =
            self.config.default_execution_environment_mode.clone();
        if let Some(sandbox_enabled) = input.sandbox_enabled {
            settings.sandbox_enabled = sandbox_enabled;
        }
        if let Some(base_url) = input.sandbox_manager_base_url {
            let base_url = base_url.trim();
            if !base_url.is_empty() {
                settings.sandbox_manager_base_url = base_url.trim_end_matches('/').to_string();
            }
        }
        if let Some(ttl_seconds) = input.sandbox_lease_ttl_seconds {
            settings.sandbox_lease_ttl_seconds = ttl_seconds.max(1);
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

    pub async fn effective_execution_timeout_ms(&self) -> Result<u64, String> {
        Ok(self
            .get_runtime_settings()
            .await?
            .and_then(|settings| settings.execution_timeout_ms)
            .filter(|value| *value > 0)
            .unwrap_or(self.config.execution_timeout.as_millis() as u64)
            .max(1))
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

    pub async fn effective_execution_environment_mode(&self) -> Result<String, String> {
        Ok(normalize_execution_environment_mode(Some(
            self.config.default_execution_environment_mode.as_str(),
        )))
    }

    pub async fn effective_sandbox_enabled(&self) -> Result<bool, String> {
        Ok(self
            .get_runtime_settings()
            .await?
            .map(|settings| settings.sandbox_enabled)
            .unwrap_or(false))
    }

    pub async fn effective_sandbox_manager_base_url(&self) -> Result<String, String> {
        Ok(self
            .get_runtime_settings()
            .await?
            .map(|settings| settings.sandbox_manager_base_url)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.config.default_sandbox_manager_base_url.clone())
            .trim_end_matches('/')
            .to_string())
    }

    pub async fn effective_sandbox_lease_ttl_seconds(&self) -> Result<u64, String> {
        Ok(self
            .get_runtime_settings()
            .await?
            .map(|settings| settings.sandbox_lease_ttl_seconds.max(1))
            .unwrap_or(self.config.default_sandbox_lease_ttl_seconds.max(1)))
    }
}
