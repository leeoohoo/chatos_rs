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
        Ok(self
            .effective_task_runner_runtime_settings()
            .await?
            .max_iterations)
    }

    pub async fn effective_task_runner_runtime_settings(
        &self,
    ) -> Result<chatos_agent::TaskRunnerRuntimeSettings, String> {
        let snapshot = load_managed_config_snapshot().await?;
        chatos_agent::require_task_runner_runtime_settings(&snapshot)
    }

    pub async fn effective_execution_timeout_ms(&self) -> Result<u64, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_u64(&snapshot, TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY, 1)
    }

    pub async fn effective_ai_read_timeout_ms(&self) -> Result<u64, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_u64(&snapshot, TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY, 1)
    }

    pub async fn effective_tool_result_model_budget_limits(
        &self,
    ) -> Result<ToolResultModelBudgetLimits, String> {
        let snapshot = load_managed_config_snapshot().await?;
        let per_result =
            require_managed_usize(&snapshot, TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY, 1)?;
        let total = require_managed_usize(
            &snapshot,
            TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
            1,
        )?;
        if total < per_result {
            return Err(format!(
                "managed configuration key {} must be greater than or equal to {}",
                TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
                TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY
            ));
        }
        Ok(ToolResultModelBudgetLimits::new(per_result, total))
    }
}
