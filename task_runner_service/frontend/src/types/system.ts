export interface HealthResponse {
  status: string;
  service: string;
  now: string;
}

export interface SystemConfigResponse {
  host: string;
  port: number;
  store_mode: string;
  database_url: string;
  memory_engine_base_url?: string | null;
  memory_engine_source_id: string;
  memory_engine_configured: boolean;
  default_tenant_id: string;
  default_subject_id: string;
  default_workspace_dir: string;
  memory_timeout_ms: number;
  default_execution_timeout_ms: number;
  execution_timeout_ms: number;
  scheduler_poll_interval_ms: number;
  auto_memory_summary: boolean;
  default_task_execution_max_iterations: number;
  task_execution_max_iterations: number;
  default_tool_result_model_max_chars: number;
  tool_result_model_max_chars: number;
  default_tool_results_model_total_max_chars: number;
  tool_results_model_total_max_chars: number;
  default_execution_environment_mode: 'local' | 'cloud' | string;
  execution_environment_mode: 'local' | 'cloud' | string;
  sandbox_enabled: boolean;
  default_sandbox_manager_base_url: string;
  sandbox_manager_base_url: string;
  default_sandbox_lease_ttl_seconds: number;
  sandbox_lease_ttl_seconds: number;
}
