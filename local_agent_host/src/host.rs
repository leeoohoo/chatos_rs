// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_client_storage::{AgentEventStateRecord, ClientStorage, RecordScope, StorageError};
use chatos_local_agent_runtime::{
    reduce_and_commit, scan_recoverable_work, CommittedReduction, DurableScheduler,
    ModelGatewayClient, RecoveryIssue, ReduceAndCommitRequest, ReducerPolicy, SchedulerTickRequest,
    SchedulerTickResult, SingleModelStepExecutor, StepEvidence,
};
use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;

use crate::{LocalAgentProfileRegistry, ProfileRegistryError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAgentHostPolicy {
    pub claim_ttl: Duration,
    pub maximum_event_attempts: u32,
    pub reducer: ReducerPolicy,
}

impl Default for LocalAgentHostPolicy {
    fn default() -> Self {
        Self {
            claim_ttl: Duration::seconds(90),
            maximum_event_attempts: 3,
            reducer: ReducerPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentHostStartupReport {
    pub active_run_count: usize,
    pub ready_event_count: usize,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub recovery_issues: Vec<RecoveryIssue>,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentHostError {
    #[error("invalid local Agent Host configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Profile(#[from] ProfileRegistryError),
    #[error("event {actual} does not match the claimed event {expected}")]
    ClaimedEventMismatch { expected: String, actual: String },
}

pub struct LocalAgentHost {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    policy: LocalAgentHostPolicy,
    scheduler: Mutex<DurableScheduler>,
    profiles: LocalAgentProfileRegistry,
    model_steps: SingleModelStepExecutor,
}

impl LocalAgentHost {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        storage: Arc<dyn ClientStorage>,
        gateway: Arc<dyn ModelGatewayClient>,
        profiles: LocalAgentProfileRegistry,
        scope: RecordScope,
        device_id: impl Into<String>,
        policy: LocalAgentHostPolicy,
        now: DateTime<Utc>,
    ) -> Result<(Self, LocalAgentHostStartupReport), LocalAgentHostError> {
        let device_id = device_id.into();
        if scope.owner_user_id.trim().is_empty() || device_id.trim().is_empty() {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "owner user and device identifiers are required",
            ));
        }
        if profiles.is_empty() {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "at least one Agent profile must be registered",
            ));
        }
        if policy.claim_ttl <= Duration::zero() || policy.maximum_event_attempts == 0 {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "claim TTL and event attempt limit must be positive",
            ));
        }
        let plan = scan_recoverable_work(storage.as_ref(), scope.clone(), now).await?;
        let report = LocalAgentHostStartupReport {
            active_run_count: plan.active_runs.len(),
            ready_event_count: plan.ready_events.len(),
            next_wake_at: plan.next_wake_at,
            recovery_issues: plan.issues.clone(),
        };
        Ok((
            Self {
                storage,
                scope: scope.clone(),
                device_id,
                policy,
                scheduler: Mutex::new(DurableScheduler::from_recovery(scope, &plan)),
                profiles,
                model_steps: SingleModelStepExecutor::new(gateway),
            },
            report,
        ))
    }

    pub fn profiles(&self) -> &LocalAgentProfileRegistry {
        &self.profiles
    }

    pub fn model_steps(&self) -> &SingleModelStepExecutor {
        &self.model_steps
    }

    pub async fn claim_next(
        &self,
        claim_token: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<SchedulerTickResult, LocalAgentHostError> {
        Ok(self
            .scheduler
            .lock()
            .await
            .tick(
                self.storage.as_ref(),
                SchedulerTickRequest {
                    device_id: self.device_id.clone(),
                    claim_token: claim_token.into(),
                    now,
                    claim_ttl: self.policy.claim_ttl,
                    max_attempts: self.policy.maximum_event_attempts,
                },
            )
            .await?)
    }

    pub async fn commit_claimed(
        &self,
        claimed: &AgentEventStateRecord,
        evidence: StepEvidence,
        now: DateTime<Utc>,
    ) -> Result<CommittedReduction, LocalAgentHostError> {
        if claimed.metadata.id != claimed.event.event_id {
            return Err(LocalAgentHostError::ClaimedEventMismatch {
                expected: claimed.metadata.id.clone(),
                actual: claimed.event.event_id.clone(),
            });
        }
        let committed = reduce_and_commit(
            self.storage.as_ref(),
            ReduceAndCommitRequest {
                scope: self.scope.clone(),
                event_id: claimed.event.event_id.clone(),
                claim_token: claimed.event.claim_token.clone().unwrap_or_default(),
                origin_device_id: self.device_id.clone(),
                evidence,
                now,
                policy: self.policy.reducer,
            },
        )
        .await?;
        self.scheduler
            .lock()
            .await
            .schedule_all(committed.emitted_events.iter());
        Ok(committed)
    }

    pub async fn refresh_recovery(
        &self,
        now: DateTime<Utc>,
    ) -> Result<LocalAgentHostStartupReport, LocalAgentHostError> {
        let plan = scan_recoverable_work(self.storage.as_ref(), self.scope.clone(), now).await?;
        self.scheduler.lock().await.merge_recovery(&plan);
        Ok(LocalAgentHostStartupReport {
            active_run_count: plan.active_runs.len(),
            ready_event_count: plan.ready_events.len(),
            next_wake_at: plan.next_wake_at,
            recovery_issues: plan.issues,
        })
    }
}
