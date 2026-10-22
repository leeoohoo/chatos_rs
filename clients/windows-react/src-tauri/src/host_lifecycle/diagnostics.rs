use serde::Serialize;
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const DIAGNOSTIC_CAPACITY: usize = 64;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiagnosticEntry {
    sequence: u64,
    timestamp_unix_ms: u64,
    event: &'static str,
    outcome: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiagnosticsSnapshot {
    capacity: usize,
    entries: Vec<HostDiagnosticEntry>,
}

#[derive(Default)]
struct HostDiagnosticsState {
    next_sequence: u64,
    entries: VecDeque<HostDiagnosticEntry>,
}

#[derive(Default)]
pub(super) struct HostDiagnostics {
    state: Mutex<HostDiagnosticsState>,
}

impl HostDiagnostics {
    pub(super) async fn record(&self, event: &'static str, outcome: &'static str) {
        let mut state = self.state.lock().await;
        state.next_sequence = state.next_sequence.saturating_add(1);
        let sequence = state.next_sequence;
        if state.entries.len() == DIAGNOSTIC_CAPACITY {
            state.entries.pop_front();
        }
        state.entries.push_back(HostDiagnosticEntry {
            sequence,
            timestamp_unix_ms: unix_timestamp_milliseconds(),
            event,
            outcome,
        });
    }

    pub(super) async fn snapshot(&self) -> HostDiagnosticsSnapshot {
        let state = self.state.lock().await;
        HostDiagnosticsSnapshot {
            capacity: DIAGNOSTIC_CAPACITY,
            entries: state.entries.iter().cloned().collect(),
        }
    }
}

pub(super) fn classify_error(error: &str) -> &'static str {
    if error.contains("timed out") {
        "timeout"
    } else if error.contains("response frame") || error.contains("early eof") {
        "invalid_transport"
    } else if error.contains("protocol v46") || error.contains("protocol_version") {
        "protocol_mismatch"
    } else {
        "failed"
    }
}

fn unix_timestamp_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn diagnostics_are_bounded_and_monotonic() {
        let diagnostics = HostDiagnostics::default();
        for _ in 0..(DIAGNOSTIC_CAPACITY + 3) {
            diagnostics.record("test", "ok").await;
        }

        let snapshot = diagnostics.snapshot().await;
        assert_eq!(snapshot.entries.len(), DIAGNOSTIC_CAPACITY);
        assert_eq!(
            snapshot.entries.first().map(|entry| entry.sequence),
            Some(4)
        );
        assert_eq!(
            snapshot.entries.last().map(|entry| entry.sequence),
            Some((DIAGNOSTIC_CAPACITY + 3) as u64),
        );
    }

    #[test]
    fn diagnostic_errors_are_reduced_to_safe_categories() {
        assert_eq!(classify_error("Host health check timed out."), "timeout");
        assert_eq!(classify_error("token=secret-value"), "failed");
    }
}
