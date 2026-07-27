// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

const TELEMETRY_SCHEMA_VERSION: u32 = 1;
const MAX_RECENT_EVENTS: usize = 200;
const MAX_TERMINAL_SESSIONS: usize = 200;
const MAX_ERROR_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRuntimeTelemetrySnapshot {
    pub schema_version: u32,
    pub revision: u64,
    #[serde(default)]
    pub sessions: Vec<PluginRuntimeSessionTelemetry>,
    #[serde(default)]
    pub recent_events: Vec<PluginRuntimeTelemetryEvent>,
}

impl Default for PluginRuntimeTelemetrySnapshot {
    fn default() -> Self {
        Self {
            schema_version: TELEMETRY_SCHEMA_VERSION,
            revision: 0,
            sessions: Vec::new(),
            recent_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeSessionStatus {
    Ready,
    Executing,
    Degraded,
    Failed,
    Cancelled,
    Expired,
}

impl PluginRuntimeSessionStatus {
    fn terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Expired)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeTelemetryPhase {
    Prepare,
    Execute,
    Health,
    Cancel,
    Lifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeTelemetryEventStatus {
    Started,
    Succeeded,
    Failed,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRuntimeSessionTelemetry {
    pub run_id: String,
    pub adapter_session_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub status: PluginRuntimeSessionStatus,
    pub active_executions: u32,
    pub execution_count: u64,
    #[serde(default)]
    pub last_operation: Option<String>,
    #[serde(default)]
    pub last_tool_name: Option<String>,
    #[serde(default)]
    pub health_status: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    pub expires_at: i64,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRuntimeTelemetryEvent {
    pub sequence: u64,
    pub run_id: String,
    #[serde(default)]
    pub adapter_session_id: Option<String>,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub phase: PluginRuntimeTelemetryPhase,
    pub status: PluginRuntimeTelemetryEventStatus,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub health_status: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PluginRuntimeTelemetryIdentity {
    pub run_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
}

#[derive(Debug, Default)]
pub(super) struct PluginRuntimeTelemetryState {
    revision: u64,
    sessions: HashMap<String, PluginRuntimeSessionTelemetry>,
    recent_events: Vec<PluginRuntimeTelemetryEvent>,
}

impl PluginRuntimeTelemetryState {
    pub(super) fn snapshot(&self) -> PluginRuntimeTelemetrySnapshot {
        let mut sessions = self.sessions.values().cloned().collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.adapter_session_id.cmp(&right.adapter_session_id))
        });
        PluginRuntimeTelemetrySnapshot {
            schema_version: TELEMETRY_SCHEMA_VERSION,
            revision: self.revision,
            sessions,
            recent_events: self.recent_events.clone(),
        }
    }

    pub(super) fn record_prepare_started(&mut self, identity: &PluginRuntimeTelemetryIdentity) {
        self.push_event(
            identity,
            None,
            PluginRuntimeTelemetryPhase::Prepare,
            PluginRuntimeTelemetryEventStatus::Started,
            None,
            None,
            None,
            None,
            None,
        );
    }

    pub(super) fn record_prepare_succeeded(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
        expires_at: i64,
        duration_ms: u64,
        health_status: Option<&str>,
    ) {
        let now = telemetry_timestamp();
        self.sessions.insert(
            adapter_session_id.to_string(),
            PluginRuntimeSessionTelemetry {
                run_id: identity.run_id.clone(),
                adapter_session_id: adapter_session_id.to_string(),
                plugin_id: identity.plugin_id.clone(),
                release_id: identity.release_id.clone(),
                component_key: identity.component_key.clone(),
                status: session_status_for_health(health_status),
                active_executions: 0,
                execution_count: 0,
                last_operation: None,
                last_tool_name: None,
                health_status: health_status.map(str::to_string),
                started_at: now.clone(),
                updated_at: now,
                completed_at: None,
                expires_at,
                last_error: None,
            },
        );
        self.push_event(
            identity,
            Some(adapter_session_id),
            PluginRuntimeTelemetryPhase::Prepare,
            PluginRuntimeTelemetryEventStatus::Succeeded,
            None,
            None,
            Some(duration_ms),
            health_status,
            None,
        );
        self.prune_terminal_sessions();
    }

    pub(super) fn record_prepare_failed(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        duration_ms: u64,
        error: &str,
    ) {
        self.push_event(
            identity,
            None,
            PluginRuntimeTelemetryPhase::Prepare,
            PluginRuntimeTelemetryEventStatus::Failed,
            None,
            None,
            Some(duration_ms),
            None,
            Some(error),
        );
    }

    pub(super) fn record_execution_started(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
        phase: PluginRuntimeTelemetryPhase,
        operation: &str,
        tool_name: Option<&str>,
    ) {
        if let Some(session) = self.exact_session_mut(identity, adapter_session_id) {
            session.active_executions = session.active_executions.saturating_add(1);
            session.status = PluginRuntimeSessionStatus::Executing;
            session.last_operation = Some(operation.to_string());
            session.last_tool_name = tool_name.map(str::to_string);
            session.updated_at = telemetry_timestamp();
            session.last_error = None;
        }
        self.push_event(
            identity,
            Some(adapter_session_id),
            phase,
            PluginRuntimeTelemetryEventStatus::Started,
            Some(operation),
            tool_name,
            None,
            None,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_execution_finished(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
        phase: PluginRuntimeTelemetryPhase,
        operation: &str,
        tool_name: Option<&str>,
        duration_ms: u64,
        result: Result<Option<&str>, &str>,
    ) {
        let (event_status, health_status, error) = match result {
            Ok(health_status) => (
                PluginRuntimeTelemetryEventStatus::Succeeded,
                health_status,
                None,
            ),
            Err(error) => (PluginRuntimeTelemetryEventStatus::Failed, None, Some(error)),
        };
        if let Some(session) = self.exact_session_mut(identity, adapter_session_id) {
            session.active_executions = session.active_executions.saturating_sub(1);
            session.execution_count = session.execution_count.saturating_add(1);
            session.last_operation = Some(operation.to_string());
            session.last_tool_name = tool_name.map(str::to_string);
            session.updated_at = telemetry_timestamp();
            if !session.status.terminal() {
                match error {
                    Some(error) => {
                        session.status = if phase == PluginRuntimeTelemetryPhase::Health {
                            PluginRuntimeSessionStatus::Degraded
                        } else {
                            PluginRuntimeSessionStatus::Failed
                        };
                        session.last_error = Some(sanitize_error(error));
                    }
                    None => {
                        if let Some(health_status) = health_status {
                            session.health_status = Some(health_status.to_string());
                        }
                        session.status =
                            session_status_for_health(session.health_status.as_deref());
                        session.last_error = None;
                    }
                }
                if session.active_executions > 0 {
                    session.status = PluginRuntimeSessionStatus::Executing;
                }
            }
        }
        self.push_event(
            identity,
            Some(adapter_session_id),
            phase,
            event_status,
            Some(operation),
            tool_name,
            Some(duration_ms),
            health_status,
            error,
        );
    }

    pub(super) fn record_lifecycle_finished(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        operation: &str,
        duration_ms: u64,
        result: Result<(), &str>,
    ) {
        let (status, error) = match result {
            Ok(()) => (PluginRuntimeTelemetryEventStatus::Succeeded, None),
            Err(error) => (PluginRuntimeTelemetryEventStatus::Failed, Some(error)),
        };
        self.push_event(
            identity,
            None,
            PluginRuntimeTelemetryPhase::Lifecycle,
            status,
            Some(operation),
            None,
            Some(duration_ms),
            None,
            error,
        );
    }

    pub(super) fn record_cancel_started(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
    ) {
        self.push_event(
            identity,
            Some(adapter_session_id),
            PluginRuntimeTelemetryPhase::Cancel,
            PluginRuntimeTelemetryEventStatus::Started,
            None,
            None,
            None,
            None,
            None,
        );
    }

    pub(super) fn record_cancelled(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
        duration_ms: u64,
    ) {
        let now = telemetry_timestamp();
        if let Some(session) = self.exact_session_mut(identity, adapter_session_id) {
            session.status = PluginRuntimeSessionStatus::Cancelled;
            session.active_executions = 0;
            session.updated_at = now.clone();
            session.completed_at = Some(now);
            session.last_error = None;
        }
        self.push_event(
            identity,
            Some(adapter_session_id),
            PluginRuntimeTelemetryPhase::Cancel,
            PluginRuntimeTelemetryEventStatus::Cancelled,
            None,
            None,
            Some(duration_ms),
            None,
            None,
        );
        self.prune_terminal_sessions();
    }

    pub(super) fn record_expired(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
    ) {
        let now = telemetry_timestamp();
        if let Some(session) = self.exact_session_mut(identity, adapter_session_id) {
            if session.status.terminal() {
                return;
            }
            session.status = PluginRuntimeSessionStatus::Expired;
            session.active_executions = 0;
            session.updated_at = now.clone();
            session.completed_at = Some(now);
            session.last_error = None;
        }
        self.push_event(
            identity,
            Some(adapter_session_id),
            PluginRuntimeTelemetryPhase::Cancel,
            PluginRuntimeTelemetryEventStatus::Expired,
            None,
            None,
            None,
            None,
            None,
        );
        self.prune_terminal_sessions();
    }

    fn exact_session_mut(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: &str,
    ) -> Option<&mut PluginRuntimeSessionTelemetry> {
        self.sessions.get_mut(adapter_session_id).filter(|session| {
            session.run_id == identity.run_id
                && session.plugin_id == identity.plugin_id
                && session.release_id == identity.release_id
                && session.component_key == identity.component_key
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_event(
        &mut self,
        identity: &PluginRuntimeTelemetryIdentity,
        adapter_session_id: Option<&str>,
        phase: PluginRuntimeTelemetryPhase,
        status: PluginRuntimeTelemetryEventStatus,
        operation: Option<&str>,
        tool_name: Option<&str>,
        duration_ms: Option<u64>,
        health_status: Option<&str>,
        error: Option<&str>,
    ) {
        self.revision = self.revision.saturating_add(1);
        self.recent_events.push(PluginRuntimeTelemetryEvent {
            sequence: self.revision,
            run_id: identity.run_id.clone(),
            adapter_session_id: adapter_session_id.map(str::to_string),
            plugin_id: identity.plugin_id.clone(),
            release_id: identity.release_id.clone(),
            component_key: identity.component_key.clone(),
            phase,
            status,
            operation: operation.map(str::to_string),
            tool_name: tool_name.map(str::to_string),
            timestamp: telemetry_timestamp(),
            duration_ms,
            health_status: health_status.map(str::to_string),
            error: error.map(sanitize_error),
        });
        if self.recent_events.len() > MAX_RECENT_EVENTS {
            let remove = self.recent_events.len() - MAX_RECENT_EVENTS;
            self.recent_events.drain(..remove);
        }
    }

    fn prune_terminal_sessions(&mut self) {
        let mut terminal = self
            .sessions
            .values()
            .filter(|session| session.status.terminal())
            .map(|session| {
                (
                    session.updated_at.clone(),
                    session.adapter_session_id.clone(),
                )
            })
            .collect::<Vec<_>>();
        if terminal.len() <= MAX_TERMINAL_SESSIONS {
            return;
        }
        terminal.sort();
        let remove = terminal.len() - MAX_TERMINAL_SESSIONS;
        for (_, adapter_session_id) in terminal.into_iter().take(remove) {
            self.sessions.remove(adapter_session_id.as_str());
        }
    }
}

fn session_status_for_health(health_status: Option<&str>) -> PluginRuntimeSessionStatus {
    if health_status == Some("degraded") {
        PluginRuntimeSessionStatus::Degraded
    } else {
        PluginRuntimeSessionStatus::Ready
    }
}

fn telemetry_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(super) fn sanitize_error(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut sanitized = String::new();
    for token in normalized.split(' ') {
        let lower = token.to_ascii_lowercase();
        let replacement = if lower.contains("://") {
            "<redacted-url>"
        } else if lower.contains("access_token")
            || lower.contains("refresh_token")
            || lower.contains("client_secret")
            || lower.starts_with("bearer")
            || lower.starts_with("password=")
        {
            "<redacted-secret>"
        } else {
            token
        };
        let separator = usize::from(!sanitized.is_empty());
        if sanitized
            .len()
            .saturating_add(separator)
            .saturating_add(replacement.len())
            > MAX_ERROR_BYTES
        {
            break;
        }
        if separator == 1 {
            sanitized.push(' ');
        }
        sanitized.push_str(replacement);
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PluginRuntimeTelemetryIdentity {
        PluginRuntimeTelemetryIdentity {
            run_id: "run-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            component_key: "component-1".to_string(),
        }
    }

    #[test]
    fn telemetry_tracks_run_scoped_lifecycle_without_payloads() {
        let identity = identity();
        let mut state = PluginRuntimeTelemetryState::default();
        state.record_prepare_started(&identity);
        state.record_prepare_succeeded(&identity, "adapter-1", 10, 4, Some("healthy"));
        state.record_execution_started(
            &identity,
            "adapter-1",
            PluginRuntimeTelemetryPhase::Execute,
            "mcp_tools_call",
            Some("search"),
        );
        state.record_execution_finished(
            &identity,
            "adapter-1",
            PluginRuntimeTelemetryPhase::Execute,
            "mcp_tools_call",
            Some("search"),
            8,
            Ok(Some("healthy")),
        );
        state.record_cancel_started(&identity, "adapter-1");
        state.record_cancelled(&identity, "adapter-1", 2);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.revision, 6);
        assert_eq!(snapshot.sessions.len(), 1);
        let session = &snapshot.sessions[0];
        assert_eq!(session.run_id, "run-1");
        assert_eq!(session.execution_count, 1);
        assert_eq!(session.status, PluginRuntimeSessionStatus::Cancelled);
        let serialized = serde_json::to_string(&snapshot).expect("serialize telemetry");
        assert!(!serialized.contains("arguments"));
        assert!(!serialized.contains("result"));
    }

    #[test]
    fn telemetry_redacts_urls_and_secret_markers_from_bounded_errors() {
        let error = sanitize_error(
            "request https://example.test/private failed access_token=secret refresh_token=secret",
        );
        assert_eq!(
            error,
            "request <redacted-url> failed <redacted-secret> <redacted-secret>"
        );
        assert!(sanitize_error("x".repeat(2048).as_str()).len() <= MAX_ERROR_BYTES);
    }
}
