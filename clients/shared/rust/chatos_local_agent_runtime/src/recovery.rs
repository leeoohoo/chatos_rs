// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, ListQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{LocalAgentEventStatus, LocalAgentRunStatus};
use chrono::{DateTime, Utc};

use crate::pagination::advance_cursor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryIssue {
    OrphanedEvent {
        event_id: String,
        run_id: String,
    },
    EventForTerminalRun {
        event_id: String,
        run_id: String,
    },
    EventVersionMismatch {
        event_id: String,
        run_id: String,
        expected_version: u64,
        actual_version: u64,
    },
    StrandedRun {
        run_id: String,
        status: LocalAgentRunStatus,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryPlan {
    pub active_runs: Vec<AgentRunStateRecord>,
    pub ready_events: Vec<AgentEventStateRecord>,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub issues: Vec<RecoveryIssue>,
}

/// Builds the scheduler's startup plan from the selected authoritative
/// storage backend. The scan is linear and happens at Host startup, not once
/// per model step. Future events contribute only their earliest wake time.
pub async fn scan_recoverable_work(
    storage: &dyn ClientStorage,
    scope: RecordScope,
    now: DateTime<Utc>,
) -> StorageResult<RecoveryPlan> {
    let mut operation = RecoveryScanOperation {
        scope,
        now,
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "recovery scan completed without a plan".to_string(),
    })
}

struct RecoveryScanOperation {
    scope: RecordScope,
    now: DateTime<Utc>,
    result: Option<RecoveryPlan>,
}

#[async_trait]
impl StorageTransaction for RecoveryScanOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut runs = BTreeMap::new();
        let mut run_cursor = None;
        loop {
            let page = repositories
                .agent_runs()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: run_cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            for record in page.records {
                runs.insert(record.run.run_id.clone(), record);
            }
            if !advance_cursor(&mut run_cursor, page.next_cursor)? {
                break;
            }
        }

        let mut ready_events = Vec::new();
        let mut next_wake_at = None;
        let mut issues = Vec::new();
        let mut runs_with_live_events = BTreeSet::new();
        let mut event_cursor = None;
        loop {
            let page = repositories
                .agent_events()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: event_cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            for record in page.records {
                if matches!(
                    record.event.status,
                    LocalAgentEventStatus::Applied | LocalAgentEventStatus::Failed
                ) {
                    continue;
                }
                let Some(run_record) = runs.get(&record.event.run_id) else {
                    issues.push(RecoveryIssue::OrphanedEvent {
                        event_id: record.event.event_id.clone(),
                        run_id: record.event.run_id.clone(),
                    });
                    continue;
                };
                if run_record.run.status.is_terminal() {
                    issues.push(RecoveryIssue::EventForTerminalRun {
                        event_id: record.event.event_id.clone(),
                        run_id: record.event.run_id.clone(),
                    });
                    continue;
                }
                if record.event.expected_version != run_record.run.version {
                    issues.push(RecoveryIssue::EventVersionMismatch {
                        event_id: record.event.event_id.clone(),
                        run_id: record.event.run_id.clone(),
                        expected_version: record.event.expected_version,
                        actual_version: run_record.run.version,
                    });
                    continue;
                }
                runs_with_live_events.insert(record.event.run_id.clone());
                match record.event.status {
                    LocalAgentEventStatus::Pending => {
                        if record.event.available_at <= self.now {
                            ready_events.push(record);
                        } else {
                            retain_earliest(&mut next_wake_at, record.event.available_at);
                        }
                    }
                    LocalAgentEventStatus::Claimed => {
                        let claim_until =
                            record
                                .event
                                .claim_until
                                .ok_or_else(|| StorageError::InvalidData {
                                    reason: format!(
                                        "claimed event {} has no claim deadline",
                                        record.event.event_id
                                    ),
                                })?;
                        if claim_until <= self.now {
                            ready_events.push(record);
                        } else {
                            retain_earliest(&mut next_wake_at, claim_until);
                        }
                    }
                    LocalAgentEventStatus::Applied | LocalAgentEventStatus::Failed => {}
                }
            }
            if !advance_cursor(&mut event_cursor, page.next_cursor)? {
                break;
            }
        }

        let active_runs = runs
            .into_values()
            .filter(|record| !record.run.status.is_terminal())
            .collect::<Vec<_>>();
        for run in &active_runs {
            if run_requires_event(run.run.status)
                && !runs_with_live_events.contains(run.run.run_id.as_str())
            {
                issues.push(RecoveryIssue::StrandedRun {
                    run_id: run.run.run_id.clone(),
                    status: run.run.status,
                });
            }
        }
        ready_events.sort_by(|left, right| {
            left.event
                .available_at
                .cmp(&right.event.available_at)
                .then_with(|| left.event.event_id.cmp(&right.event.event_id))
        });
        issues.sort_by(|left, right| issue_key(left).cmp(&issue_key(right)));
        self.result = Some(RecoveryPlan {
            active_runs,
            ready_events,
            next_wake_at,
            issues,
        });
        Ok(())
    }
}

fn retain_earliest(target: &mut Option<DateTime<Utc>>, candidate: DateTime<Utc>) {
    if target.is_none_or(|current| candidate < current) {
        *target = Some(candidate);
    }
}

const fn run_requires_event(status: LocalAgentRunStatus) -> bool {
    matches!(
        status,
        LocalAgentRunStatus::Queued
            | LocalAgentRunStatus::ModelReady
            | LocalAgentRunStatus::ModelRunning
            | LocalAgentRunStatus::WaitingToolResult
            | LocalAgentRunStatus::ContinuationReady
            | LocalAgentRunStatus::RetryScheduled
    )
}

fn issue_key(issue: &RecoveryIssue) -> (&str, &str) {
    match issue {
        RecoveryIssue::OrphanedEvent { event_id, run_id }
        | RecoveryIssue::EventForTerminalRun { event_id, run_id }
        | RecoveryIssue::EventVersionMismatch {
            event_id, run_id, ..
        } => (run_id, event_id),
        RecoveryIssue::StrandedRun { run_id, .. } => (run_id, ""),
    }
}
