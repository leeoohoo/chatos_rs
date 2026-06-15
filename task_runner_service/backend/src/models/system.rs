use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub now: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfigResponse {
    pub host: String,
    pub port: u16,
    pub store_mode: String,
    pub database_url: String,
    pub memory_engine_base_url: Option<String>,
    pub memory_engine_source_id: String,
    pub memory_engine_configured: bool,
    pub default_tenant_id: String,
    pub default_subject_id: String,
    pub default_workspace_dir: String,
    pub memory_timeout_ms: u64,
    pub execution_timeout_ms: u64,
    pub scheduler_poll_interval_ms: u64,
    pub auto_memory_summary: bool,
    pub default_task_execution_max_iterations: usize,
    pub task_execution_max_iterations: usize,
    pub default_tool_result_model_max_chars: usize,
    pub tool_result_model_max_chars: usize,
    pub default_tool_results_model_total_max_chars: usize,
    pub tool_results_model_total_max_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerInternalPromptPreviewResponse {
    pub locale: String,
    pub task_prompt_template: String,
    pub process_log_system_prompt: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}
