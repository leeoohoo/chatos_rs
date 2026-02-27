use serde_json::json;
use tracing::warn;

use crate::models::session::Session;
use crate::models::session_summary_job_config::{
    SessionSummaryJobConfig, SessionSummaryJobConfigService,
};
use crate::repositories::ai_model_configs;
use crate::services::llm_prompt_runner::PromptRunnerRuntime;

use super::types::{EffectiveSummaryJobConfig, SummaryJobDefaults};

const DEFAULT_USER_ID: &str = "default-user";

fn clamp_positive(value: i64, fallback: i64, min_value: i64) -> i64 {
    let candidate = if value > 0 { value } else { fallback };
    candidate.max(min_value)
}

pub fn resolve_user_id(session: &Session) -> String {
    session
        .user_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| DEFAULT_USER_ID.to_string())
}

pub async fn resolve_effective_config(
    session: &Session,
    defaults: &SummaryJobDefaults,
) -> EffectiveSummaryJobConfig {
    let user_id = resolve_user_id(session);
    let config = match SessionSummaryJobConfigService::get_by_user(&user_id).await {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "[SESSION-SUMMARY-JOB] load user config failed for user_id={}: {}",
                user_id, err
            );
            None
        }
    };

    to_effective_config(config, defaults)
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
        500,
    );
    let round_limit = clamp_positive(
        config.as_ref().map(|value| value.round_limit).unwrap_or(0),
        fallback_round_limit,
        1,
    );
    let target_summary_tokens = clamp_positive(
        config
            .as_ref()
            .map(|value| value.target_summary_tokens)
            .unwrap_or(0),
        fallback_target_tokens,
        200,
    );
    let job_interval_seconds = clamp_positive(
        config
            .as_ref()
            .map(|value| value.job_interval_seconds)
            .unwrap_or(0),
        fallback_job_interval_seconds,
        10,
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

pub async fn resolve_runtime(config: &EffectiveSummaryJobConfig) -> PromptRunnerRuntime {
    if let Some(model_config_id) = config.model_config_id.as_deref() {
        match ai_model_configs::get_ai_model_config_by_id(model_config_id).await {
            Ok(Some(model_cfg)) if model_cfg.enabled => {
                let source = json!({
                    "model_name": model_cfg.model,
                    "provider": model_cfg.provider,
                    "thinking_level": model_cfg.thinking_level,
                    "api_key": model_cfg.api_key,
                    "base_url": model_cfg.base_url,
                    "supports_responses": model_cfg.supports_responses,
                    "temperature": 0.2,
                });
                return PromptRunnerRuntime::from_ai_model_config(&source, &config.fallback_model);
            }
            Ok(Some(_)) => {
                warn!(
                    "[SESSION-SUMMARY-JOB] model config disabled, fallback to default model: {}",
                    model_config_id
                );
            }
            Ok(None) => {
                warn!(
                    "[SESSION-SUMMARY-JOB] model config not found, fallback to default model: {}",
                    model_config_id
                );
            }
            Err(err) => {
                warn!(
                    "[SESSION-SUMMARY-JOB] load model config failed ({}), fallback to default model: {}",
                    model_config_id, err
                );
            }
        }
    }

    let fallback = json!({
        "model_name": config.fallback_model,
        "provider": "gpt",
        "temperature": 0.2,
        "supports_responses": false,
    });
    PromptRunnerRuntime::from_ai_model_config(&fallback, &config.fallback_model)
}
