// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RuntimeInvocationStore {
    pub async fn stats(&self) -> Result<RuntimeInvocationStoreStats, String> {
        let now = chrono::Utc::now().timestamp();
        let mut stats = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                Ok(summarize_runtime_invocations(
                    "memory",
                    self.quota.limits(),
                    invocations.values().map(|record| {
                        (
                            record.status,
                            record.file_modification_outcome,
                            record
                                .started_at_unix_ms
                                .or(Some(record.created_at_unix_ms)),
                            record.completed_at_unix_ms,
                        )
                    }),
                ))
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                aggregate_runtime_invocation_stats(pool, self.quota.limits()).await
            }
        }?;
        stats.registration = self.diagnostics.registration_stats();
        stats.session_closed_reclaimed_total = self
            .diagnostics
            .session_closed_reclaimed
            .load(Ordering::Relaxed);
        stats.quota_release_failures_total = self
            .diagnostics
            .quota_release_failures
            .load(Ordering::Relaxed);
        stats.store_recoveries_total = self.diagnostics.store_recoveries.load(Ordering::Relaxed);
        Ok(stats)
    }
}

pub(super) fn summarize_runtime_invocations(
    backend: &'static str,
    quota_limits: RuntimeInvocationQuotaLimits,
    records: impl IntoIterator<
        Item = (
            RuntimeInvocationStatus,
            Option<FileModificationOutcome>,
            Option<i64>,
            Option<i64>,
        ),
    >,
) -> RuntimeInvocationStoreStats {
    let mut stats = RuntimeInvocationStoreStats {
        backend,
        quota_limits,
        total_active: 0,
        queued: 0,
        running: 0,
        waiting_for_user: 0,
        cancel_requested: 0,
        terminal: 0,
        registration: RuntimeInvocationRegistrationStats::default(),
        session_closed_reclaimed_total: 0,
        quota_release_failures_total: 0,
        store_recoveries_total: 0,
        duration: RuntimeInvocationDurationStats::default(),
        file_modifications: FileModificationOutcomeStats::default(),
    };
    for (status, file_modification_outcome, started_at, completed_at) in records {
        match status {
            RuntimeInvocationStatus::Queued => {
                stats.queued = stats.queued.saturating_add(1);
                stats.total_active = stats.total_active.saturating_add(1);
            }
            RuntimeInvocationStatus::Running => {
                stats.running = stats.running.saturating_add(1);
                stats.total_active = stats.total_active.saturating_add(1);
            }
            RuntimeInvocationStatus::WaitingForUser => {
                stats.waiting_for_user = stats.waiting_for_user.saturating_add(1);
                stats.total_active = stats.total_active.saturating_add(1);
            }
            RuntimeInvocationStatus::CancelRequested => {
                stats.cancel_requested = stats.cancel_requested.saturating_add(1);
                stats.total_active = stats.total_active.saturating_add(1);
            }
            RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState => {
                stats.terminal = stats.terminal.saturating_add(1)
            }
        }
        if let (Some(started_at), Some(completed_at)) = (started_at, completed_at) {
            let duration_ms = completed_at.saturating_sub(started_at).max(0) as u64;
            stats.duration.completed_count = stats.duration.completed_count.saturating_add(1);
            stats.duration.total_ms = stats.duration.total_ms.saturating_add(duration_ms);
            stats.duration.max_ms = stats.duration.max_ms.max(duration_ms);
        }
        if let Some(outcome) = file_modification_outcome {
            stats.file_modifications.record(outcome);
        }
    }
    stats
}

impl FileModificationOutcomeStats {
    fn record(&mut self, outcome: FileModificationOutcome) {
        self.total = self.total.saturating_add(1);
        let counter = match outcome {
            FileModificationOutcome::Changed => &mut self.changed,
            FileModificationOutcome::AlreadyApplied => &mut self.already_applied,
            FileModificationOutcome::StaleContext => &mut self.stale_context,
            FileModificationOutcome::ExpectedMatch => &mut self.expected_match,
            FileModificationOutcome::Validation => &mut self.validation,
            FileModificationOutcome::Infrastructure => &mut self.infrastructure,
        };
        *counter = counter.saturating_add(1);
    }
}

pub(super) async fn aggregate_runtime_invocation_stats(
    pool: &chatos_postgres::PgPool,
    quota_limits: RuntimeInvocationQuotaLimits,
) -> Result<RuntimeInvocationStoreStats, String> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
        "SELECT \
         COUNT(*) FILTER (WHERE status IN ('queued','running','waiting_for_user','cancel_requested')), \
         COUNT(*) FILTER (WHERE status='queued'), \
         COUNT(*) FILTER (WHERE status='running'), \
         COUNT(*) FILTER (WHERE status='waiting_for_user'), \
         COUNT(*) FILTER (WHERE status='cancel_requested'), \
         COUNT(*) FILTER (WHERE status IN ('completed','failed','cancelled','unknown_execution_state')), \
         COUNT(completed_at_unix_ms), \
         COALESCE(SUM(GREATEST(completed_at_unix_ms-COALESCE(started_at_unix_ms,created_at_unix_ms),0)) \
             FILTER (WHERE completed_at_unix_ms IS NOT NULL),0)::BIGINT, \
         COALESCE(MAX(GREATEST(completed_at_unix_ms-COALESCE(started_at_unix_ms,created_at_unix_ms),0)) \
             FILTER (WHERE completed_at_unix_ms IS NOT NULL),0)::BIGINT, \
         COUNT(file_modification_outcome), \
         COUNT(*) FILTER (WHERE file_modification_outcome='changed'), \
         COUNT(*) FILTER (WHERE file_modification_outcome='already_applied'), \
         COUNT(*) FILTER (WHERE file_modification_outcome='stale_context'), \
         COUNT(*) FILTER (WHERE file_modification_outcome='expected_match'), \
         COUNT(*) FILTER (WHERE file_modification_outcome='validation'), \
         COUNT(*) FILTER (WHERE file_modification_outcome='infrastructure') \
         FROM mcp_management_runtime_invocations WHERE expires_at>now()",
    )
        .fetch_one(pool)
        .await
        .map_err(|error| format!("aggregate Runtime Invocation stats failed: {error}"))?;
    Ok(RuntimeInvocationStoreStats {
        backend: "postgresql",
        quota_limits,
        total_active: runtime_stat_count(row.0),
        queued: runtime_stat_count(row.1),
        running: runtime_stat_count(row.2),
        waiting_for_user: runtime_stat_count(row.3),
        cancel_requested: runtime_stat_count(row.4),
        terminal: runtime_stat_count(row.5),
        registration: RuntimeInvocationRegistrationStats::default(),
        session_closed_reclaimed_total: 0,
        quota_release_failures_total: 0,
        store_recoveries_total: 0,
        duration: RuntimeInvocationDurationStats {
            completed_count: runtime_stat_count(row.6),
            total_ms: runtime_stat_u64(row.7),
            max_ms: runtime_stat_u64(row.8),
        },
        file_modifications: FileModificationOutcomeStats {
            total: runtime_stat_count(row.9),
            changed: runtime_stat_count(row.10),
            already_applied: runtime_stat_count(row.11),
            stale_context: runtime_stat_count(row.12),
            expected_match: runtime_stat_count(row.13),
            validation: runtime_stat_count(row.14),
            infrastructure: runtime_stat_count(row.15),
        },
    })
}

fn runtime_stat_count(value: i64) -> usize {
    usize::try_from(value.max(0)).unwrap_or(usize::MAX)
}

fn runtime_stat_u64(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(u64::MAX)
}
