// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_terminal_runtime::TerminalSessionMeta;
use chrono::{DateTime, Utc};

use super::runtime::{refresh_session_status, terminal_state};
use super::TerminalRuntimeState;

const LOG_MAX_ENTRIES_ENV: &str = "TASK_RUNNER_TERMINAL_LOG_MAX_ENTRIES";
const MAX_SESSIONS_ENV: &str = "TASK_RUNNER_TERMINAL_MAX_SESSIONS";
const EXITED_SESSION_RETENTION_SECONDS_ENV: &str =
    "TASK_RUNNER_TERMINAL_EXITED_SESSION_RETENTION_SECONDS";
const CLEANUP_INTERVAL_MS_ENV: &str = "TASK_RUNNER_TERMINAL_CLEANUP_INTERVAL_MS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskTerminalRetentionPolicy {
    log_max_entries: usize,
    max_sessions: usize,
    exited_session_retention: Duration,
    cleanup_interval: Duration,
}

impl TaskTerminalRetentionPolicy {
    pub fn from_managed_env() -> Result<Self, String> {
        Self::from_values(
            required_usize(LOG_MAX_ENTRIES_ENV)?,
            required_usize(MAX_SESSIONS_ENV)?,
            required_u64(EXITED_SESSION_RETENTION_SECONDS_ENV)?,
            required_u64(CLEANUP_INTERVAL_MS_ENV)?,
        )
    }

    fn from_values(
        log_max_entries: usize,
        max_sessions: usize,
        exited_session_retention_seconds: u64,
        cleanup_interval_ms: u64,
    ) -> Result<Self, String> {
        if !(100..=100_000).contains(&log_max_entries) {
            return Err(format!(
                "{LOG_MAX_ENTRIES_ENV} must be between 100 and 100000"
            ));
        }
        if !(1..=10_000).contains(&max_sessions) {
            return Err(format!("{MAX_SESSIONS_ENV} must be between 1 and 10000"));
        }
        if !(60..=2_592_000).contains(&exited_session_retention_seconds) {
            return Err(format!(
                "{EXITED_SESSION_RETENTION_SECONDS_ENV} must be between 60 and 2592000"
            ));
        }
        if !(1_000..=3_600_000).contains(&cleanup_interval_ms) {
            return Err(format!(
                "{CLEANUP_INTERVAL_MS_ENV} must be between 1000 and 3600000"
            ));
        }
        Ok(Self {
            log_max_entries,
            max_sessions,
            exited_session_retention: Duration::from_secs(exited_session_retention_seconds),
            cleanup_interval: Duration::from_millis(cleanup_interval_ms),
        })
    }

    pub(super) fn log_max_entries(&self) -> usize {
        self.log_max_entries
    }

    pub(super) fn max_sessions(&self) -> usize {
        self.max_sessions
    }

    fn expiry_cutoff(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
        let retention = chrono::Duration::from_std(self.exited_session_retention)
            .map_err(|error| format!("terminal retention duration is invalid: {error}"))?;
        Ok(now - retention)
    }

    #[cfg(test)]
    pub(super) fn test_default() -> Self {
        Self::from_values(4_000, 512, 86_400, 60_000).expect("valid terminal test policy")
    }
}

pub fn spawn_task_terminal_retention() -> tokio::task::JoinHandle<()> {
    let state = terminal_state().clone();
    tokio::spawn(async move {
        loop {
            match prune_expired_terminal_sessions(&state, Utc::now()).await {
                Ok(pruned_sessions) if pruned_sessions > 0 => {
                    tracing::info!(
                        pruned_sessions,
                        "task runner pruned expired terminal sessions and logs"
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "task runner failed to prune expired terminal sessions"
                    );
                }
            }
            tokio::time::sleep(state.policy.cleanup_interval).await;
        }
    })
}

pub(super) async fn prune_expired_terminal_sessions(
    state: &TerminalRuntimeState,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    let cutoff = state.policy.expiry_cutoff(now)?;
    let sessions = state
        .sessions
        .read()
        .await
        .iter()
        .map(|(session_id, session)| (session_id.clone(), session.clone()))
        .collect::<Vec<_>>();
    let mut expired_session_ids = Vec::new();
    for (session_id, session) in sessions {
        if let Err(error) = refresh_session_status(&session).await {
            tracing::warn!(
                terminal_id = session_id.as_str(),
                error = error.as_str(),
                "task runner failed to refresh terminal status during retention cleanup"
            );
            continue;
        }
        let meta = session.meta.lock().await;
        if terminal_session_expired(&meta, cutoff) {
            expired_session_ids.push(session_id);
        }
    }
    if expired_session_ids.is_empty() {
        return Ok(0);
    }

    let mut sessions = state.sessions.write().await;
    let mut removed = 0;
    for session_id in expired_session_ids {
        removed += usize::from(sessions.remove(session_id.as_str()).is_some());
    }
    Ok(removed)
}

fn terminal_session_expired(meta: &TerminalSessionMeta, cutoff: DateTime<Utc>) -> bool {
    if !meta.is_exited() {
        return false;
    }
    meta.finished_at
        .as_deref()
        .and_then(|finished_at| DateTime::parse_from_rfc3339(finished_at).ok())
        .is_some_and(|finished_at| finished_at.with_timezone(&Utc) <= cutoff)
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
    use std::sync::Arc;

    use tokio::process::Command;
    use tokio::sync::Mutex;

    use super::*;
    use crate::terminal_store::TerminalSession;

    #[test]
    fn terminal_retention_policy_enforces_capacity_boundaries() {
        assert!(TaskTerminalRetentionPolicy::from_values(99, 512, 86_400, 60_000).is_err());
        assert!(TaskTerminalRetentionPolicy::from_values(4_000, 0, 86_400, 60_000).is_err());
        assert!(TaskTerminalRetentionPolicy::from_values(4_000, 512, 59, 60_000).is_err());
        assert!(TaskTerminalRetentionPolicy::from_values(4_000, 512, 86_400, 999).is_err());
        assert!(TaskTerminalRetentionPolicy::from_values(4_000, 512, 86_400, 60_000).is_ok());
    }

    #[test]
    fn terminal_retention_only_expires_old_exited_sessions() {
        let now = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
            .expect("valid now")
            .with_timezone(&Utc);
        let cutoff = now - chrono::Duration::hours(24);
        let mut old_exited = session_meta("2026-08-05T11:59:59Z");
        old_exited.mark_exited(Some(0), "2026-08-05T11:59:59Z".to_string());
        let mut recent_exited = session_meta("2026-08-06T11:00:00Z");
        recent_exited.mark_exited(Some(0), "2026-08-06T11:00:00Z".to_string());
        let active = session_meta("2026-08-01T00:00:00Z");

        assert!(terminal_session_expired(&old_exited, cutoff));
        assert!(!terminal_session_expired(&recent_exited, cutoff));
        assert!(!terminal_session_expired(&active, cutoff));
    }

    #[tokio::test]
    async fn terminal_retention_removes_only_expired_exited_session_state() {
        let state = TerminalRuntimeState::new(
            TaskTerminalRetentionPolicy::from_values(4_000, 512, 86_400, 60_000)
                .expect("valid policy"),
        );
        let old_exited = stored_session("terminal-old", "2026-08-05T11:59:59Z", true);
        let active = stored_session("terminal-active", "2026-08-01T00:00:00Z", false);
        {
            let mut sessions = state.sessions.write().await;
            sessions.insert("terminal-old".to_string(), old_exited);
            sessions.insert("terminal-active".to_string(), active);
        }

        let now = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
            .expect("valid now")
            .with_timezone(&Utc);
        let removed = prune_expired_terminal_sessions(&state, now)
            .await
            .expect("prune terminal sessions");

        assert_eq!(removed, 1);
        let sessions = state.sessions.read().await;
        assert!(!sessions.contains_key("terminal-old"));
        assert!(sessions.contains_key("terminal-active"));
    }

    #[tokio::test]
    async fn terminal_retention_refreshes_naturally_exited_processes_before_pruning() {
        let state = TerminalRuntimeState::new(
            TaskTerminalRetentionPolicy::from_values(4_000, 512, 60, 60_000).expect("valid policy"),
        );
        state.sessions.write().await.insert(
            "terminal-natural-exit".to_string(),
            stored_session("terminal-natural-exit", "2026-08-06T12:00:00Z", false),
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
        let first_scan = Utc::now();

        assert_eq!(
            prune_expired_terminal_sessions(&state, first_scan)
                .await
                .expect("refresh terminal status"),
            0
        );
        let session = state.sessions.read().await["terminal-natural-exit"].clone();
        assert!(session.meta.lock().await.is_exited());
        assert_eq!(
            prune_expired_terminal_sessions(&state, first_scan + chrono::Duration::seconds(61))
                .await
                .expect("prune naturally exited terminal"),
            1
        );
    }

    fn session_meta(started_at: &str) -> TerminalSessionMeta {
        TerminalSessionMeta::new(
            "terminal-1".to_string(),
            "/workspace".to_string(),
            Some("project-1".to_string()),
            Some("user-1".to_string()),
            "echo ok".to_string(),
            started_at.to_string(),
        )
    }

    fn stored_session(id: &str, finished_at: &str, exited: bool) -> Arc<TerminalSession> {
        let mut command = Command::new("sh");
        command.arg("-c").arg("exit 0");
        let child = command.spawn().expect("spawn terminal test child");
        let mut meta = session_meta(finished_at);
        meta.id = id.to_string();
        if exited {
            meta.mark_exited(Some(0), finished_at.to_string());
        }
        Arc::new(TerminalSession {
            meta: Mutex::new(meta),
            child: Mutex::new(child),
            logs: Mutex::new(chatos_terminal_runtime::TerminalLogBuffer::new(100)),
        })
    }
}
