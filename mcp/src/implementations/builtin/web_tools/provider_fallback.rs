// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;

use tokio::time::{timeout, Duration};

use super::provider_types::ProviderAttempt;
use super::provider_utils::sanitize_provider_error;

pub(super) enum StrategyRun<T> {
    Success(T),
    Failed(ProviderAttempt),
}

pub(super) async fn run_timed_strategy<F, T>(
    provider: &str,
    timeout_seconds: u64,
    future: F,
) -> StrategyRun<T>
where
    F: Future<Output = Result<T, String>>,
{
    match timeout(Duration::from_secs(timeout_seconds), future).await {
        Ok(Ok(value)) => StrategyRun::Success(value),
        Ok(Err(err)) => StrategyRun::Failed(ProviderAttempt {
            provider: provider.to_string(),
            error: sanitize_provider_error(err),
        }),
        Err(_) => StrategyRun::Failed(ProviderAttempt {
            provider: provider.to_string(),
            error: "request timed out".to_string(),
        }),
    }
}
