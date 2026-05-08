use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE: &str =
    "你是 memory engine 的总结引擎。请输出结构化、简洁、可复用的总结，重点保留事实、约束、决策、风险、待办和当前用户目标。";

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineModelProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertEngineModelProfileRequest {
    pub name: String,
    pub provider: String,
    #[serde(alias = "model_name")]
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobPolicy {
    pub job_type: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub model_profile_id: Option<String>,
    pub summary_prompt: Option<String>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub interval_seconds: Option<i64>,
    pub max_threads_per_tick: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_records_per_thread: Option<i64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertEngineJobPolicyRequest {
    pub enabled: Option<bool>,
    pub model_profile_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<Option<i64>>,
    pub round_limit: Option<Option<i64>>,
    pub target_summary_tokens: Option<Option<i64>>,
    pub interval_seconds: Option<Option<i64>>,
    pub max_threads_per_tick: Option<Option<i64>>,
    pub keep_level0_count: Option<Option<i64>>,
    pub max_level: Option<Option<i64>>,
    pub max_records_per_thread: Option<Option<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobRun {
    pub id: String,
    pub job_type: String,
    pub trigger_type: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject_id: Option<String>,
    pub thread_label: Option<String>,
    pub status: String,
    pub input_count: i64,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub metadata: Option<Value>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateEngineJobRunRequest {
    pub job_type: String,
    pub trigger_type: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject_id: Option<String>,
    pub thread_label: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct FinishEngineJobRunRequest {
    pub status: String,
    pub input_count: i64,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub metadata: Option<Value>,
    pub error_message: Option<String>,
}
