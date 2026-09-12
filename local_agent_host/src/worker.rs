// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chatos_local_agent_runtime::SchedulerTickResult;
use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;

use crate::{stable_host_id, LocalAgentExecutionSession, LocalAgentHost, LocalAgentHostError};

const MAXIMUM_IDLE_SLEEP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentWorkerExit {
    pub processed_event_count: u64,
}

/// The sole long-lived event consumer for every local Agent Profile. It owns
/// no Agent state and performs one durable claim and one bounded event
/// dispatch at a time; all continuation state is committed before the next
/// iteration.
pub struct LocalAgentHostWorker {
    host: Arc<LocalAgentHost>,
    session: LocalAgentExecutionSession,
    worker_id: String,
    claim_sequence: AtomicU64,
}

impl LocalAgentHostWorker {
    pub fn new(
        host: Arc<LocalAgentHost>,
        session: LocalAgentExecutionSession,
        worker_id: impl Into<String>,
    ) -> Result<Self, LocalAgentHostError> {
        let worker_id = worker_id.into();
        if worker_id.trim().is_empty() || worker_id.trim() != worker_id || worker_id.len() > 512 {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "local Agent worker ID is invalid",
            ));
        }
        Ok(Self {
            host,
            session,
            worker_id,
            claim_sequence: AtomicU64::new(0),
        })
    }

    pub async fn run(
        &self,
        shutdown: CancellationToken,
    ) -> Result<LocalAgentWorkerExit, LocalAgentHostError> {
        let mut processed_event_count = 0u64;
        loop {
            if shutdown.is_cancelled() {
                return Ok(LocalAgentWorkerExit {
                    processed_event_count,
                });
            }
            let now = Utc::now();
            let claim_token = self.next_claim_token(now)?;
            match self.host.claim_next(claim_token, now).await? {
                SchedulerTickResult::Claimed(claimed) => {
                    self.host
                        .process_claimed_event(&claimed, &self.session, Utc::now())
                        .await?;
                    processed_event_count = processed_event_count.checked_add(1).ok_or(
                        LocalAgentHostError::InvalidConfiguration(
                            "local Agent worker event counter overflowed",
                        ),
                    )?;
                }
                SchedulerTickResult::Idle { next_wake_at } => {
                    self.wait_until_ready(next_wake_at, &shutdown).await;
                }
                SchedulerTickResult::RescanRequired
                | SchedulerTickResult::AttemptsExhausted { .. }
                | SchedulerTickResult::CandidateUnavailable { .. }
                | SchedulerTickResult::CandidateAlreadyFinished { .. } => {
                    self.host.refresh_recovery(Utc::now()).await?;
                }
            }
        }
    }

    fn next_claim_token(&self, now: DateTime<Utc>) -> Result<String, LocalAgentHostError> {
        let sequence = self
            .claim_sequence
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_add(1)
            })
            .map_err(|_| {
                LocalAgentHostError::InvalidConfiguration(
                    "local Agent worker claim sequence overflowed",
                )
            })?;
        Ok(stable_host_id(
            "claim",
            &[
                self.worker_id.as_str(),
                sequence.to_string().as_str(),
                now.timestamp_nanos_opt()
                    .unwrap_or_default()
                    .to_string()
                    .as_str(),
            ],
        ))
    }

    async fn wait_until_ready(
        &self,
        next_wake_at: Option<DateTime<Utc>>,
        shutdown: &CancellationToken,
    ) {
        let Some(delay) = next_wake_delay(next_wake_at, Utc::now()) else {
            tokio::select! {
                _ = shutdown.cancelled() => {}
                _ = self.host.wait_for_scheduler_wake() => {}
            }
            return;
        };
        tokio::select! {
            _ = shutdown.cancelled() => {}
            _ = self.host.wait_for_scheduler_wake() => {}
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

fn next_wake_delay(next_wake_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<Duration> {
    let next_wake_at = next_wake_at?;
    if next_wake_at <= now {
        return Some(Duration::ZERO);
    }
    Some(
        (next_wake_at - now)
            .to_std()
            .unwrap_or(Duration::ZERO)
            .min(MAXIMUM_IDLE_SLEEP),
    )
}

#[cfg(test)]
mod tests {
    use super::next_wake_delay;
    use chrono::{Duration as ChronoDuration, Utc};
    use std::time::Duration;

    #[test]
    fn idle_wait_is_immediate_for_due_work_and_bounded_for_future_rescans() {
        let now = Utc::now();
        assert_eq!(
            next_wake_delay(Some(now - ChronoDuration::seconds(1)), now),
            Some(Duration::ZERO)
        );
        assert_eq!(
            next_wake_delay(Some(now + ChronoDuration::hours(1)), now),
            Some(Duration::from_secs(60))
        );
        assert_eq!(next_wake_delay(None, now), None);
    }
}
