// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskService {
    pub async fn get_runtime_settings(&self) -> Result<Option<RuntimeSettingsRecord>, String> {
        self.store.get_runtime_settings().await
    }

    pub async fn update_runtime_settings(
        &self,
        _input: UpdateRuntimeSettingsRequest,
    ) -> Result<RuntimeSettingsRecord, String> {
        Err("Task Runner 运行参数已由全局配置中心统一管理".to_string())
    }

    pub async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        let snapshot = load_managed_config_snapshot().await;
        Ok(chatos_agent::resolve_agent_max_iterations(
            snapshot.as_ref(),
            self.config.default_task_execution_max_iterations,
        ))
    }

    pub async fn effective_execution_timeout_ms(&self) -> Result<u64, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.u64("task_runner.execution.timeout_ms"))
            .unwrap_or(self.config.execution_timeout.as_millis() as u64)
            .max(1))
    }

    pub async fn effective_tool_result_model_budget_limits(
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

    pub async fn effective_execution_environment_mode(&self) -> Result<String, String> {
        Ok(normalize_execution_environment_mode(Some(
            self.config.default_execution_environment_mode.as_str(),
        )))
    }

    pub async fn effective_sandbox_enabled(&self) -> Result<bool, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.bool("task_runner.sandbox.enabled"))
            .unwrap_or(false))
    }

    pub async fn effective_sandbox_manager_base_url(&self) -> Result<String, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.string("task_runner.sandbox.manager_base_url"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.config.default_sandbox_manager_base_url.clone())
            .trim_end_matches('/')
            .to_string())
    }

    pub async fn effective_sandbox_lease_ttl_seconds(&self) -> Result<u64, String> {
        Ok(load_managed_config_snapshot()
            .await
            .and_then(|snapshot| snapshot.u64("task_runner.sandbox.lease_ttl_seconds"))
            .unwrap_or(self.config.default_sandbox_lease_ttl_seconds)
            .max(1))
    }
}
