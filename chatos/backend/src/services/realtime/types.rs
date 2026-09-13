// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use std::ops::Deref;

use crate::models::memory_mapping_types::MemoryContactDto;
use crate::models::remote_connection::RemoteConnectionView;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::models::terminal::Terminal;

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairRealtimePayload {
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub running: bool,
    pub pending_message_count: Option<i64>,
    pub running_job_count: Option<i64>,
    pub scope_session_count: Option<usize>,
    pub processed_sessions: Option<usize>,
    pub summarized_sessions: Option<usize>,
    pub generated_summaries: Option<usize>,
    pub marked_messages: Option<usize>,
    pub failed_sessions: Option<usize>,
    pub job_type: Option<String>,
    pub mode: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSummariesUpdatedRealtimePayload {
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub items: Vec<SessionSummaryV2>,
    pub total: usize,
    pub has_summary: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactsUpdatedRealtimePayload {
    pub reason: String,
    pub contact_id: Option<String>,
    pub contact: Option<MemoryContactDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotepadUpdatedRealtimePayload {
    pub reason: String,
    pub note_id: Option<String>,
    pub folder: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteConnectionsUpdatedRealtimePayload {
    pub reason: String,
    pub connection_id: Option<String>,
    pub connection: Option<RemoteConnectionView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionsUpdatedRealtimePayload {
    pub reason: String,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    pub session: Option<Session>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalStateRealtimePayload {
    pub terminal_id: String,
    pub project_id: Option<String>,
    pub terminal_name: Option<String>,
    pub cwd: Option<String>,
    pub status: String,
    pub busy: bool,
    pub reason: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalListInvalidatedRealtimePayload {
    pub terminal_id: Option<String>,
    pub project_id: Option<String>,
    pub reason: String,
    pub terminal: Option<Terminal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RealtimeEventPayload {
    ReviewRepair(ReviewRepairRealtimePayload),
    ConversationSummariesUpdated(ConversationSummariesUpdatedRealtimePayload),
    ContactsUpdated(ContactsUpdatedRealtimePayload),
    NotepadUpdated(NotepadUpdatedRealtimePayload),
    RemoteConnectionsUpdated(RemoteConnectionsUpdatedRealtimePayload),
    SessionsUpdated(SessionsUpdatedRealtimePayload),
    TerminalState(TerminalStateRealtimePayload),
    TerminalListInvalidated(TerminalListInvalidatedRealtimePayload),
}

#[derive(Debug, Clone, Serialize)]
pub struct RealtimeEventEnvelope {
    #[serde(rename = "type")]
    pub message_type: &'static str,
    pub event: &'static str,
    pub user_id: String,
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
    pub payload: RealtimeEventPayload,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SequencedRealtimeEventEnvelope {
    pub event_id: String,
    pub event_sequence: u64,
    #[serde(flatten)]
    pub envelope: RealtimeEventEnvelope,
}

impl Deref for SequencedRealtimeEventEnvelope {
    type Target = RealtimeEventEnvelope;

    fn deref(&self) -> &Self::Target {
        &self.envelope
    }
}
