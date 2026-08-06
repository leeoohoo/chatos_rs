// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::state::TaskRunnerRuntimeStats;

const RETENTION_DAYS_ENV: &str = "TASK_RUNNER_ASK_USER_PROMPT_RETENTION_DAYS";
const CLEANUP_INTERVAL_MS_ENV: &str = "TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_INTERVAL_MS";
const CLEANUP_BATCH_SIZE_ENV: &str = "TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_BATCH_SIZE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskUserPromptRetentionPolicy {
    retention_days: u64,
    cleanup_interval: Duration,
    cleanup_batch_size: usize,
}

impl AskUserPromptRetentionPolicy {
    pub fn from_managed_env() -> Result<Self, String> {
        Self::from_values(
            required_u64(RETENTION_DAYS_ENV)?,
            required_u64(CLEANUP_INTERVAL_MS_ENV)?,
            required_usize(CLEANUP_BATCH_SIZE_ENV)?,
        )
    }

    fn from_values(
        retention_days: u64,
        cleanup_interval_ms: u64,
        cleanup_batch_size: usize,
    ) -> Result<Self, String> {
        if !(1..=3650).contains(&retention_days) {
            return Err(format!("{RETENTION_DAYS_ENV} must be between 1 and 3650"));
        }
        if !(60_000..=86_400_000).contains(&cleanup_interval_ms) {
            return Err(format!(
                "{CLEANUP_INTERVAL_MS_ENV} must be between 60000 and 86400000"
            ));
        }
        if !(1..=10_000).contains(&cleanup_batch_size) {
            return Err(format!(
                "{CLEANUP_BATCH_SIZE_ENV} must be between 1 and 10000"
            ));
        }
        Ok(Self {
            retention_days,
            cleanup_interval: Duration::from_millis(cleanup_interval_ms),
            cleanup_batch_size,
        })
    }

    fn cutoff(&self) -> Result<String, String> {
        let days = i64::try_from(self.retention_days)
            .map_err(|_| "Ask User prompt retention days overflow".to_string())?;
        Ok((Utc::now() - ChronoDuration::days(days)).to_rfc3339())
    }
}

pub fn spawn_ask_user_prompt_retention(
    policy: AskUserPromptRetentionPolicy,
    service: AskUserPromptService,
    runtime_stats: TaskRunnerRuntimeStats,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match policy.cutoff() {
                Ok(cutoff) => match service
                    .prune_terminal_prompts_before(cutoff.as_str(), policy.cleanup_batch_size)
                    .await
                {
                    Ok(result) => {
                        runtime_stats
                            .record_ask_user_prompt_retention_success(result.deleted_prompts);
                        tracing::info!(
                            cutoff = cutoff.as_str(),
                            eligible_prompts = result.eligible_prompts,
                            deleted_prompts = result.deleted_prompts,
                            "task runner pruned expired terminal Ask User prompts"
                        );
                    }
                    Err(error) => {
                        runtime_stats.record_ask_user_prompt_retention_failure();
                        tracing::warn!(
                            cutoff = cutoff.as_str(),
                            error = error.as_str(),
                            "task runner failed to prune expired Ask User prompts"
                        );
                    }
                },
                Err(error) => {
                    runtime_stats.record_ask_user_prompt_retention_failure();
                    tracing::error!(
                        error = error.as_str(),
                        "task runner failed to calculate Ask User prompt retention cutoff"
                    );
                }
            }
            tokio::time::sleep(policy.cleanup_interval).await;
        }
    })
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = chatos_service_runtime::env_text(key)
        .ok_or_else(|| format!("{key} must be provided by configuration center"))?;
    value
        .parse::<u64>()
        .map_err(|error| format!("{key} must be an unsigned integer: {error}"))
}

fn required_usize(key: &str) -> Result<usize, String> {
    usize::try_from(required_u64(key)?).map_err(|_| format!("{key} is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_user_prompt_retention_policy_rejects_invalid_boundaries() {
        assert!(AskUserPromptRetentionPolicy::from_values(0, 60_000, 1).is_err());
        assert!(AskUserPromptRetentionPolicy::from_values(90, 59_999, 1).is_err());
        assert!(AskUserPromptRetentionPolicy::from_values(90, 60_000, 0).is_err());
        assert!(AskUserPromptRetentionPolicy::from_values(90, 3_600_000, 200).is_ok());
    }
}
