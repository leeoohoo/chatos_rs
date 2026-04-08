use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryJobConfig {
    pub user_id: String,
    pub enabled: bool,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub updated_at: String,
}
