use tracing::warn;

use crate::models::session::SessionService;
use crate::models::session_summary_job_config::{
    SessionSummaryJobConfig, SessionSummaryJobConfigService,
};
use crate::models::sub_agent_run::SubAgentRunService;

use super::types::{
    EffectiveSummaryJobConfig, SummaryJobDefaults, MIN_JOB_INTERVAL_SECONDS, MIN_ROUND_LIMIT,
    MIN_TARGET_SUMMARY_TOKENS, MIN_TOKEN_LIMIT,
};

const DEFAULT_USER_ID: &str = "default-user";

fn clamp_positive(value: i64, fallback: i64, min_value: i64) -> i64 {
    let candidate = if value > 0 { value } else { fallback };
    candidate.max(min_value)
}

pub async fn resolve_effective_config(
    run_id: &str,
    defaults: &SummaryJobDefaults,
) -> Result<Option<EffectiveSummaryJobConfig>, String> {
    let Some(run) = SubAgentRunService::get_by_id(run_id).await? else {
        return Ok(None);
    };

    let user_id = if run.session_id.trim().is_empty() {
        DEFAULT_USER_ID.to_string()
    } else {
        match SessionService::get_by_id(run.session_id.as_str()).await {
            Ok(Some(session)) => session
                .user_id
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .unwrap_or_else(|| DEFAULT_USER_ID.to_string()),
            Ok(None) => DEFAULT_USER_ID.to_string(),
            Err(err) => {
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] load parent session failed, fallback default user: run_id={} session_id={} error={}",
                    run_id, run.session_id, err
                );
                DEFAULT_USER_ID.to_string()
            }
        }
    };

    let user_config = match SessionSummaryJobConfigService::get_by_user(user_id.as_str()).await {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "[SUB-AGENT-SUMMARY-JOB] load user summary config failed for user_id={}: {}",
                user_id, err
            );
            None
        }
    };

    Ok(Some(to_effective_config(user_config, defaults)))
}

fn to_effective_config(
    config: Option<SessionSummaryJobConfig>,
    defaults: &SummaryJobDefaults,
) -> EffectiveSummaryJobConfig {
    let fallback_enabled = defaults.enabled;
    let fallback_token_limit = defaults.token_limit;
    let fallback_round_limit = defaults.round_limit;
    let fallback_target_tokens = defaults.target_summary_tokens;
    let fallback_job_interval_seconds = defaults.job_interval_seconds;

    let enabled = config
        .as_ref()
        .map(|value| value.enabled)
        .unwrap_or(fallback_enabled);
    let token_limit = clamp_positive(
        config.as_ref().map(|value| value.token_limit).unwrap_or(0),
        fallback_token_limit,
        MIN_TOKEN_LIMIT,
    );
    let round_limit = clamp_positive(
        config.as_ref().map(|value| value.round_limit).unwrap_or(0),
        fallback_round_limit,
        MIN_ROUND_LIMIT,
    );
    let target_summary_tokens = clamp_positive(
        config
            .as_ref()
            .map(|value| value.target_summary_tokens)
            .unwrap_or(0),
        fallback_target_tokens,
        MIN_TARGET_SUMMARY_TOKENS,
    );
    let job_interval_seconds = clamp_positive(
        config
            .as_ref()
            .map(|value| value.job_interval_seconds)
            .unwrap_or(0),
        fallback_job_interval_seconds,
        MIN_JOB_INTERVAL_SECONDS,
    );

    EffectiveSummaryJobConfig {
        enabled,
        token_limit,
        round_limit,
        target_summary_tokens,
        job_interval_seconds,
        keep_last_n_messages: defaults.keep_last_n_messages,
        model_config_id: config
            .as_ref()
            .and_then(|value| value.summary_model_config_id.clone()),
        fallback_model: defaults.fallback_model.clone(),
    }
}
