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
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                aggregate_runtime_invocation_stats(collection, DateTime::now(), self.quota.limits())
                    .await
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
    collection: &Collection<RuntimeInvocationRecord>,
    now: DateTime,
    quota_limits: RuntimeInvocationQuotaLimits,
) -> Result<RuntimeInvocationStoreStats, String> {
    let terminal_statuses = vec![
        RuntimeInvocationStatus::Completed.as_str(),
        RuntimeInvocationStatus::Failed.as_str(),
        RuntimeInvocationStatus::Cancelled.as_str(),
        RuntimeInvocationStatus::UnknownExecutionState.as_str(),
    ];
    let active_statuses = active_runtime_invocation_statuses()
        .iter()
        .map(|status| status.as_str())
        .collect::<Vec<_>>();
    let mut cursor = collection
        .aggregate(
            vec![
                doc! { "$match": { "expires_at": { "$gt": now } } },
                doc! {
                    "$group": {
                        "_id": bson::Bson::Null,
                        "total_active": {
                            "$sum": { "$cond": [
                                { "$in": ["$status", active_statuses] },
                                1,
                                0,
                            ] }
                        },
                        "queued": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::Queued.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "running": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::Running.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "waiting_for_user": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::WaitingForUser.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "cancel_requested": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::CancelRequested.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "terminal": {
                            "$sum": { "$cond": [
                                { "$in": ["$status", terminal_statuses] },
                                1,
                                0,
                            ] }
                        },
                        "duration_completed_count": {
                            "$sum": { "$cond": [
                                { "$eq": [{ "$type": "$completed_at_unix_ms" }, "long"] },
                                1,
                                0,
                            ] }
                        },
                        "duration_total_ms": {
                            "$sum": { "$cond": [
                                { "$eq": [{ "$type": "$completed_at_unix_ms" }, "long"] },
                                { "$max": [
                                    { "$subtract": [
                                        "$completed_at_unix_ms",
                                        { "$ifNull": ["$started_at_unix_ms", "$created_at_unix_ms"] },
                                    ] },
                                    0,
                                ] },
                                0,
                            ] }
                        },
                        "duration_max_ms": {
                            "$max": { "$cond": [
                                { "$eq": [{ "$type": "$completed_at_unix_ms" }, "long"] },
                                { "$max": [
                                    { "$subtract": [
                                        "$completed_at_unix_ms",
                                        { "$ifNull": ["$started_at_unix_ms", "$created_at_unix_ms"] },
                                    ] },
                                    0,
                                ] },
                                0,
                            ] }
                        },
                        "file_modification_total": {
                            "$sum": { "$cond": [
                                { "$in": ["$file_modification_outcome", [
                                    "changed",
                                    "already_applied",
                                    "stale",
                                    "stale_context",
                                    "expected_match",
                                    "validation",
                                    "infrastructure",
                                ]] },
                                1,
                                0,
                            ] }
                        },
                        "file_modification_changed": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "changed"] }, 1, 0
                            ] }
                        },
                        "file_modification_already_applied": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "already_applied"] }, 1, 0
                            ] }
                        },
                        "file_modification_stale_context": {
                            "$sum": { "$cond": [
                                { "$in": ["$file_modification_outcome", ["stale", "stale_context"]] }, 1, 0
                            ] }
                        },
                        "file_modification_expected_match": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "expected_match"] }, 1, 0
                            ] }
                        },
                        "file_modification_validation": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "validation"] }, 1, 0
                            ] }
                        },
                        "file_modification_infrastructure": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "infrastructure"] }, 1, 0
                            ] }
                        },
                    }
                },
            ],
            None,
        )
        .await
        .map_err(|error| format!("aggregate Runtime Invocation stats failed: {error}"))?;
    let Some(document) = cursor
        .try_next()
        .await
        .map_err(|error| format!("read Runtime Invocation stats failed: {error}"))?
    else {
        return Ok(RuntimeInvocationStoreStats {
            backend: "mongo",
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
        });
    };
    Ok(RuntimeInvocationStoreStats {
        backend: "mongo",
        quota_limits,
        total_active: runtime_stat_count(&document, "total_active"),
        queued: runtime_stat_count(&document, "queued"),
        running: runtime_stat_count(&document, "running"),
        waiting_for_user: runtime_stat_count(&document, "waiting_for_user"),
        cancel_requested: runtime_stat_count(&document, "cancel_requested"),
        terminal: runtime_stat_count(&document, "terminal"),
        registration: RuntimeInvocationRegistrationStats::default(),
        session_closed_reclaimed_total: 0,
        quota_release_failures_total: 0,
        store_recoveries_total: 0,
        duration: RuntimeInvocationDurationStats {
            completed_count: runtime_stat_count(&document, "duration_completed_count"),
            total_ms: runtime_stat_u64(&document, "duration_total_ms"),
            max_ms: runtime_stat_u64(&document, "duration_max_ms"),
        },
        file_modifications: FileModificationOutcomeStats {
            total: runtime_stat_count(&document, "file_modification_total"),
            changed: runtime_stat_count(&document, "file_modification_changed"),
            already_applied: runtime_stat_count(&document, "file_modification_already_applied"),
            stale_context: runtime_stat_count(&document, "file_modification_stale_context"),
            expected_match: runtime_stat_count(&document, "file_modification_expected_match"),
            validation: runtime_stat_count(&document, "file_modification_validation"),
            infrastructure: runtime_stat_count(&document, "file_modification_infrastructure"),
        },
    })
}

fn runtime_stat_count(document: &mongodb::bson::Document, key: &str) -> usize {
    let value = match document.get(key) {
        Some(bson::Bson::Int32(value)) => i64::from(*value),
        Some(bson::Bson::Int64(value)) => *value,
        Some(bson::Bson::Double(value)) if value.is_finite() => *value as i64,
        _ => 0,
    };
    usize::try_from(value.max(0)).unwrap_or(usize::MAX)
}

fn runtime_stat_u64(document: &mongodb::bson::Document, key: &str) -> u64 {
    runtime_stat_count(document, key) as u64
}
