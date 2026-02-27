use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryJobDefaults {
    pub enabled: bool,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub keep_last_n_messages: usize,
    pub max_sessions_per_tick: i64,
    pub fallback_model: String,
}

impl SummaryJobDefaults {
    pub fn from_env() -> Self {
        let enabled = std::env::var("SESSION_SUMMARY_JOB_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let token_limit = std::env::var("SESSION_SUMMARY_JOB_TOKEN_LIMIT")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(6000)
            .max(500);
        let round_limit = std::env::var("SESSION_SUMMARY_JOB_ROUND_LIMIT")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(8)
            .max(1);
        let target_summary_tokens = std::env::var("SESSION_SUMMARY_JOB_TARGET_TOKENS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(700)
            .max(200);
        let job_interval_seconds = std::env::var("SESSION_SUMMARY_JOB_INTERVAL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(30)
            .max(10);
        let keep_last_n_messages = std::env::var("SESSION_SUMMARY_JOB_KEEP_LAST_N_MESSAGES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(6);
        let max_sessions_per_tick = std::env::var("SESSION_SUMMARY_JOB_MAX_SESSIONS_PER_TICK")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(50)
            .max(1);
        let fallback_model = std::env::var("SESSION_SUMMARY_JOB_MODEL")
            .unwrap_or_else(|_| "gpt-5.3-codex".to_string());

        Self {
            enabled,
            token_limit,
            round_limit,
            target_summary_tokens,
            job_interval_seconds,
            keep_last_n_messages,
            max_sessions_per_tick,
            fallback_model,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveSummaryJobConfig {
    pub enabled: bool,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub keep_last_n_messages: usize,
    pub model_config_id: Option<String>,
    pub fallback_model: String,
}
