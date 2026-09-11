// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    AgentMessage, LocalAgentEvent, LocalAgentRun, ProviderContextItem, SyncOutboxItem,
    ToolExecution,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RecordScope {
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordMetadata {
    pub id: String,
    pub scope: RecordScope,
    pub origin_device_id: String,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationRecord {
    pub metadata: RecordMetadata,
    pub title: String,
    pub project_id: Option<String>,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRecord {
    pub metadata: RecordMetadata,
    pub profile: String,
    pub status: String,
    pub state: Value,
}

/// Durable Local Agent run state. The nested protocol value is shared with
/// native IPC and the reducer; storage does not define a second state model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRunStateRecord {
    pub metadata: RecordMetadata,
    pub run: LocalAgentRun,
}

/// Durable event consumed exactly once by the Local Agent scheduler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentEventStateRecord {
    pub metadata: RecordMetadata,
    pub event: LocalAgentEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessageStateRecord {
    pub metadata: RecordMetadata,
    pub message: AgentMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderContextStateRecord {
    pub metadata: RecordMetadata,
    pub item: ProviderContextItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutionStateRecord {
    pub metadata: RecordMetadata,
    pub execution: ToolExecution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncOutboxStateRecord {
    pub metadata: RecordMetadata,
    pub item: SyncOutboxItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRecord {
    pub metadata: RecordMetadata,
    pub conversation_id: Option<String>,
    pub status: String,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRecord {
    pub metadata: RecordMetadata,
    pub name: String,
    pub root_reference: Option<String>,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginStateRecord {
    pub metadata: RecordMetadata,
    pub plugin_id: String,
    pub release: String,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaStateRecord {
    pub metadata: RecordMetadata,
    pub project_id: Option<String>,
    pub media_kind: String,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientSettingRecord {
    pub metadata: RecordMetadata,
    pub key: String,
    pub value: Value,
}

/// Clipboard metadata. Large or binary payloads remain in encrypted local
/// files; the database stores only a stable reference and integrity fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipboardRecord {
    pub metadata: RecordMetadata,
    pub mime_type: String,
    pub content_hash: String,
    pub payload_reference: Option<String>,
    pub byte_size: u64,
    pub state: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryRecordKind {
    Project,
    AgentRun,
    Continuity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoryRecord {
    pub metadata: RecordMetadata,
    pub project_id: String,
    pub kind: StoryRecordKind,
    pub status: Option<String>,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotepadRecord {
    pub metadata: RecordMetadata,
    pub project_id: Option<String>,
    pub title: String,
    pub content: String,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalHistoryRecord {
    pub metadata: RecordMetadata,
    pub project_id: Option<String>,
    pub terminal_session_id: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PutRecord<R> {
    pub record: R,
    /// `None` creates the record and fails if it already exists. A revision
    /// performs compare-and-swap so concurrent clients cannot overwrite data.
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordQuery {
    pub scope: RecordScope,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListQuery {
    pub scope: RecordScope,
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListQuery {
    pub const MAX_LIMIT: u32 = 500;

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.limit == 0 {
            return Err("limit must be greater than zero");
        }
        if self.limit > Self::MAX_LIMIT {
            return Err("limit exceeds 500");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordPage<R> {
    pub records: Vec<R>,
    pub next_cursor: Option<String>,
}
