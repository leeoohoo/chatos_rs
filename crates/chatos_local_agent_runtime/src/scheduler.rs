// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeSet, HashMap};

use chatos_client_storage::{AgentEventStateRecord, ClientStorage, RecordScope, StorageResult};
use chrono::{DateTime, Duration, Utc};

use crate::{
    claim_event, AttemptLimitDisposition, EventClaimRequest, EventClaimResult, RecoveryPlan,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScheduledEvent {
    available_at: DateTime<Utc>,
    event_id: String,
}

#[derive(Debug, Clone)]
pub struct SchedulerTickRequest {
    pub device_id: String,
    pub claim_token: String,
    pub now: DateTime<Utc>,
    pub claim_ttl: Duration,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerTickResult {
    Claimed(Box<AgentEventStateRecord>),
    AttemptsExhausted {
        event_id: String,
        disposition: AttemptLimitDisposition,
    },
    CandidateUnavailable {
        event_id: String,
    },
    CandidateAlreadyFinished {
        event_id: String,
    },
    RescanRequired,
    Idle {
        next_wake_at: Option<DateTime<Utc>>,
    },
}

/// In-memory wake queue for one owner scope. It never owns an Agent loop:
/// callers invoke one bounded tick, execute the claimed step, persist it, then
/// enqueue emitted events. Durable storage remains the authority.
#[derive(Debug, Clone)]
pub struct DurableScheduler {
    scope: RecordScope,
    scheduled: BTreeSet<ScheduledEvent>,
    due_by_event_id: HashMap<String, DateTime<Utc>>,
    next_rescan_at: Option<DateTime<Utc>>,
}

impl DurableScheduler {
    pub fn from_recovery(scope: RecordScope, plan: &RecoveryPlan) -> Self {
        let mut scheduler = Self {
            scope,
            scheduled: BTreeSet::new(),
            due_by_event_id: HashMap::new(),
            next_rescan_at: plan.next_wake_at,
        };
        scheduler.schedule_all(plan.ready_events.iter());
        scheduler
    }

    pub fn merge_recovery(&mut self, plan: &RecoveryPlan) {
        self.next_rescan_at = plan.next_wake_at;
        self.schedule_all(plan.ready_events.iter());
    }

    pub fn schedule(&mut self, record: &AgentEventStateRecord) {
        let event_id = record.event.event_id.clone();
        let available_at = record.event.available_at;
        if let Some(previous_due) = self.due_by_event_id.insert(event_id.clone(), available_at) {
            self.scheduled.remove(&ScheduledEvent {
                available_at: previous_due,
                event_id: event_id.clone(),
            });
        }
        self.scheduled.insert(ScheduledEvent {
            available_at,
            event_id,
        });
    }

    pub fn schedule_all<'record>(
        &mut self,
        records: impl IntoIterator<Item = &'record AgentEventStateRecord>,
    ) {
        for record in records {
            self.schedule(record);
        }
    }

    pub fn next_wake_at(&self) -> Option<DateTime<Utc>> {
        let candidate_wake = self.scheduled.first().map(|event| event.available_at);
        earliest(candidate_wake, self.next_rescan_at)
    }

    /// Attempts at most one durable event claim. The Host schedules another
    /// tick after handling the result; this method contains no polling loop or
    /// sleep and is safe to call from platform lifecycle code.
    pub async fn tick(
        &mut self,
        storage: &dyn ClientStorage,
        request: SchedulerTickRequest,
    ) -> StorageResult<SchedulerTickResult> {
        let claim_until = request
            .now
            .checked_add_signed(request.claim_ttl)
            .ok_or_else(|| chatos_client_storage::StorageError::InvalidData {
                reason: "scheduler claim deadline overflow".to_string(),
            })?;
        if claim_until <= request.now {
            return Err(chatos_client_storage::StorageError::InvalidData {
                reason: "scheduler claim TTL must be positive".to_string(),
            });
        }

        let Some(candidate) = self.scheduled.first().cloned() else {
            if self.next_rescan_at.is_some_and(|wake| wake <= request.now) {
                return Ok(SchedulerTickResult::RescanRequired);
            }
            return Ok(SchedulerTickResult::Idle {
                next_wake_at: self.next_wake_at(),
            });
        };
        if candidate.available_at > request.now {
            if self.next_rescan_at.is_some_and(|wake| wake <= request.now) {
                return Ok(SchedulerTickResult::RescanRequired);
            }
            return Ok(SchedulerTickResult::Idle {
                next_wake_at: self.next_wake_at(),
            });
        }
        self.scheduled.remove(&candidate);
        self.due_by_event_id.remove(&candidate.event_id);

        let result = claim_event(
            storage,
            EventClaimRequest {
                scope: self.scope.clone(),
                event_id: candidate.event_id.clone(),
                device_id: request.device_id,
                claim_token: request.claim_token,
                now: request.now,
                claim_until,
                max_attempts: request.max_attempts,
            },
        )
        .await?;
        match result {
            EventClaimResult::Acquired(record) => {
                self.retain_rescan_at(claim_until);
                Ok(SchedulerTickResult::Claimed(record))
            }
            EventClaimResult::AttemptsExhausted { disposition } => {
                Ok(SchedulerTickResult::AttemptsExhausted {
                    event_id: candidate.event_id,
                    disposition,
                })
            }
            EventClaimResult::NotAvailable => Ok(SchedulerTickResult::CandidateUnavailable {
                event_id: candidate.event_id,
            }),
            EventClaimResult::AlreadyFinished => {
                Ok(SchedulerTickResult::CandidateAlreadyFinished {
                    event_id: candidate.event_id,
                })
            }
        }
    }

    fn retain_rescan_at(&mut self, candidate: DateTime<Utc>) {
        if self
            .next_rescan_at
            .is_none_or(|current| candidate < current)
        {
            self.next_rescan_at = Some(candidate);
        }
    }
}

fn earliest(left: Option<DateTime<Utc>>, right: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
