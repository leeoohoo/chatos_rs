// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chrono::{TimeDelta, Utc};
use tracing::{info, warn};

use crate::repositories::agent_artifacts;
use crate::services::object_storage::{service as object_storage_service, StoredObjectRef};

const DEFAULT_STAGED_TTL_SECONDS: i64 = 24 * 60 * 60;
const DEFAULT_RECONCILE_INTERVAL_SECONDS: u64 = 60;
const DEFAULT_BATCH_SIZE: i64 = 50;

pub fn spawn_reconciler() {
    tokio::spawn(async {
        let interval_seconds = configured_u64(
            "CHATOS_AGENT_ARTIFACT_RECONCILE_INTERVAL_SECONDS",
            DEFAULT_RECONCILE_INTERVAL_SECONDS,
        )
        .clamp(10, 3_600);
        loop {
            if let Err(error) = reconcile_once().await {
                warn!("Agent artifact maintenance pass failed: {error}");
            }
            tokio::time::sleep(Duration::from_secs(interval_seconds)).await;
        }
    });
}

pub async fn reconcile_once() -> Result<(u64, usize), String> {
    let ttl_seconds = configured_i64(
        "CHATOS_AGENT_ARTIFACT_STAGED_TTL_SECONDS",
        DEFAULT_STAGED_TTL_SECONDS,
    )
    .clamp(60, 30 * 24 * 60 * 60);
    let cutoff = Utc::now()
        .checked_sub_signed(TimeDelta::seconds(ttl_seconds))
        .ok_or_else(|| "agent artifact staged TTL is invalid".to_string())?;
    let enqueued = agent_artifacts::enqueue_expired_staged(cutoff, DEFAULT_BATCH_SIZE).await?;

    let storage = object_storage_service().await?;
    let jobs = agent_artifacts::claim_deletions(DEFAULT_BATCH_SIZE).await?;
    let mut completed = 0;
    for job in jobs {
        let deletion = storage
            .delete_object(&StoredObjectRef {
                bucket: Some(job.record.bucket.clone()),
                object_key: job.record.object_key.clone(),
                name: Some(job.record.name.clone()),
                mime_type: Some(job.record.mime_type.clone()),
            })
            .await;
        match deletion {
            Ok(()) => {
                if agent_artifacts::complete_deletion(job.record.id.as_str()).await? {
                    completed += 1;
                }
            }
            Err(error) => {
                let delay = deletion_retry_delay_seconds(job.attempt);
                let next_attempt_at = Utc::now()
                    .checked_add_signed(TimeDelta::seconds(delay))
                    .ok_or_else(|| "agent artifact deletion retry time overflowed".to_string())?;
                agent_artifacts::fail_deletion(
                    job.record.id.as_str(),
                    job.attempt,
                    next_attempt_at,
                    error.as_str(),
                )
                .await?;
            }
        }
    }
    if enqueued > 0 || completed > 0 {
        info!(
            "Agent artifact maintenance completed: expired_enqueued={enqueued} deleted={completed}"
        );
    }
    Ok((enqueued, completed))
}

fn deletion_retry_delay_seconds(attempt: i32) -> i64 {
    let exponent = (attempt.saturating_sub(1) as u32).min(10);
    (5_i64.saturating_mul(1_i64 << exponent)).min(3_600)
}

fn configured_u64(key: &str, fallback: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

fn configured_i64(key: &str, fallback: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_retry_uses_bounded_exponential_backoff() {
        assert_eq!(deletion_retry_delay_seconds(1), 5);
        assert_eq!(deletion_retry_delay_seconds(2), 10);
        assert_eq!(deletion_retry_delay_seconds(10), 2_560);
        assert_eq!(deletion_retry_delay_seconds(11), 3_600);
        assert_eq!(deletion_retry_delay_seconds(i32::MAX), 3_600);
    }
}
