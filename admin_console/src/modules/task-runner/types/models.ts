// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface RuntimeSettingsRecord {
  id: string;
  task_execution_max_iterations: number;
  execution_timeout_ms?: number | null;
  tool_result_model_max_chars: number;
  tool_results_model_total_max_chars: number;
  created_at: string;
  updated_at: string;
}

export interface UpdateRuntimeSettingsPayload {
  task_execution_max_iterations?: number;
  execution_timeout_ms?: number;
  tool_result_model_max_chars?: number;
  tool_results_model_total_max_chars?: number;
}
