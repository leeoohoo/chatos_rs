use serde::{Deserialize, Serialize};

use super::{default_agent_memory_max_level, default_i64_1, default_keep_raw_level0_count};

pub const DEFAULT_SUMMARY_PROMPT_TEMPLATE: &str = "你是 Memory Server 的总结引擎。请输出结构化简洁总结，重点保留事实、决策、风险、待办。目标长度约 {{target_tokens}} tokens。";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    #[serde(default)]
    pub summary_prompt: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub max_sessions_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSummaryJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub max_sessions_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRollupJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    #[serde(default)]
    pub summary_prompt: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub keep_raw_level0_count: i64,
    pub max_level: i64,
    pub max_sessions_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    #[serde(default)]
    pub summary_prompt: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    #[serde(default = "default_keep_raw_level0_count")]
    pub keep_raw_level0_count: i64,
    #[serde(default = "default_agent_memory_max_level")]
    pub max_level: i64,
    pub max_agents_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertAgentMemoryJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_agents_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSummaryRollupJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_sessions_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    pub id: String,
    pub job_type: String,
    pub session_id: Option<String>,
    pub status: String,
    pub trigger_type: Option<String>,
    pub input_count: i64,
    pub output_count: i64,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}
