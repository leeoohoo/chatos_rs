use serde::Serialize;
use serde_json::Value;

use crate::models::memory_mapping_types::MemoryContactDto;
use crate::models::project::Project;
use crate::models::remote_connection::RemoteConnection;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::models::terminal::Terminal;
use crate::services::task_manager::{TaskDraft, TaskRecord};

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairRealtimePayload {
    pub conversation_id: String,
    pub project_id: String,
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
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub items: Vec<SessionSummaryV2>,
    pub total: usize,
    pub has_summary: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectChangeSummaryRealtimePayload {
    pub project_id: String,
    pub reason: String,
    pub conversation_id: Option<String>,
    pub path: Option<String>,
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
pub struct ProjectsUpdatedRealtimePayload {
    pub reason: String,
    pub project_id: Option<String>,
    pub project: Option<Project>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteConnectionsUpdatedRealtimePayload {
    pub reason: String,
    pub connection_id: Option<String>,
    pub connection: Option<RemoteConnection>,
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
pub struct ProjectRunStateRealtimePayload {
    pub project_id: String,
    pub terminal_id: Option<String>,
    pub terminal_name: Option<String>,
    pub cwd: Option<String>,
    pub status: String,
    pub busy: bool,
    pub running: bool,
    pub reason: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRunInstanceRealtimePayload {
    pub project_id: String,
    pub terminal_id: String,
    pub terminal_name: String,
    pub cwd: String,
    pub status: String,
    pub busy: bool,
    pub running: bool,
    pub reason: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRunCatalogRealtimePayload {
    pub project_id: String,
    pub reason: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectMembersUpdatedRealtimePayload {
    pub project_id: String,
    pub reason: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskBoardRealtimePayload {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub review_id: Option<String>,
    pub task_id: Option<String>,
    pub action: String,
    pub task: Option<TaskRecord>,
    pub draft_tasks: Option<Vec<TaskDraft>>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiPromptRealtimePayload {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub prompt_id: String,
    pub action: String,
    pub status: Option<String>,
    pub tool_call_id: Option<String>,
    pub prompt_kind: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub allow_cancel: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamRealtimePayload {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub project_id: Option<String>,
    pub user_message_id: Option<String>,
    pub stream_type: String,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteSftpTransferRealtimePayload {
    pub id: String,
    pub connection_id: String,
    pub direction: String,
    pub state: String,
    pub total_bytes: Option<u64>,
    pub transferred_bytes: u64,
    pub percent: Option<f64>,
    pub current_path: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RealtimeEventPayload {
    ReviewRepair(ReviewRepairRealtimePayload),
    ConversationSummariesUpdated(ConversationSummariesUpdatedRealtimePayload),
    ProjectChangeSummary(ProjectChangeSummaryRealtimePayload),
    ContactsUpdated(ContactsUpdatedRealtimePayload),
    NotepadUpdated(NotepadUpdatedRealtimePayload),
    ProjectsUpdated(ProjectsUpdatedRealtimePayload),
    RemoteConnectionsUpdated(RemoteConnectionsUpdatedRealtimePayload),
    SessionsUpdated(SessionsUpdatedRealtimePayload),
    TerminalState(TerminalStateRealtimePayload),
    TerminalListInvalidated(TerminalListInvalidatedRealtimePayload),
    ProjectRunState(ProjectRunStateRealtimePayload),
    ProjectRunInstance(ProjectRunInstanceRealtimePayload),
    ProjectRunCatalog(ProjectRunCatalogRealtimePayload),
    ProjectMembersUpdated(ProjectMembersUpdatedRealtimePayload),
    TaskBoard(TaskBoardRealtimePayload),
    UiPrompt(UiPromptRealtimePayload),
    ChatStream(ChatStreamRealtimePayload),
    RemoteSftpTransfer(RemoteSftpTransferRealtimePayload),
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
