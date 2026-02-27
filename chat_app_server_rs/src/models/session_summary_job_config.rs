use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::repositories::session_summary_job_configs as repo;

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

#[derive(Debug, FromRow)]
pub struct SessionSummaryJobConfigRow {
    pub user_id: String,
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub updated_at: String,
}

impl SessionSummaryJobConfigRow {
    pub fn to_config(self) -> SessionSummaryJobConfig {
        SessionSummaryJobConfig {
            user_id: self.user_id,
            enabled: self.enabled == 1,
            summary_model_config_id: self.summary_model_config_id,
            token_limit: self.token_limit,
            round_limit: self.round_limit,
            target_summary_tokens: self.target_summary_tokens,
            job_interval_seconds: self.job_interval_seconds,
            updated_at: self.updated_at,
        }
    }
}

pub struct SessionSummaryJobConfigService;

impl SessionSummaryJobConfigService {
    pub async fn get_by_user(user_id: &str) -> Result<Option<SessionSummaryJobConfig>, String> {
        repo::get_config_by_user(user_id).await
    }

    pub async fn upsert(
        config: &SessionSummaryJobConfig,
    ) -> Result<SessionSummaryJobConfig, String> {
        repo::upsert_config(config).await
    }
}
