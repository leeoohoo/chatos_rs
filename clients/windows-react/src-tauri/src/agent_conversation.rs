use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, GetConversationCommand,
    GetConversationRuntimeSettingsCommand, GetLatestCapabilityPolicySnapshotCommand, GetRunCommand,
    GuideConversationTurnCommand, HostCommand, HostResult, ListLatestModelConfigSnapshotsCommand,
    LocalAgentRunStatus, LocalConversationAttachmentSpec, LocalConversationDetail,
    LocalConversationMessageRole, LocalConversationResourceBinding, LocalConversationResourceKind,
    LocalConversationRuntimeSettings, LocalConversationTurnRecord, LocalConversationTurnStatus,
    LocalModelConfigSnapshot, PutConversationRuntimeSettingsCommand, ResumeConversationTurnCommand,
    StartConversationTurnCommand,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, State};

use crate::{
    agent_skill_management::{AgentProfessionRuntimeSnapshot, AgentSkillStore},
    attachment_tool_worker::AttachmentToolWorker,
    authentication::AuthenticationManager,
    conversation_attachments::{AttachmentManager, AttachmentScope},
    host_lifecycle::{validate_owner, HostManager},
    terminal_tool_worker::TerminalToolWorker,
};

pub(crate) mod documents;
pub(crate) mod heartbeat;
pub(crate) mod team_todo;

const DATABASE_FILENAME: &str = "AgentGroupChat.sqlite3";
const MAIN_CHAT_PROFILE: &str = "main_chat";
const MAX_ITERATIONS: u32 = 32;
const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS project_agent_rooms (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    goal TEXT NOT NULL DEFAULT '',
    default_agent_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    conversation_kind TEXT NOT NULL
        CHECK(conversation_kind IN ('project_team', 'human_agent_direct', 'agent_agent_direct')),
    direct_key TEXT,
    project_manager_agent_id TEXT,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, direct_key),
    FOREIGN KEY(owner_user_id, default_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS project_agent_room_members (
    owner_user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    role TEXT NOT NULL,
    responsibility TEXT NOT NULL DEFAULT '',
    plugin_allowlist_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL CHECK(status IN ('active', 'removed')),
    joined_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, room_id, agent_id),
    FOREIGN KEY(owner_user_id, room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS project_agent_messages (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender_kind TEXT NOT NULL CHECK(sender_kind IN ('human', 'agent', 'system')),
    sender_id TEXT NOT NULL,
    content TEXT NOT NULL,
    reply_to_message_id TEXT,
    source_run_id TEXT,
    causation_id TEXT,
    root_message_id TEXT NOT NULL,
    hop_count INTEGER NOT NULL CHECK(hop_count >= 0),
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id),
    FOREIGN KEY(owner_user_id, room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, reply_to_message_id)
        REFERENCES project_agent_messages(owner_user_id, id),
    FOREIGN KEY(owner_user_id, root_message_id)
        REFERENCES project_agent_messages(owner_user_id, id)
        DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX IF NOT EXISTS project_agent_messages_room_order
    ON project_agent_messages(owner_user_id, room_id, created_at_unix_ms, id);
CREATE TABLE IF NOT EXISTS project_agent_message_attachments (
    owner_user_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes > 0),
    kind TEXT NOT NULL CHECK(kind IN ('image', 'file', 'audio')),
    origin TEXT NOT NULL CHECK(origin IN ('file', 'pastedImage', 'pastedDocument', 'pastedText')),
    relative_path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'local_only'
        CHECK(sync_status IN ('local_only', 'queued', 'uploading', 'synced', 'failed')),
    source_document_ref TEXT,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, message_id, position),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS project_agent_message_mentions (
    owner_user_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    PRIMARY KEY(owner_user_id, message_id, agent_id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS project_agent_read_cursors (
    owner_user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    message_created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, room_id, agent_id),
    FOREIGN KEY(owner_user_id, room_id, agent_id)
        REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS windows_agent_chat_message_references (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    message_ref TEXT NOT NULL,
    room_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, run_id, message_ref),
    UNIQUE(owner_user_id, run_id, message_id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS windows_agent_chat_attachment_references (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    attachment_ref TEXT NOT NULL,
    room_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    attachment_id TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, run_id, attachment_ref),
    UNIQUE(owner_user_id, run_id, message_id, attachment_id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, attachment_id)
        REFERENCES project_agent_message_attachments(owner_user_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS windows_agent_document_drafts (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    request_key TEXT NOT NULL,
    document_ref TEXT NOT NULL,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes > 0),
    sha256 TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('available', 'consumed')),
    created_at_unix_ms INTEGER NOT NULL,
    consumed_at_unix_ms INTEGER,
    PRIMARY KEY(owner_user_id, run_id, document_ref),
    UNIQUE(owner_user_id, run_id, request_key)
);
CREATE TABLE IF NOT EXISTS windows_agent_chat_read_receipts (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    request_key TEXT NOT NULL,
    requested_limit INTEGER NOT NULL CHECK(requested_limit BETWEEN 1 AND 500),
    output_json TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, run_id, request_key)
);
CREATE TABLE IF NOT EXISTS project_agent_deliveries (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    root_message_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    trigger_kind TEXT NOT NULL
        CHECK(trigger_kind IN ('mention', 'default_agent', 'agent_mention', 'heartbeat', 'todo', 'todo_status')),
    status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    attempt INTEGER NOT NULL CHECK(attempt >= 0),
    hop_count INTEGER NOT NULL CHECK(hop_count >= 0),
    deduplication_key TEXT NOT NULL,
    response_message_id TEXT,
    last_error TEXT,
    claimed_at_unix_ms INTEGER,
    completed_at_unix_ms INTEGER,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, deduplication_key),
    FOREIGN KEY(owner_user_id, room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id),
    FOREIGN KEY(owner_user_id, root_message_id)
        REFERENCES project_agent_messages(owner_user_id, id),
    FOREIGN KEY(owner_user_id, room_id, target_agent_id)
        REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
    FOREIGN KEY(owner_user_id, response_message_id)
        REFERENCES project_agent_messages(owner_user_id, id)
);
CREATE INDEX IF NOT EXISTS project_agent_deliveries_agent_queue
    ON project_agent_deliveries(owner_user_id, target_agent_id, status, created_at_unix_ms, id);
CREATE UNIQUE INDEX IF NOT EXISTS one_running_manager_delivery_per_agent
    ON project_agent_deliveries(owner_user_id, target_agent_id)
    WHERE status = 'running' AND trigger_kind != 'todo';
CREATE UNIQUE INDEX IF NOT EXISTS one_running_executor_delivery_per_agent
    ON project_agent_deliveries(owner_user_id, target_agent_id)
    WHERE status = 'running' AND trigger_kind = 'todo';
CREATE TABLE IF NOT EXISTS local_agent_team_assets (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    team_room_id TEXT NOT NULL,
    category TEXT NOT NULL CHECK(category IN (
        'overview', 'current_progress', 'tech_stack', 'architecture',
        'conventions', 'decision', 'reference'
    )),
    title TEXT NOT NULL,
    markdown TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_by_agent_id TEXT,
    updated_by_agent_id TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id),
    FOREIGN KEY(owner_user_id, team_room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, created_by_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id),
    FOREIGN KEY(owner_user_id, updated_by_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE INDEX IF NOT EXISTS local_agent_team_assets_team
    ON local_agent_team_assets(
        owner_user_id, team_room_id, status, category, updated_at_unix_ms
    );
CREATE TABLE IF NOT EXISTS local_agent_team_asset_revisions (
    owner_user_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    title TEXT NOT NULL,
    markdown TEXT NOT NULL,
    editor_agent_id TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, asset_id, revision),
    FOREIGN KEY(owner_user_id, asset_id)
        REFERENCES local_agent_team_assets(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, editor_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS local_agent_todos (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    team_room_id TEXT NOT NULL,
    source_room_id TEXT,
    source_message_id TEXT,
    request_key TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL,
    priority INTEGER NOT NULL CHECK(priority BETWEEN 0 AND 100),
    sort_order INTEGER NOT NULL CHECK(sort_order >= 0),
    status TEXT NOT NULL CHECK(status IN (
        'pending', 'in_progress', 'blocked', 'completed', 'cancelled'
    )),
    blocked_reason TEXT NOT NULL,
    result TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    execution_plan_json TEXT NOT NULL,
    execution_contract_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, agent_id, request_key),
    FOREIGN KEY(owner_user_id, agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id),
    FOREIGN KEY(owner_user_id, team_room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, source_room_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, source_message_id)
        REFERENCES project_agent_messages(owner_user_id, id)
);
CREATE INDEX IF NOT EXISTS local_agent_todos_agent_queue
    ON local_agent_todos(owner_user_id, agent_id, status, priority DESC, sort_order);
CREATE INDEX IF NOT EXISTS local_agent_todos_team_queue
    ON local_agent_todos(owner_user_id, team_room_id, status, priority DESC, sort_order);
CREATE TABLE IF NOT EXISTS local_agent_todo_sources (
    owner_user_id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    relation TEXT NOT NULL CHECK(relation IN (
        'created', 'updated', 'reprioritized', 'blocked_context'
    )),
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, todo_id, conversation_id, message_id, relation),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, conversation_id)
        REFERENCES project_agent_rooms(owner_user_id, id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS local_agent_todo_dependencies (
    owner_user_id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    prerequisite_todo_id TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, todo_id, prerequisite_todo_id),
    CHECK(todo_id != prerequisite_todo_id),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, prerequisite_todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS local_agent_todo_dependencies_prerequisite
    ON local_agent_todo_dependencies(owner_user_id, prerequisite_todo_id, todo_id);
CREATE TABLE IF NOT EXISTS local_agent_todo_events (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    run_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN (
        'started', 'progress', 'blocked', 'completed', 'cancelled'
    )),
    stage TEXT NOT NULL,
    detail TEXT NOT NULL,
    asset_update_suggestions_json TEXT NOT NULL DEFAULT '[]',
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, todo_id, sequence),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS local_agent_todo_asset_snapshots (
    owner_user_id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    team_room_id TEXT NOT NULL,
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    markdown TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    captured_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, todo_id, asset_id),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, asset_id)
        REFERENCES local_agent_team_assets(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS windows_agent_delivery_runs (
    owner_user_id TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('ready', 'running', 'succeeded', 'failed', 'cancelled')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, delivery_id),
    UNIQUE(owner_user_id, turn_id),
    UNIQUE(owner_user_id, run_id),
    FOREIGN KEY(owner_user_id, delivery_id)
        REFERENCES project_agent_deliveries(owner_user_id, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS windows_agent_delivery_runs_recent
    ON windows_agent_delivery_runs(owner_user_id, updated_at_unix_ms DESC, run_id DESC);
CREATE TABLE IF NOT EXISTS windows_agent_run_control_journal (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('cancel', 'retry')),
    team_room_id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'completed')),
    host_status TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    completed_at_unix_ms INTEGER,
    PRIMARY KEY(owner_user_id, id),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, delivery_id)
        REFERENCES project_agent_deliveries(owner_user_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS one_pending_windows_agent_run_control
    ON windows_agent_run_control_journal(owner_user_id, todo_id, action)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS windows_agent_run_control_pending
    ON windows_agent_run_control_journal(owner_user_id, status, created_at_unix_ms, id);
"#;

#[derive(Clone)]
pub(crate) struct AgentConversationStore {
    database: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirectRoom {
    id: String,
    agent_id: String,
    name: String,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AgentExecutionBinding {
    room_id: String,
    agent_id: String,
    name: String,
    role_prompt: String,
    model_config_id: String,
    thinking_level: Option<String>,
    profession_key: String,
    default_plugin_ids: Vec<String>,
    default_skill_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TeamExecutionBinding {
    agent: AgentExecutionBinding,
    project_id: String,
    team_name: String,
    team_goal: String,
    team_role: String,
    team_responsibility: String,
    is_project_manager: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingTeamDelivery {
    delivery_id: String,
    message_id: String,
    content: String,
    trigger_kind: String,
    todo_id: Option<String>,
    execution_plan: Option<team_todo::AgentTodoExecutionPlan>,
    binding: TeamExecutionBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingAgentCommunicationDelivery {
    delivery_id: String,
    message_id: String,
    content: String,
    trigger_kind: String,
    conversation_kind: String,
    room_name: String,
    binding: AgentExecutionBinding,
}

#[derive(Clone)]
struct ActiveAgentCommunicationRunBinding {
    delivery_id: String,
    conversation_id: String,
    turn_id: String,
    run_id: String,
}

struct AgentCommunicationRunReconciliation {
    delivery_id: String,
    turn_id: String,
    run_id: String,
    status: LocalConversationTurnStatus,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedAgentConversation {
    room_id: String,
    agent_id: String,
    conversation_id: String,
    title: String,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SendAgentConversationDraftInput {
    owner_user_id: String,
    room_id: String,
    guidance_turn_id: Option<String>,
    submission: AgentConversationDraftSubmission,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentConversationDraftSubmission {
    content: String,
    #[serde(default)]
    attachment_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentConversationSendAck {
    delivery_kind: &'static str,
    turn_id: String,
    user_message_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentConversationActivityItem {
    delivery_id: String,
    room_id: String,
    agent_id: String,
    turn_id: String,
    run_id: String,
    delivery_status: String,
    run_status: String,
    response_message_id: Option<String>,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentConversationActivitySnapshot {
    room_id: String,
    agent_id: String,
    conversation_id: String,
    conversation_version: u64,
    has_active_delivery: bool,
    items: Vec<AgentConversationActivityItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProfileActivitySnapshot {
    agent_id: String,
    has_active_delivery: bool,
    items: Vec<AgentConversationActivityItem>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamSummary {
    id: String,
    project_id: String,
    name: String,
    goal: String,
    default_agent_id: Option<String>,
    project_manager_agent_id: Option<String>,
    member_count: i64,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateAgentTeamInput {
    owner_user_id: String,
    project_id: String,
    name: String,
    goal: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamMemberSummary {
    agent_id: String,
    role: String,
    responsibility: String,
    joined_at_unix_ms: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamDetail {
    team: AgentTeamSummary,
    members: Vec<AgentTeamMemberSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InviteAgentToTeamInput {
    owner_user_id: String,
    team_id: String,
    agent_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateAgentTeamMemberInput {
    owner_user_id: String,
    team_id: String,
    agent_id: String,
    role: String,
    responsibility: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManageAgentTeamMemberInput {
    owner_user_id: String,
    team_id: String,
    agent_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QueueAgentTeamMessageInput {
    owner_user_id: String,
    team_id: String,
    content: String,
    #[serde(default)]
    mentioned_agent_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueuedAgentTeamDelivery {
    delivery_id: String,
    target_agent_id: String,
    trigger_kind: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueueAgentTeamMessageResult {
    message_id: String,
    team_id: String,
    created_at_unix_ms: i64,
    deliveries: Vec<QueuedAgentTeamDelivery>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DispatchedAgentTeamDelivery {
    delivery_id: String,
    team_id: String,
    agent_id: String,
    turn_id: String,
    run_id: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamMessageSummary {
    id: String,
    sender_kind: String,
    sender_id: String,
    content: String,
    mentioned_agent_ids: Vec<String>,
    created_at_unix_ms: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamDeliverySummary {
    id: String,
    message_id: String,
    target_agent_id: String,
    trigger_kind: String,
    status: String,
    response_message_id: Option<String>,
    last_error: Option<String>,
    created_at_unix_ms: i64,
    completed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamConversationSnapshot {
    team_id: String,
    messages: Vec<AgentTeamMessageSummary>,
    deliveries: Vec<AgentTeamDeliverySummary>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamRunSummary {
    delivery_id: String,
    message_id: String,
    agent_id: String,
    lane: String,
    trigger_kind: String,
    delivery_status: String,
    run_status: String,
    turn_id: String,
    run_id: String,
    response_message_id: Option<String>,
    last_error: Option<String>,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamRunHistorySnapshot {
    team_id: String,
    message_count: i64,
    pending_delivery_count: i64,
    running_delivery_count: i64,
    completed_delivery_count: i64,
    failed_delivery_count: i64,
    cancelled_delivery_count: i64,
    runs: Vec<AgentTeamRunSummary>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamAsset {
    pub(crate) id: String,
    team_id: String,
    pub(crate) category: String,
    pub(crate) title: String,
    pub(crate) markdown: String,
    pub(crate) revision: i64,
    status: String,
    created_by_agent_id: Option<String>,
    updated_by_agent_id: Option<String>,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamAssetRevision {
    asset_id: String,
    revision: i64,
    title: String,
    markdown: String,
    editor_agent_id: Option<String>,
    created_at_unix_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentAssetToolBinding {
    pub(crate) team_id: String,
    pub(crate) agent_id: String,
    pub(crate) todo_id: Option<String>,
    pub(crate) is_project_manager: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentCommunicationToolBinding {
    pub(crate) room_id: String,
    pub(crate) agent_id: String,
    pub(crate) conversation_kind: String,
    pub(crate) is_project_manager: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentAssetToolRecord {
    pub(crate) asset_id: String,
    pub(crate) category: String,
    pub(crate) title: String,
    pub(crate) markdown: String,
    pub(crate) revision: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveAgentTeamAssetInput {
    owner_user_id: String,
    team_id: String,
    asset_id: Option<String>,
    category: String,
    title: String,
    markdown: String,
    expected_revision: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ArchiveAgentTeamAssetInput {
    owner_user_id: String,
    team_id: String,
    asset_id: String,
    expected_revision: i64,
}

impl AgentConversationStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let connector = confined_connector_directory(&app_data_directory)?;
        let database = connector.join(DATABASE_FILENAME);
        reject_link_or_non_file(&database)?;
        let connection = open_database(&database, false)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|_| store_error())?;
        Ok(Self { database })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let app_data_directory = dirs::data_local_dir()
            .ok_or_else(store_error)?
            .join("com.chatos.windows");
        Self::new(app_data_directory)
    }

    fn open_human_agent_direct(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<DirectRoom, String> {
        validate_agent_identifier(agent_id)?;
        reject_link_or_non_file(&self.database)?;
        let direct_key = format!("human:{owner_user_id}|agent:{agent_id}");
        validate_identifier(&direct_key)?;
        let mut connection = open_database(&self.database, false)?;
        if let Some(room) = load_direct_room(&connection, owner_user_id, &direct_key)? {
            return Ok(room);
        }
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        if let Some(room) = load_direct_room(&transaction, owner_user_id, &direct_key)? {
            transaction.commit().map_err(|_| store_error())?;
            return Ok(room);
        }
        let profile: Option<(String, String)> = transaction
            .query_row(
                "SELECT name, description FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
                params![owner_user_id, agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| store_error())?;
        let Some((name, description)) = profile else {
            return Err("Agent 不存在或已停用。".into());
        };
        validate_text(&name, 120, false)?;
        validate_text(&description, 2_000, true)?;
        let room_id = uuid::Uuid::new_v4().to_string().to_lowercase();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                    project_manager_agent_id
                ) VALUES (?1, ?2, ?3, ?4, '', ?5, 'active', ?6, ?6,
                          'human_agent_direct', ?7, NULL)
                "#,
                params![
                    owner_user_id,
                    room_id,
                    format!("direct:{room_id}"),
                    name,
                    agent_id,
                    now,
                    direct_key,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_room_members (
                    owner_user_id, room_id, agent_id, role, responsibility,
                    plugin_allowlist_json, status, joined_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                "#,
                params![owner_user_id, room_id, agent_id, name, description, now],
            )
            .map_err(|_| store_error())?;
        transaction.commit().map_err(|_| store_error())?;
        Ok(DirectRoom {
            id: room_id,
            agent_id: agent_id.to_owned(),
            name,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        })
    }

    fn load_execution_binding(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<AgentExecutionBinding, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = connection
            .query_row(
                r#"
                SELECT room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id AND profile.id = member.agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.conversation_kind = 'human_agent_direct'
                  AND room.status = 'active' AND member.status = 'active'
                  AND profile.status = 'active' AND room.default_agent_id = member.agent_id
                ORDER BY member.joined_at_unix_ms, member.agent_id
                LIMIT 2
                "#,
                params![owner_user_id, room_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| store_error())?;
        let Some((
            room_id,
            agent_id,
            name,
            role_prompt,
            model_config_id,
            thinking_level,
            profession_key,
            plugin_ids_json,
            skill_ids_json,
        )) = binding
        else {
            return Err("Agent 私聊不存在、已停用或身份绑定已经失效。".into());
        };
        let active_member_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                params![owner_user_id, room_id],
                |row| row.get(0),
            )
            .map_err(|_| store_error())?;
        if active_member_count != 1 {
            return Err("Agent 私聊成员绑定无效，已拒绝执行。".into());
        }
        validate_agent_identifier(&agent_id)?;
        validate_text(&name, 120, false)?;
        validate_text(&role_prompt, 32_000, false)?;
        validate_identifier(&model_config_id)?;
        validate_identifier(&profession_key)?;
        if thinking_level.as_deref().is_some_and(|level| {
            !matches!(
                level,
                "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            )
        }) {
            return Err(store_error());
        }
        Ok(AgentExecutionBinding {
            room_id,
            agent_id,
            name,
            role_prompt,
            model_config_id,
            thinking_level,
            profession_key,
            default_plugin_ids: parse_identifier_array(&plugin_ids_json)?,
            default_skill_ids: parse_identifier_array(&skill_ids_json)?,
        })
    }

    fn next_agent_communication_delivery(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<Option<PendingAgentCommunicationDelivery>, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let pending = connection
            .query_row(
                r#"
                SELECT delivery.id, delivery.message_id, message.content,
                       delivery.trigger_kind, room.conversation_kind, room.name,
                       room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json
                FROM project_agent_deliveries delivery
                JOIN project_agent_messages message
                  ON message.owner_user_id = delivery.owner_user_id
                 AND message.id = delivery.message_id
                 AND message.room_id = delivery.room_id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = delivery.owner_user_id
                 AND room.id = delivery.room_id
                JOIN project_agent_room_members member
                  ON member.owner_user_id = delivery.owner_user_id
                 AND member.room_id = delivery.room_id
                 AND member.agent_id = delivery.target_agent_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.status = 'pending' AND delivery.trigger_kind != 'todo'
                  AND room.status = 'active'
                  AND (room.conversation_kind = 'agent_agent_direct'
                       OR (room.conversation_kind = 'human_agent_direct'
                           AND delivery.trigger_kind = 'heartbeat'))
                  AND member.status = 'active' AND profile.status = 'active'
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = delivery.owner_user_id
                      AND active.target_agent_id = delivery.target_agent_id
                      AND active.status = 'running' AND active.id != delivery.id
                      AND active.trigger_kind != 'todo'
                  )
                ORDER BY delivery.created_at_unix_ms, delivery.id
                LIMIT 1
                "#,
                params![owner_user_id, room_id],
                |row| {
                    Ok(PendingAgentCommunicationDelivery {
                        delivery_id: row.get(0)?,
                        message_id: row.get(1)?,
                        content: row.get(2)?,
                        trigger_kind: row.get(3)?,
                        conversation_kind: row.get(4)?,
                        room_name: row.get(5)?,
                        binding: AgentExecutionBinding {
                            room_id: row.get(6)?,
                            agent_id: row.get(7)?,
                            name: row.get(8)?,
                            role_prompt: row.get(9)?,
                            model_config_id: row.get(10)?,
                            thinking_level: row.get(11)?,
                            profession_key: row.get(12)?,
                            default_plugin_ids: parse_identifier_array(&row.get::<_, String>(13)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            default_skill_ids: parse_identifier_array(&row.get::<_, String>(14)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        },
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?;
        let Some(pending) = pending else {
            return Ok(None);
        };
        for value in [
            pending.delivery_id.as_str(),
            pending.message_id.as_str(),
            pending.binding.room_id.as_str(),
            pending.binding.agent_id.as_str(),
        ] {
            validate_identifier(value).map_err(|_| team_store_error())?;
        }
        if !matches!(
            pending.trigger_kind.as_str(),
            "default_agent" | "agent_mention" | "heartbeat"
        ) {
            return Err(team_store_error());
        }
        validate_agent_execution_binding(&pending.binding)?;
        if !matches!(
            pending.conversation_kind.as_str(),
            "human_agent_direct" | "agent_agent_direct"
        ) || (pending.trigger_kind == "heartbeat"
            && pending.conversation_kind != "human_agent_direct")
        {
            return Err(team_store_error());
        }
        validate_text(&pending.room_name, 160, false).map_err(|_| team_store_error())?;
        validate_text(&pending.content, 1_000_000, false).map_err(|_| team_store_error())?;
        Ok(Some(pending))
    }

    fn claim_agent_communication_delivery(
        &self,
        owner_user_id: &str,
        pending: &PendingAgentCommunicationDelivery,
        turn_id: &str,
        run_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        validate_identifier(turn_id)?;
        validate_identifier(run_id)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let changed = transaction
            .execute(
                r#"
                UPDATE project_agent_deliveries
                SET status = 'running', attempt = attempt + 1,
                    claimed_at_unix_ms = ?1, last_error = NULL
                WHERE owner_user_id = ?2 AND id = ?3 AND room_id = ?4
                  AND target_agent_id = ?5 AND status = 'pending'
                  AND trigger_kind != 'todo'
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = ?2 AND active.target_agent_id = ?5
                      AND active.status = 'running' AND active.id != ?3
                      AND active.trigger_kind != 'todo'
                  )
                "#,
                params![
                    now_unix_ms,
                    owner_user_id,
                    pending.delivery_id,
                    pending.binding.room_id,
                    pending.binding.agent_id,
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent Delivery 已被其他执行器领取，请刷新后重试。".into());
        }
        transaction
            .execute(
                "INSERT INTO windows_agent_delivery_runs (owner_user_id, delivery_id, turn_id, \
                 run_id, status, created_at_unix_ms, updated_at_unix_ms) \
                 VALUES (?1, ?2, ?3, ?4, 'ready', ?5, ?5)",
                params![
                    owner_user_id,
                    pending.delivery_id,
                    turn_id,
                    run_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())
    }

    fn active_agent_communication_run_bindings(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<Vec<ActiveAgentCommunicationRunBinding>, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.target_agent_id, run.turn_id, run.run_id
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = delivery.owner_user_id
                 AND room.id = delivery.room_id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND room.status = 'active'
                  AND (room.conversation_kind = 'agent_agent_direct'
                       OR (room.conversation_kind = 'human_agent_direct'
                           AND delivery.trigger_kind = 'heartbeat'))
                  AND (delivery.status = 'running' OR run.status IN ('ready', 'running'))
                ORDER BY run.updated_at_unix_ms DESC, delivery.id
                LIMIT 500
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, room_id], |row| {
                let agent_id = row.get::<_, String>(1)?;
                Ok(ActiveAgentCommunicationRunBinding {
                    delivery_id: row.get(0)?,
                    conversation_id: agent_communication_conversation_id(room_id, &agent_id),
                    turn_id: row.get(2)?,
                    run_id: row.get(3)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let bindings = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for binding in &bindings {
            for value in [
                &binding.delivery_id,
                &binding.conversation_id,
                &binding.turn_id,
                &binding.run_id,
            ] {
                validate_identifier(value).map_err(|_| team_store_error())?;
            }
        }
        Ok(bindings)
    }

    fn reconcile_agent_communication_runs(
        &self,
        owner_user_id: &str,
        room_id: &str,
        updates: &[AgentCommunicationRunReconciliation],
    ) -> Result<(), String> {
        validate_identifier(room_id)?;
        if updates.len() > 500 {
            return Err(team_store_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        for update in updates {
            for value in [&update.delivery_id, &update.turn_id, &update.run_id] {
                validate_identifier(value).map_err(|_| team_store_error())?;
            }
            if update.created_at_unix_ms < 0
                || update.updated_at_unix_ms < update.created_at_unix_ms
            {
                return Err(team_store_error());
            }
            let delivery_status = match update.status {
                LocalConversationTurnStatus::Running => "running",
                LocalConversationTurnStatus::Succeeded => "completed",
                LocalConversationTurnStatus::Failed => "failed",
                LocalConversationTurnStatus::Cancelled => "cancelled",
            };
            let completed_at = if update.status == LocalConversationTurnStatus::Running {
                None
            } else {
                Some(update.updated_at_unix_ms)
            };
            let changed = transaction
                .execute(
                    r#"
                    UPDATE project_agent_deliveries
                    SET status = ?1,
                        claimed_at_unix_ms = COALESCE(claimed_at_unix_ms, ?2),
                        completed_at_unix_ms = ?3
                    WHERE owner_user_id = ?4 AND room_id = ?5 AND id = ?6
                      AND status IN ('pending', 'running')
                      AND EXISTS (
                        SELECT 1 FROM windows_agent_delivery_runs run
                        WHERE run.owner_user_id = project_agent_deliveries.owner_user_id
                          AND run.delivery_id = project_agent_deliveries.id
                          AND run.turn_id = ?7 AND run.run_id = ?8
                      )
                    "#,
                    params![
                        delivery_status,
                        update.created_at_unix_ms,
                        completed_at,
                        owner_user_id,
                        room_id,
                        update.delivery_id,
                        update.turn_id,
                        update.run_id,
                    ],
                )
                .map_err(|_| team_store_error())?;
            if changed > 1 {
                return Err(team_store_error());
            }
            let run_changed = transaction
                .execute(
                    "UPDATE windows_agent_delivery_runs SET status = ?1, \
                     updated_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND delivery_id = ?4 \
                     AND turn_id = ?5 AND run_id = ?6",
                    params![
                        update.status.as_str(),
                        update.updated_at_unix_ms,
                        owner_user_id,
                        update.delivery_id,
                        update.turn_id,
                        update.run_id,
                    ],
                )
                .map_err(|_| team_store_error())?;
            if run_changed != 1 {
                return Err(team_store_error());
            }
        }
        transaction.commit().map_err(|_| team_store_error())
    }

    fn activity_room_ids(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, String> {
        validate_agent_identifier(agent_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let active: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active')",
                params![owner_user_id, agent_id],
                |row| row.get(0),
            )
            .map_err(|_| store_error())?;
        if !active {
            return Err("Agent 不存在或已停用。".into());
        }
        let mut statement = connection
            .prepare(
                r#"
                SELECT DISTINCT room.id
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                WHERE room.owner_user_id = ?1 AND member.agent_id = ?2
                  AND room.status = 'active' AND member.status = 'active'
                  AND room.conversation_kind = 'human_agent_direct'
                ORDER BY room.updated_at_unix_ms DESC, room.id DESC
                LIMIT 100
                "#,
            )
            .map_err(|_| store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, agent_id], |row| row.get(0))
            .map_err(|_| store_error())?;
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(|_| store_error())
    }

    fn list_project_teams(&self, owner_user_id: &str) -> Result<Vec<AgentTeamSummary>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT room.id, room.project_id, room.name, room.goal,
                       room.default_agent_id, room.project_manager_agent_id,
                       (SELECT count(*) FROM project_agent_room_members member
                        WHERE member.owner_user_id = room.owner_user_id
                          AND member.room_id = room.id AND member.status = 'active'),
                       room.created_at_unix_ms, room.updated_at_unix_ms
                FROM project_agent_rooms room
                WHERE room.owner_user_id = ?1 AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                ORDER BY room.updated_at_unix_ms DESC, room.id DESC
                LIMIT 1000
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], |row| {
                Ok(AgentTeamSummary {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    goal: row.get(3)?,
                    default_agent_id: row.get(4)?,
                    project_manager_agent_id: row.get(5)?,
                    member_count: row.get(6)?,
                    created_at_unix_ms: row.get(7)?,
                    updated_at_unix_ms: row.get(8)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let teams = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for team in &teams {
            validate_team_summary(team)?;
        }
        Ok(teams)
    }

    fn create_project_team(
        &self,
        owner_user_id: &str,
        project_id: &str,
        name: &str,
        goal: &str,
    ) -> Result<AgentTeamSummary, String> {
        validate_identifier(project_id).map_err(|_| invalid_team_input())?;
        validate_text(name, 160, false).map_err(|_| invalid_team_input())?;
        validate_text(goal, 8_000, true).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms \
                 WHERE owner_user_id = ?1 AND project_id = ?2 \
                   AND status = 'active' AND conversation_kind = 'project_team')",
                params![owner_user_id, project_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if exists {
            return Err("这个项目已经有 Agent 团队，请直接打开现有团队。".into());
        }
        let now = chrono::Utc::now().timestamp_millis().max(0);
        let id = uuid::Uuid::new_v4().to_string().to_lowercase();
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                    project_manager_agent_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'active', ?6, ?6,
                          'project_team', NULL, NULL)
                "#,
                params![owner_user_id, id, project_id, name, goal, now],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        let team = AgentTeamSummary {
            id,
            project_id: project_id.to_owned(),
            name: name.to_owned(),
            goal: goal.to_owned(),
            default_agent_id: None,
            project_manager_agent_id: None,
            member_count: 0,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        validate_team_summary(&team)?;
        Ok(team)
    }

    fn load_project_team(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let team = connection
            .query_row(
                r#"
                SELECT room.id, room.project_id, room.name, room.goal,
                       room.default_agent_id, room.project_manager_agent_id,
                       (SELECT count(*) FROM project_agent_room_members member
                        WHERE member.owner_user_id = room.owner_user_id
                          AND member.room_id = room.id AND member.status = 'active'),
                       room.created_at_unix_ms, room.updated_at_unix_ms
                FROM project_agent_rooms room
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                "#,
                params![owner_user_id, team_id],
                |row| {
                    Ok(AgentTeamSummary {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        name: row.get(2)?,
                        goal: row.get(3)?,
                        default_agent_id: row.get(4)?,
                        project_manager_agent_id: row.get(5)?,
                        member_count: row.get(6)?,
                        created_at_unix_ms: row.get(7)?,
                        updated_at_unix_ms: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 团队不存在或已停用。".to_owned())?;
        validate_team_summary(&team)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT member.agent_id, member.role, member.responsibility,
                       member.joined_at_unix_ms
                FROM project_agent_room_members member
                WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                  AND member.status = 'active'
                ORDER BY member.joined_at_unix_ms, member.agent_id
                LIMIT 1000
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], |row| {
                Ok(AgentTeamMemberSummary {
                    agent_id: row.get(0)?,
                    role: row.get(1)?,
                    responsibility: row.get(2)?,
                    joined_at_unix_ms: row.get(3)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let members = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for member in &members {
            validate_team_member(member)?;
        }
        if members.len() as i64 != team.member_count {
            return Err(team_store_error());
        }
        Ok(AgentTeamDetail { team, members })
    }

    fn invite_agent_to_project_team(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let team_exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active' \
                   AND conversation_kind = 'project_team')",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !team_exists {
            return Err("Agent 团队不存在或已停用。".into());
        }
        let profile = transaction
            .query_row(
                "SELECT name, description FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
                params![owner_user_id, agent_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 不存在或已停用。".to_owned())?;
        validate_text(&profile.0, 160, false).map_err(|_| team_store_error())?;
        validate_text(&profile.1, 8_000, true).map_err(|_| team_store_error())?;
        let existing_status = transaction
            .query_row(
                "SELECT status FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND agent_id = ?3",
                params![owner_user_id, team_id, agent_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| team_store_error())?;
        if existing_status.as_deref() == Some("active") {
            return Err("这个 Agent 已经在当前团队中。".into());
        }
        let member_count: i64 = transaction
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        let now = chrono::Utc::now().timestamp_millis().max(0);
        if existing_status.as_deref() == Some("removed") {
            transaction
                .execute(
                    "UPDATE project_agent_room_members \
                     SET role = ?1, responsibility = ?2, plugin_allowlist_json = '[]', \
                         status = 'active', joined_at_unix_ms = ?3 \
                     WHERE owner_user_id = ?4 AND room_id = ?5 AND agent_id = ?6 \
                       AND status = 'removed'",
                    params![profile.0, profile.1, now, owner_user_id, team_id, agent_id],
                )
                .map_err(|_| team_store_error())?;
        } else if existing_status.is_none() {
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                    "#,
                    params![owner_user_id, team_id, agent_id, profile.0, profile.1, now],
                )
                .map_err(|_| team_store_error())?;
        } else {
            return Err(team_store_error());
        }
        transaction
            .execute(
                "UPDATE project_agent_rooms \
                 SET default_agent_id = CASE WHEN ?1 = 0 THEN ?2 ELSE default_agent_id END, \
                     updated_at_unix_ms = ?3 \
                 WHERE owner_user_id = ?4 AND id = ?5 AND status = 'active'",
                params![member_count, agent_id, now, owner_user_id, team_id],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        self.load_project_team(owner_user_id, team_id)
    }

    fn update_project_team_member(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        role: &str,
        responsibility: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        validate_text(role, 160, false).map_err(|_| invalid_team_input())?;
        validate_text(responsibility, 8_000, true).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, false)?;
        let changed = connection
            .execute(
                r#"
                UPDATE project_agent_room_members
                SET role = ?1, responsibility = ?2
                WHERE owner_user_id = ?3 AND room_id = ?4 AND agent_id = ?5
                  AND status = 'active'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_rooms room
                    WHERE room.owner_user_id = ?3 AND room.id = ?4
                      AND room.status = 'active' AND room.conversation_kind = 'project_team'
                  )
                "#,
                params![role, responsibility, owner_user_id, team_id, agent_id],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent 不是当前团队的活动成员。".into());
        }
        self.load_project_team(owner_user_id, team_id)
    }

    fn set_project_team_default_agent(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        self.assign_project_team_agent(owner_user_id, team_id, agent_id, false)
    }

    fn set_project_team_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        self.assign_project_team_agent(owner_user_id, team_id, agent_id, true)
    }

    fn assign_project_team_agent(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        project_manager: bool,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let member_exists: bool = transaction
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM project_agent_room_members member
                    JOIN project_agent_rooms room
                      ON room.owner_user_id = member.owner_user_id AND room.id = member.room_id
                    WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                      AND member.agent_id = ?3 AND member.status = 'active'
                      AND room.status = 'active' AND room.conversation_kind = 'project_team'
                )
                "#,
                params![owner_user_id, team_id, agent_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !member_exists {
            return Err("Agent 不是当前团队的活动成员。".into());
        }
        if project_manager {
            let profession = transaction
                .query_row(
                    "SELECT profession_key FROM local_agent_profiles \
                     WHERE owner_user_id = ?1 AND id = ?2",
                    params![owner_user_id, agent_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| team_store_error())?;
            if profession.as_deref() != Some("project_manager") {
                return Err("只有职业为“项目经理”的 Agent 才能担任项目经理。".into());
            }
        }
        let now = chrono::Utc::now().timestamp_millis().max(0);
        let column = if project_manager {
            "project_manager_agent_id"
        } else {
            "default_agent_id"
        };
        let changed = transaction
            .execute(
                &format!(
                    "UPDATE project_agent_rooms SET {column} = ?1, updated_at_unix_ms = \
                     max(updated_at_unix_ms, ?2) WHERE owner_user_id = ?3 AND id = ?4 \
                     AND status = 'active' AND conversation_kind = 'project_team'"
                ),
                params![agent_id, now, owner_user_id, team_id],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent 团队不存在或已停用。".into());
        }
        transaction.commit().map_err(|_| team_store_error())?;
        self.load_project_team(owner_user_id, team_id)
    }

    fn remove_agent_from_project_team(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let room = transaction
            .query_row(
                "SELECT default_agent_id, project_manager_agent_id, updated_at_unix_ms \
                 FROM project_agent_rooms WHERE owner_user_id = ?1 AND id = ?2 \
                 AND status = 'active' AND conversation_kind = 'project_team'",
                params![owner_user_id, team_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 团队不存在或已停用。".to_owned())?;
        if room.1.as_deref() == Some(agent_id) {
            return Err("请先指定新的项目经理，再移出当前项目经理。".into());
        }
        let now = chrono::Utc::now().timestamp_millis().max(room.2);
        transaction
            .execute(
                "UPDATE project_agent_deliveries \
                 SET status = CASE WHEN status = 'pending' THEN 'cancelled' ELSE 'failed' END, \
                     last_error = '成员已由 Human 移出当前团队。', completed_at_unix_ms = ?1 \
                 WHERE owner_user_id = ?2 AND room_id = ?3 AND target_agent_id = ?4 \
                   AND status IN ('pending', 'running')",
                params![now, owner_user_id, team_id, agent_id],
            )
            .map_err(|_| team_store_error())?;
        let changed = transaction
            .execute(
                "UPDATE project_agent_room_members SET status = 'removed' \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND agent_id = ?3 \
                   AND status = 'active'",
                params![owner_user_id, team_id, agent_id],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent 不是当前团队的活动成员。".into());
        }
        if room.0.as_deref() == Some(agent_id) {
            let replacement = transaction
                .query_row(
                    "SELECT agent_id FROM project_agent_room_members \
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active' \
                     ORDER BY joined_at_unix_ms, agent_id LIMIT 1",
                    params![owner_user_id, team_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| team_store_error())?;
            transaction
                .execute(
                    "UPDATE project_agent_rooms SET default_agent_id = ?1, \
                     updated_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
                     AND status = 'active'",
                    params![replacement, now, owner_user_id, team_id],
                )
                .map_err(|_| team_store_error())?;
        }
        transaction.commit().map_err(|_| team_store_error())?;
        self.load_project_team(owner_user_id, team_id)
    }

    fn queue_project_team_message(
        &self,
        owner_user_id: &str,
        team_id: &str,
        content: &str,
        mentioned_agent_ids: &[String],
    ) -> Result<QueueAgentTeamMessageResult, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_text(content, 1_000_000, false).map_err(|_| invalid_team_message())?;
        if mentioned_agent_ids.len() > 100 {
            return Err(invalid_team_message());
        }
        let mut unique_mentions = std::collections::HashSet::new();
        for agent_id in mentioned_agent_ids {
            validate_agent_identifier(agent_id).map_err(|_| invalid_team_message())?;
            if !unique_mentions.insert(agent_id.as_str()) {
                return Err(invalid_team_message());
            }
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let default_agent_id = transaction
            .query_row(
                "SELECT default_agent_id FROM project_agent_rooms \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active' \
                   AND conversation_kind = 'project_team'",
                params![owner_user_id, team_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 团队不存在或已停用。".to_owned())?;
        let active_member_ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT agent_id FROM project_agent_room_members \
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<std::collections::HashSet<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        if mentioned_agent_ids
            .iter()
            .any(|agent_id| !active_member_ids.contains(agent_id))
        {
            return Err("@Agent 必须是当前团队的活动成员。".into());
        }
        let targets = if mentioned_agent_ids.is_empty() {
            default_agent_id.into_iter().collect::<Vec<_>>()
        } else {
            mentioned_agent_ids.to_vec()
        };
        let message_id = uuid::Uuid::new_v4().to_string().to_lowercase();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, 'human', ?1, ?4, NULL, NULL, NULL, ?2, 0, ?5)
                "#,
                params![owner_user_id, message_id, team_id, content, now],
            )
            .map_err(|_| team_store_error())?;
        for (position, agent_id) in mentioned_agent_ids.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO project_agent_message_mentions (\
                     owner_user_id, message_id, agent_id, position) VALUES (?1, ?2, ?3, ?4)",
                    params![owner_user_id, message_id, agent_id, position as i64],
                )
                .map_err(|_| team_store_error())?;
        }
        let trigger_kind = if mentioned_agent_ids.is_empty() {
            "default_agent"
        } else {
            "mention"
        };
        let mut deliveries = Vec::with_capacity(targets.len());
        for target_agent_id in targets {
            let delivery_id = uuid::Uuid::new_v4().to_string().to_lowercase();
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_deliveries (
                        owner_user_id, id, room_id, message_id, root_message_id,
                        target_agent_id, trigger_kind, status, attempt, hop_count,
                        deduplication_key, response_message_id, last_error,
                        claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, 'pending', 0, 0,
                              ?7, NULL, NULL, NULL, NULL, ?8)
                    "#,
                    params![
                        owner_user_id,
                        delivery_id,
                        team_id,
                        message_id,
                        target_agent_id,
                        trigger_kind,
                        format!("{message_id}:{target_agent_id}"),
                        now,
                    ],
                )
                .map_err(|_| team_store_error())?;
            deliveries.push(QueuedAgentTeamDelivery {
                delivery_id,
                target_agent_id,
                trigger_kind: trigger_kind.to_owned(),
            });
        }
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(QueueAgentTeamMessageResult {
            message_id,
            team_id: team_id.to_owned(),
            created_at_unix_ms: now,
            deliveries,
        })
    }

    fn load_project_team_conversation(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentTeamConversationSnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms WHERE owner_user_id = ?1 \
                 AND id = ?2 AND status = 'active' AND conversation_kind = 'project_team')",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !exists {
            return Err("Agent 团队不存在或已停用。".into());
        }
        let message_rows = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT id, sender_kind, sender_id, content, created_at_unix_ms
                    FROM (
                        SELECT id, sender_kind, sender_id, content, created_at_unix_ms
                        FROM project_agent_messages
                        WHERE owner_user_id = ?1 AND room_id = ?2
                        ORDER BY created_at_unix_ms DESC, id DESC LIMIT 200
                    ) recent
                    ORDER BY created_at_unix_ms, id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        let message_ids = message_rows
            .iter()
            .map(|row| row.0.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut mentions_by_message: std::collections::HashMap<String, Vec<(i64, String)>> =
            std::collections::HashMap::new();
        {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT mention.message_id, mention.position, mention.agent_id
                    FROM project_agent_message_mentions mention
                    JOIN project_agent_messages message
                      ON message.owner_user_id = mention.owner_user_id
                     AND message.id = mention.message_id
                    WHERE mention.owner_user_id = ?1 AND message.room_id = ?2
                    ORDER BY mention.message_id, mention.position, mention.agent_id
                    LIMIT 20000
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| team_store_error())?;
            for row in rows {
                let (message_id, position, agent_id) = row.map_err(|_| team_store_error())?;
                if message_ids.contains(message_id.as_str()) {
                    mentions_by_message
                        .entry(message_id)
                        .or_default()
                        .push((position, agent_id));
                }
            }
        }
        let mut messages = Vec::with_capacity(message_rows.len());
        for (id, sender_kind, sender_id, content, created_at_unix_ms) in message_rows {
            validate_identifier(&id).map_err(|_| team_store_error())?;
            validate_text(&content, 1_000_000, true).map_err(|_| team_store_error())?;
            let mentioned_agent_ids = mentions_by_message
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, agent_id)| agent_id)
                .collect();
            messages.push(AgentTeamMessageSummary {
                id,
                sender_kind,
                sender_id,
                content,
                mentioned_agent_ids,
                created_at_unix_ms,
            });
        }
        let deliveries = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT id, message_id, target_agent_id, trigger_kind, status,
                           response_message_id, last_error, created_at_unix_ms,
                           completed_at_unix_ms
                    FROM project_agent_deliveries
                    WHERE owner_user_id = ?1 AND room_id = ?2
                    ORDER BY created_at_unix_ms DESC, id DESC LIMIT 500
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok(AgentTeamDeliverySummary {
                        id: row.get(0)?,
                        message_id: row.get(1)?,
                        target_agent_id: row.get(2)?,
                        trigger_kind: row.get(3)?,
                        status: row.get(4)?,
                        response_message_id: row.get(5)?,
                        last_error: row.get(6)?,
                        created_at_unix_ms: row.get(7)?,
                        completed_at_unix_ms: row.get(8)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        Ok(AgentTeamConversationSnapshot {
            team_id: team_id.to_owned(),
            messages,
            deliveries,
        })
    }

    fn load_project_team_runs(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentTeamRunHistorySnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms WHERE owner_user_id = ?1 \
                 AND id = ?2 AND status = 'active' AND conversation_kind = 'project_team')",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !exists {
            return Err("Agent 团队不存在或已停用。".into());
        }
        let message_count = connection
            .query_row(
                "SELECT count(*) FROM project_agent_messages \
                 WHERE owner_user_id = ?1 AND room_id = ?2",
                params![owner_user_id, team_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| team_store_error())?;
        let delivery_counts = connection
            .query_row(
                r#"
                SELECT
                    COALESCE(sum(CASE status WHEN 'pending' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'running' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'completed' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'failed' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'cancelled' THEN 1 ELSE 0 END), 0)
                FROM project_agent_deliveries
                WHERE owner_user_id = ?1 AND room_id = ?2
                "#,
                params![owner_user_id, team_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(|_| team_store_error())?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.message_id, delivery.target_agent_id,
                       CASE WHEN delivery.trigger_kind = 'todo' THEN 'executor' ELSE 'manager' END,
                       delivery.trigger_kind, delivery.status, run.status, run.turn_id,
                       run.run_id, delivery.response_message_id, delivery.last_error,
                       delivery.created_at_unix_ms, run.updated_at_unix_ms
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                ORDER BY run.updated_at_unix_ms DESC, run.run_id DESC
                LIMIT 500
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], |row| {
                Ok(AgentTeamRunSummary {
                    delivery_id: row.get(0)?,
                    message_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    lane: row.get(3)?,
                    trigger_kind: row.get(4)?,
                    delivery_status: row.get(5)?,
                    run_status: row.get(6)?,
                    turn_id: row.get(7)?,
                    run_id: row.get(8)?,
                    response_message_id: row.get(9)?,
                    last_error: row.get(10)?,
                    created_at_unix_ms: row.get(11)?,
                    updated_at_unix_ms: row.get(12)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let runs = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        Ok(AgentTeamRunHistorySnapshot {
            team_id: team_id.to_owned(),
            message_count,
            pending_delivery_count: delivery_counts.0,
            running_delivery_count: delivery_counts.1,
            completed_delivery_count: delivery_counts.2,
            failed_delivery_count: delivery_counts.3,
            cancelled_delivery_count: delivery_counts.4,
            runs,
        })
    }

    fn list_project_team_assets(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<Vec<AgentTeamAsset>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, team_room_id, category, title, markdown, revision, status,
                       created_by_agent_id, updated_by_agent_id,
                       created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_team_assets
                WHERE owner_user_id = ?1 AND team_room_id = ?2 AND status = 'active'
                ORDER BY category, updated_at_unix_ms DESC, id
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], map_team_asset)
            .map_err(|_| team_store_error())?;
        let assets = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for asset in &assets {
            validate_team_asset(asset)?;
        }
        Ok(assets)
    }

    pub(crate) fn load_agent_asset_tool_records(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<(AgentAssetToolBinding, Vec<AgentAssetToolRecord>), String> {
        validate_identifier(run_id).map_err(|_| team_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        let records = if let Some(todo_id) = binding.todo_id.as_deref() {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT asset_id, category, title, markdown, revision
                    FROM local_agent_todo_asset_snapshots
                    WHERE owner_user_id = ?1 AND todo_id = ?2 AND team_room_id = ?3
                    ORDER BY category, title, asset_id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, todo_id, binding.team_id], |row| {
                    Ok(AgentAssetToolRecord {
                        asset_id: row.get(0)?,
                        category: row.get(1)?,
                        title: row.get(2)?,
                        markdown: row.get(3)?,
                        revision: row.get(4)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        } else {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT id, category, title, markdown, revision
                    FROM local_agent_team_assets
                    WHERE owner_user_id = ?1 AND team_room_id = ?2 AND status = 'active'
                    ORDER BY category, title, id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, binding.team_id], |row| {
                    Ok(AgentAssetToolRecord {
                        asset_id: row.get(0)?,
                        category: row.get(1)?,
                        title: row.get(2)?,
                        markdown: row.get(3)?,
                        revision: row.get(4)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        Ok((binding, records))
    }

    #[allow(clippy::too_many_arguments)]
    fn save_project_team_asset(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: Option<&str>,
        category: &str,
        title: &str,
        markdown: &str,
        expected_revision: Option<i64>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.save_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            category,
            title,
            markdown,
            expected_revision,
            None,
            now_unix_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn save_project_team_asset_as_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        asset_id: Option<&str>,
        category: &str,
        title: &str,
        markdown: &str,
        expected_revision: Option<i64>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.save_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            category,
            title,
            markdown,
            expected_revision,
            Some(agent_id),
            now_unix_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn save_project_team_asset_with_editor(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: Option<&str>,
        category: &str,
        title: &str,
        markdown: &str,
        expected_revision: Option<i64>,
        editor_agent_id: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        if let Some(asset_id) = asset_id {
            validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
        }
        validate_team_asset_category(category)?;
        validate_text(title, 240, false).map_err(|_| invalid_team_asset_input())?;
        validate_text(markdown, 128_000, true).map_err(|_| invalid_team_asset_input())?;
        if now_unix_ms < 0 || expected_revision.is_some_and(|revision| revision <= 0) {
            return Err(invalid_team_asset_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        if let Some(agent_id) = editor_agent_id {
            require_project_team_manager(&transaction, owner_user_id, team_id, agent_id)?;
        }
        let resolved_id = asset_id
            .map(str::to_owned)
            .unwrap_or_else(|| identifier("asset"));
        let existing = transaction
            .query_row(
                r#"
                SELECT id, team_room_id, category, title, markdown, revision, status,
                       created_by_agent_id, updated_by_agent_id,
                       created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_team_assets
                WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3
                LIMIT 1
                "#,
                params![owner_user_id, team_id, resolved_id],
                map_team_asset,
            )
            .optional()
            .map_err(|_| team_store_error())?;
        let saved = if let Some(existing) = existing {
            if existing.status != "active" || expected_revision != Some(existing.revision) {
                return Err(team_asset_conflict());
            }
            let revision = existing.revision + 1;
            let updated_at_unix_ms = now_unix_ms.max(existing.created_at_unix_ms);
            let changed = transaction
                .execute(
                    r#"
                    UPDATE local_agent_team_assets
                    SET category = ?1, title = ?2, markdown = ?3, revision = ?4,
                        updated_by_agent_id = ?5, updated_at_unix_ms = ?6
                    WHERE owner_user_id = ?7 AND team_room_id = ?8 AND id = ?9
                      AND status = 'active' AND revision = ?10
                    "#,
                    params![
                        category,
                        title,
                        markdown,
                        revision,
                        editor_agent_id,
                        updated_at_unix_ms,
                        owner_user_id,
                        team_id,
                        resolved_id,
                        existing.revision,
                    ],
                )
                .map_err(|_| team_store_error())?;
            if changed != 1 {
                return Err(team_asset_conflict());
            }
            AgentTeamAsset {
                id: resolved_id,
                team_id: team_id.to_owned(),
                category: category.to_owned(),
                title: title.to_owned(),
                markdown: markdown.to_owned(),
                revision,
                status: "active".into(),
                created_by_agent_id: existing.created_by_agent_id,
                updated_by_agent_id: editor_agent_id.map(str::to_owned),
                created_at_unix_ms: existing.created_at_unix_ms,
                updated_at_unix_ms,
            }
        } else {
            if asset_id.is_some() || expected_revision.is_some() {
                return Err("团队共享资产不存在。".into());
            }
            transaction
                .execute(
                    r#"
                    INSERT INTO local_agent_team_assets (
                        owner_user_id, id, team_room_id, category, title, markdown,
                        revision, status, created_by_agent_id, updated_by_agent_id,
                        created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 'active', ?7, ?7, ?8, ?8)
                    "#,
                    params![
                        owner_user_id,
                        resolved_id,
                        team_id,
                        category,
                        title,
                        markdown,
                        editor_agent_id,
                        now_unix_ms,
                    ],
                )
                .map_err(|_| team_store_error())?;
            AgentTeamAsset {
                id: resolved_id,
                team_id: team_id.to_owned(),
                category: category.to_owned(),
                title: title.to_owned(),
                markdown: markdown.to_owned(),
                revision: 1,
                status: "active".into(),
                created_by_agent_id: editor_agent_id.map(str::to_owned),
                updated_by_agent_id: editor_agent_id.map(str::to_owned),
                created_at_unix_ms: now_unix_ms,
                updated_at_unix_ms: now_unix_ms,
            }
        };
        validate_team_asset(&saved)?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    owner_user_id,
                    saved.id,
                    saved.revision,
                    saved.title,
                    saved.markdown,
                    editor_agent_id,
                    saved.updated_at_unix_ms,
                ],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(saved)
    }

    fn archive_project_team_asset(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: &str,
        expected_revision: i64,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.archive_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            expected_revision,
            None,
            now_unix_ms,
        )
    }

    pub(crate) fn archive_project_team_asset_as_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        asset_id: &str,
        expected_revision: i64,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.archive_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            expected_revision,
            Some(agent_id),
            now_unix_ms,
        )
    }

    fn archive_project_team_asset_with_editor(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: &str,
        expected_revision: i64,
        editor_agent_id: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
        if expected_revision <= 0 || now_unix_ms < 0 {
            return Err(invalid_team_asset_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        if let Some(agent_id) = editor_agent_id {
            require_project_team_manager(&transaction, owner_user_id, team_id, agent_id)?;
        }
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_team_assets
                SET status = 'archived', revision = revision + 1,
                    updated_by_agent_id = ?1, updated_at_unix_ms = ?2
                WHERE owner_user_id = ?3 AND team_room_id = ?4 AND id = ?5
                  AND status = 'active' AND revision = ?6
                "#,
                params![
                    editor_agent_id,
                    now_unix_ms,
                    owner_user_id,
                    team_id,
                    asset_id,
                    expected_revision
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err(team_asset_conflict());
        }
        let archived = transaction
            .query_row(
                r#"
                SELECT id, team_room_id, category, title, markdown, revision, status,
                       created_by_agent_id, updated_by_agent_id,
                       created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_team_assets
                WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3
                "#,
                params![owner_user_id, team_id, asset_id],
                map_team_asset,
            )
            .map_err(|_| team_store_error())?;
        validate_team_asset(&archived)?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    owner_user_id,
                    asset_id,
                    archived.revision,
                    archived.title,
                    archived.markdown,
                    editor_agent_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(archived)
    }

    fn list_project_team_asset_revisions(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: &str,
    ) -> Result<Vec<AgentTeamAssetRevision>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let belongs = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_team_assets WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, asset_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| team_store_error())?;
        if !belongs {
            return Err("团队共享资产不存在。".into());
        }
        let mut statement = connection
            .prepare(
                r#"
                SELECT asset_id, revision, title, markdown, editor_agent_id, created_at_unix_ms
                FROM local_agent_team_asset_revisions
                WHERE owner_user_id = ?1 AND asset_id = ?2
                ORDER BY revision DESC LIMIT 500
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, asset_id], |row| {
                Ok(AgentTeamAssetRevision {
                    asset_id: row.get(0)?,
                    revision: row.get(1)?,
                    title: row.get(2)?,
                    markdown: row.get(3)?,
                    editor_agent_id: row.get(4)?,
                    created_at_unix_ms: row.get(5)?,
                })
            })
            .map_err(|_| team_store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())
    }

    fn next_project_team_delivery(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<Option<PendingTeamDelivery>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let pending = connection
            .query_row(
                r#"
                SELECT delivery.id, delivery.message_id, message.content,
                       delivery.trigger_kind, todo.id, todo.execution_plan_json,
                       room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json, room.project_id, room.name, room.goal,
                       member.role, member.responsibility,
                       CASE WHEN room.project_manager_agent_id = profile.id THEN 1 ELSE 0 END
                FROM project_agent_deliveries delivery
                JOIN project_agent_messages message
                  ON message.owner_user_id = delivery.owner_user_id
                 AND message.id = delivery.message_id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = delivery.owner_user_id
                 AND room.id = delivery.room_id
                LEFT JOIN local_agent_todos todo
                  ON todo.owner_user_id = delivery.owner_user_id
                 AND todo.agent_id = delivery.target_agent_id
                 AND delivery.trigger_kind = 'todo'
                 AND (delivery.deduplication_key = 'todo:' || todo.id
                      OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
                JOIN project_agent_room_members member
                  ON member.owner_user_id = delivery.owner_user_id
                 AND member.room_id = delivery.room_id
                 AND member.agent_id = delivery.target_agent_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.status = 'pending' AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                  AND member.status = 'active' AND profile.status = 'active'
                  AND (delivery.trigger_kind != 'todo' OR todo.status = 'in_progress')
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = delivery.owner_user_id
                      AND active.target_agent_id = delivery.target_agent_id
                      AND active.status = 'running' AND active.id != delivery.id
                      AND ((delivery.trigger_kind = 'todo' AND active.trigger_kind = 'todo')
                           OR (delivery.trigger_kind != 'todo'
                               AND active.trigger_kind != 'todo'))
                  )
                ORDER BY delivery.created_at_unix_ms, delivery.id
                LIMIT 1
                "#,
                params![owner_user_id, team_id],
                |row| {
                    Ok(PendingTeamDelivery {
                        delivery_id: row.get(0)?,
                        message_id: row.get(1)?,
                        content: row.get(2)?,
                        trigger_kind: row.get(3)?,
                        todo_id: row.get(4)?,
                        execution_plan: row
                            .get::<_, Option<String>>(5)?
                            .map(|value| serde_json::from_str(&value))
                            .transpose()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        binding: TeamExecutionBinding {
                            agent: AgentExecutionBinding {
                                room_id: row.get(6)?,
                                agent_id: row.get(7)?,
                                name: row.get(8)?,
                                role_prompt: row.get(9)?,
                                model_config_id: row.get(10)?,
                                thinking_level: row.get(11)?,
                                profession_key: row.get(12)?,
                                default_plugin_ids: parse_identifier_array(
                                    &row.get::<_, String>(13)?,
                                )
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                                default_skill_ids: parse_identifier_array(
                                    &row.get::<_, String>(14)?,
                                )
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            },
                            project_id: row.get(15)?,
                            team_name: row.get(16)?,
                            team_goal: row.get(17)?,
                            team_role: row.get(18)?,
                            team_responsibility: row.get(19)?,
                            is_project_manager: row.get::<_, i64>(20)? == 1,
                        },
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?;
        let Some(pending) = pending else {
            return Ok(None);
        };
        validate_identifier(&pending.delivery_id).map_err(|_| team_store_error())?;
        validate_identifier(&pending.message_id).map_err(|_| team_store_error())?;
        if !matches!(
            pending.trigger_kind.as_str(),
            "mention" | "default_agent" | "agent_mention" | "heartbeat" | "todo" | "todo_status"
        ) {
            return Err(team_store_error());
        }
        match (&pending.trigger_kind[..], pending.todo_id.as_deref()) {
            ("todo", Some(todo_id)) => {
                validate_identifier(todo_id).map_err(|_| team_store_error())?;
                let plan = pending
                    .execution_plan
                    .as_ref()
                    .ok_or_else(team_store_error)?;
                if !plan.plugins.is_empty() {
                    return Err("团队 Todo 当前只能调度已接入的项目与终端能力。".into());
                }
            }
            ("todo", None) | (_, Some(_)) => return Err(team_store_error()),
            _ => {}
        }
        if pending.trigger_kind != "todo" && pending.execution_plan.is_some() {
            return Err(team_store_error());
        }
        validate_team_execution_binding(&pending.binding)?;
        validate_text(&pending.content, 1_000_000, false).map_err(|_| team_store_error())?;
        Ok(Some(pending))
    }

    fn load_project_team_execution_binding(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<TeamExecutionBinding, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = connection
            .query_row(
                r#"
                SELECT room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json, room.project_id, room.name, room.goal,
                       member.role, member.responsibility,
                       CASE WHEN room.project_manager_agent_id = profile.id THEN 1 ELSE 0 END
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id AND profile.id = member.agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2 AND profile.id = ?3
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                  AND member.status = 'active' AND profile.status = 'active'
                "#,
                params![owner_user_id, team_id, agent_id],
                |row| {
                    Ok(TeamExecutionBinding {
                        agent: AgentExecutionBinding {
                            room_id: row.get(0)?,
                            agent_id: row.get(1)?,
                            name: row.get(2)?,
                            role_prompt: row.get(3)?,
                            model_config_id: row.get(4)?,
                            thinking_level: row.get(5)?,
                            profession_key: row.get(6)?,
                            default_plugin_ids: parse_identifier_array(&row.get::<_, String>(7)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            default_skill_ids: parse_identifier_array(&row.get::<_, String>(8)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        },
                        project_id: row.get(9)?,
                        team_name: row.get(10)?,
                        team_goal: row.get(11)?,
                        team_role: row.get(12)?,
                        team_responsibility: row.get(13)?,
                        is_project_manager: row.get::<_, i64>(14)? == 1,
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 不是当前团队的可执行活动成员。".to_owned())?;
        validate_team_execution_binding(&binding)?;
        Ok(binding)
    }

    pub(crate) fn validate_team_executor_binding(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        let binding = self.load_project_team_execution_binding(owner_user_id, team_id, agent_id)?;
        if binding.project_id != project_id {
            return Err("The team executor binding is no longer active for this Project.".into());
        }
        Ok(())
    }

    fn claim_project_team_delivery(
        &self,
        owner_user_id: &str,
        pending: &PendingTeamDelivery,
        turn_id: &str,
        run_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        validate_identifier(turn_id)?;
        validate_identifier(run_id)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let changed = transaction
            .execute(
                "UPDATE project_agent_deliveries SET status = 'running', attempt = attempt + 1, \
                 claimed_at_unix_ms = ?1, last_error = NULL WHERE owner_user_id = ?2 AND id = ?3 \
                 AND room_id = ?4 AND target_agent_id = ?5 AND status = 'pending' \
                 AND NOT EXISTS (SELECT 1 FROM project_agent_deliveries active \
                   WHERE active.owner_user_id = ?2 AND active.target_agent_id = ?5 \
                     AND active.status = 'running' AND active.id != ?3 \
                     AND ((project_agent_deliveries.trigger_kind = 'todo' \
                           AND active.trigger_kind = 'todo') \
                          OR (project_agent_deliveries.trigger_kind != 'todo' \
                              AND active.trigger_kind != 'todo'))) ",
                params![
                    now_unix_ms,
                    owner_user_id,
                    pending.delivery_id,
                    pending.binding.agent.room_id,
                    pending.binding.agent.agent_id,
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent Delivery 已被其他执行器领取，请刷新后重试。".into());
        }
        transaction
            .execute(
                "INSERT INTO windows_agent_delivery_runs (owner_user_id, delivery_id, turn_id, \
                 run_id, status, created_at_unix_ms, updated_at_unix_ms) \
                 VALUES (?1, ?2, ?3, ?4, 'ready', ?5, ?5)",
                params![
                    owner_user_id,
                    pending.delivery_id,
                    turn_id,
                    run_id,
                    now_unix_ms
                ],
            )
            .map_err(|_| team_store_error())?;
        if pending.trigger_kind == "todo" {
            let todo_id = pending.todo_id.as_deref().ok_or_else(team_store_error)?;
            team_todo::insert_todo_progress(
                &transaction,
                owner_user_id,
                todo_id,
                run_id,
                "started",
                "started",
                "Todo 执行线程已启动，并完成团队、项目和能力计划绑定。",
                now_unix_ms,
            )?;
        }
        transaction.commit().map_err(|_| team_store_error())
    }

    fn fail_project_team_delivery(
        &self,
        owner_user_id: &str,
        delivery_id: &str,
        failure: &str,
    ) -> Result<(), String> {
        validate_identifier(delivery_id)?;
        let message: String = failure.chars().take(2_000).collect();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        transaction
            .execute(
                "UPDATE project_agent_deliveries SET status = 'failed', last_error = ?1, \
                 completed_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
                   AND status = 'running'",
                params![message, now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        transaction
            .execute(
                "UPDATE windows_agent_delivery_runs SET status = 'failed', updated_at_unix_ms = ?1 \
                 WHERE owner_user_id = ?2 AND delivery_id = ?3 AND status IN ('ready', 'running')",
                params![now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET status = 'blocked', blocked_reason = ?1,
                    updated_at_unix_ms = CASE
                      WHEN updated_at_unix_ms >= ?2 THEN updated_at_unix_ms + 1
                      ELSE ?2
                    END
                WHERE owner_user_id = ?3 AND status = 'in_progress'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_deliveries delivery
                    WHERE delivery.owner_user_id = local_agent_todos.owner_user_id
                      AND delivery.id = ?4 AND delivery.trigger_kind = 'todo'
                      AND delivery.target_agent_id = local_agent_todos.agent_id
                      AND (delivery.deduplication_key = 'todo:' || local_agent_todos.id
                           OR delivery.deduplication_key LIKE
                              'todo:' || local_agent_todos.id || ':attempt:%')
                  )
                "#,
                params![message, now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())
    }

    fn record_direct_delivery(
        &self,
        owner_user_id: &str,
        binding: &AgentExecutionBinding,
        delivery_id: &str,
        message_id: &str,
        turn_id: &str,
        run_id: &str,
        content: &str,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        for value in [delivery_id, message_id, turn_id, run_id] {
            validate_identifier(value)?;
        }
        validate_text(content, 1_000_000, true)?;
        if now_unix_ms < 0 {
            return Err(store_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, 'human', ?1, ?4, NULL, NULL, NULL, ?2, 0, ?5)
                "#,
                params![
                    owner_user_id,
                    message_id,
                    binding.room_id,
                    content,
                    now_unix_ms,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_deliveries (
                    owner_user_id, id, room_id, message_id, root_message_id,
                    target_agent_id, trigger_kind, status, attempt, hop_count,
                    deduplication_key, response_message_id, last_error,
                    claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 'default_agent', 'pending', 0, 0,
                          ?6, NULL, NULL, NULL, NULL, ?7)
                "#,
                params![
                    owner_user_id,
                    delivery_id,
                    binding.room_id,
                    message_id,
                    binding.agent_id,
                    format!("{message_id}:{}", binding.agent_id),
                    now_unix_ms,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO windows_agent_delivery_runs (
                    owner_user_id, delivery_id, turn_id, run_id, status,
                    created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, 'ready', ?5, ?5)
                "#,
                params![owner_user_id, delivery_id, turn_id, run_id, now_unix_ms],
            )
            .map_err(|_| store_error())?;
        transaction.commit().map_err(|_| store_error())
    }

    fn reconcile_direct_activity(
        &self,
        owner_user_id: &str,
        binding: &AgentExecutionBinding,
        detail: &LocalConversationDetail,
    ) -> Result<Vec<AgentConversationActivityItem>, String> {
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        let mappings = {
            let mut statement = transaction
                .prepare(
                    r#"
                    SELECT delivery.id, delivery.message_id, run.turn_id, run.run_id,
                           delivery.status
                    FROM project_agent_deliveries delivery
                    JOIN windows_agent_delivery_runs run
                      ON run.owner_user_id = delivery.owner_user_id
                     AND run.delivery_id = delivery.id
                    WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                      AND delivery.target_agent_id = ?3
                    ORDER BY delivery.created_at_unix_ms, delivery.id
                    "#,
                )
                .map_err(|_| store_error())?;
            let rows = statement
                .query_map(
                    params![owner_user_id, binding.room_id, binding.agent_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .map_err(|_| store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| store_error())?
        };
        for (delivery_id, root_message_id, turn_id, run_id, current_status) in mappings {
            let Some(turn) = detail
                .turns
                .iter()
                .find(|candidate| candidate.turn_id == turn_id && candidate.run_id == run_id)
            else {
                continue;
            };
            let (delivery_status, completed_at) = delivery_state(turn);
            let response = if turn.status == LocalConversationTurnStatus::Succeeded {
                detail
                    .messages
                    .iter()
                    .filter(|message| {
                        message.turn_id == turn_id
                            && message.role == LocalConversationMessageRole::Assistant
                    })
                    .max_by_key(|message| message.ordinal)
            } else {
                None
            };
            let response_message_id = if let Some(message) = response {
                validate_identifier(&message.message_id).map_err(|_| store_error())?;
                let content = projected_message_text(&message.content);
                transaction
                    .execute(
                        r#"
                        INSERT OR IGNORE INTO project_agent_messages (
                            owner_user_id, id, room_id, sender_kind, sender_id, content,
                            reply_to_message_id, source_run_id, causation_id, root_message_id,
                            hop_count, created_at_unix_ms
                        ) VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?6, ?7, ?6, ?6, 1, ?8)
                        "#,
                        params![
                            owner_user_id,
                            message.message_id,
                            binding.room_id,
                            binding.agent_id,
                            content,
                            root_message_id,
                            run_id,
                            message.created_at_unix_ms,
                        ],
                    )
                    .map_err(|_| store_error())?;
                Some(message.message_id.as_str())
            } else {
                None
            };
            if matches!(current_status.as_str(), "pending" | "running") {
                transaction
                    .execute(
                        r#"
                        UPDATE project_agent_deliveries
                        SET status = ?1,
                            attempt = CASE WHEN attempt = 0 THEN 1 ELSE attempt END,
                            response_message_id = COALESCE(?2, response_message_id),
                            claimed_at_unix_ms = COALESCE(claimed_at_unix_ms, ?3),
                            completed_at_unix_ms = ?4
                        WHERE owner_user_id = ?5 AND id = ?6
                          AND status IN ('pending', 'running')
                        "#,
                        params![
                            delivery_status,
                            response_message_id,
                            turn.created_at_unix_ms,
                            completed_at,
                            owner_user_id,
                            delivery_id,
                        ],
                    )
                    .map_err(|_| store_error())?;
            }
            transaction
                .execute(
                    r#"
                    UPDATE windows_agent_delivery_runs
                    SET status = ?1, updated_at_unix_ms = ?2
                    WHERE owner_user_id = ?3 AND delivery_id = ?4
                    "#,
                    params![
                        turn.status.as_str(),
                        turn.updated_at_unix_ms,
                        owner_user_id,
                        delivery_id,
                    ],
                )
                .map_err(|_| store_error())?;
        }
        transaction.commit().map_err(|_| store_error())?;
        self.list_direct_activity(owner_user_id, binding)
    }

    fn list_direct_activity(
        &self,
        owner_user_id: &str,
        binding: &AgentExecutionBinding,
    ) -> Result<Vec<AgentConversationActivityItem>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.room_id, delivery.target_agent_id,
                       run.turn_id, run.run_id, delivery.status, run.status,
                       delivery.response_message_id, delivery.created_at_unix_ms,
                       run.updated_at_unix_ms
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.target_agent_id = ?3
                ORDER BY run.updated_at_unix_ms DESC, run.run_id DESC
                LIMIT 100
                "#,
            )
            .map_err(|_| store_error())?;
        let rows = statement
            .query_map(
                params![owner_user_id, binding.room_id, binding.agent_id],
                |row| {
                    Ok(AgentConversationActivityItem {
                        delivery_id: row.get(0)?,
                        room_id: row.get(1)?,
                        agent_id: row.get(2)?,
                        turn_id: row.get(3)?,
                        run_id: row.get(4)?,
                        delivery_status: row.get(5)?,
                        run_status: row.get(6)?,
                        response_message_id: row.get(7)?,
                        created_at_unix_ms: row.get(8)?,
                        updated_at_unix_ms: row.get(9)?,
                    })
                },
            )
            .map_err(|_| store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| store_error())
    }

    #[cfg(test)]
    fn database(&self) -> &Path {
        &self.database
    }
}

#[tauri::command]
pub(crate) async fn prepare_agent_conversation(
    owner_user_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<PreparedAgentConversation, String> {
    validate_owner(&owner_user_id)?;
    validate_agent_identifier(&agent_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let requested_agent = agent_id.clone();
    let room = tauri::async_runtime::spawn_blocking(move || {
        store.open_human_agent_direct(&owner, &requested_agent)
    })
    .await
    .map_err(|_| "Agent 私聊准备任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;

    // The Host conversation is deliberately unbound from Contact and Project resources. The
    // account-scoped Agent room above remains the authority that binds this transcript to exactly
    // one Agent; a normal contact can therefore never impersonate an Agent direct conversation.
    let conversation_id = format!("agent-direct-{}", room.id);
    let existing = manager
        .execute_detailed(HostCommand::GetConversation(GetConversationCommand {
            owner_user_id: owner_user_id.clone(),
            conversation_id: conversation_id.clone(),
        }))
        .await;
    match existing {
        Ok(HostResult::Conversation { conversation }) => {
            validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.clone(),
                    owner_user_id: owner_user_id.clone(),
                    title: room.name.clone(),
                    resource: None,
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
                }
                Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
                Err(create_error) => {
                    // A concurrent prepare may have won after our not-found read. Re-read the
                    // deterministic conversation identity before surfacing the conflict.
                    let retry = manager
                        .execute(HostCommand::GetConversation(GetConversationCommand {
                            owner_user_id: owner_user_id.clone(),
                            conversation_id: conversation_id.clone(),
                        }))
                        .await;
                    match retry {
                        Ok(HostResult::Conversation { conversation }) => {
                            validate_host_conversation(
                                &conversation,
                                &owner_user_id,
                                &conversation_id,
                            )?;
                        }
                        _ => return Err(create_error),
                    }
                }
            }
        }
        Err(error) => return Err(error.message),
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(PreparedAgentConversation {
        room_id: room.id,
        agent_id: room.agent_id,
        conversation_id,
        title: room.name,
        created_at_unix_ms: room.created_at_unix_ms,
        updated_at_unix_ms: room.updated_at_unix_ms,
    })
}

#[tauri::command]
pub(crate) async fn load_agent_conversation_activity(
    owner_user_id: String,
    room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentConversationActivitySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&room_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_room = room_id.clone();
    let binding = tauri::async_runtime::spawn_blocking(move || {
        store.load_execution_binding(&binding_owner, &binding_room)
    })
    .await
    .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let conversation_id = format!("agent-direct-{}", binding.room_id);
    let detail = agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let conversation_version = detail.conversation.version;
    let activity_store = conversations.inner().clone();
    let activity_owner = owner_user_id.clone();
    let activity_binding = binding.clone();
    let items = tauri::async_runtime::spawn_blocking(move || {
        activity_store.reconcile_direct_activity(&activity_owner, &activity_binding, &detail)
    })
    .await
    .map_err(|_| "Agent 运行动态读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(AgentConversationActivitySnapshot {
        room_id: binding.room_id,
        agent_id: binding.agent_id,
        conversation_id,
        conversation_version,
        has_active_delivery: items
            .iter()
            .any(|item| matches!(item.delivery_status.as_str(), "pending" | "running")),
        items,
    })
}

#[tauri::command]
pub(crate) async fn load_agent_team_member_activity(
    owner_user_id: String,
    team_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentConversationActivitySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&agent_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_team = team_id.clone();
    let binding_agent = agent_id.clone();
    let binding = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team_execution_binding(&binding_owner, &binding_team, &binding_agent)
    })
    .await
    .map_err(|_| "团队 Agent 执行身份读取任务异常退出。".to_owned())??;
    let conversation_id = format!(
        "agent-team-{}-{}",
        binding.agent.room_id, binding.agent.agent_id
    );
    let host_result = manager
        .execute_detailed(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.clone(),
                conversation_id: conversation_id.clone(),
            },
        ))
        .await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let activity_store = conversations.inner().clone();
    let activity_owner = owner_user_id.clone();
    let activity_binding = binding.agent.clone();
    let (conversation_version, items) = match host_result {
        Ok(HostResult::Conversation { conversation }) => {
            validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
            let version = conversation.conversation.version;
            let items = tauri::async_runtime::spawn_blocking(move || {
                activity_store.reconcile_direct_activity(
                    &activity_owner,
                    &activity_binding,
                    &conversation,
                )
            })
            .await
            .map_err(|_| "团队 Agent 运行动态读取任务异常退出。".to_owned())??;
            (version, items)
        }
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let items = tauri::async_runtime::spawn_blocking(move || {
                activity_store.list_direct_activity(&activity_owner, &activity_binding)
            })
            .await
            .map_err(|_| "团队 Agent 运行动态读取任务异常退出。".to_owned())??;
            (0, items)
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的团队 Agent 会话响应。".into()),
        Err(error) => return Err(error.message),
    };
    Ok(AgentConversationActivitySnapshot {
        room_id: binding.agent.room_id,
        agent_id: binding.agent.agent_id,
        conversation_id,
        conversation_version,
        has_active_delivery: items
            .iter()
            .any(|item| matches!(item.delivery_status.as_str(), "pending" | "running")),
        items,
    })
}

#[tauri::command]
pub(crate) async fn load_agent_profile_activity(
    owner_user_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentProfileActivitySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_agent_identifier(&agent_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let room_store = conversations.inner().clone();
    let room_owner = owner_user_id.clone();
    let room_agent = agent_id.clone();
    let room_ids = tauri::async_runtime::spawn_blocking(move || {
        room_store.activity_room_ids(&room_owner, &room_agent)
    })
    .await
    .map_err(|_| "Agent 运行 Room 查询任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;

    let mut items = Vec::new();
    for room_id in room_ids {
        let binding_store = conversations.inner().clone();
        let binding_owner = owner_user_id.clone();
        let binding_room = room_id.clone();
        let binding = tauri::async_runtime::spawn_blocking(move || {
            binding_store.load_execution_binding(&binding_owner, &binding_room)
        })
        .await
        .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
        if binding.agent_id != agent_id {
            return Err("Agent 运行动态成员身份不一致。".into());
        }
        let conversation_id = format!("agent-direct-{}", binding.room_id);
        let result = manager
            .execute_detailed(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: owner_user_id.clone(),
                    conversation_id: conversation_id.clone(),
                },
            ))
            .await;
        let detail = match result {
            Ok(HostResult::Conversation { conversation }) => {
                validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
                conversation
            }
            Err(error) if error.code.as_deref() == Some("not_found") => continue,
            Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
            Err(error) => return Err(error.message),
        };
        authentication
            .require_session_generation(&owner_user_id, generation)
            .await?;
        let activity_store = conversations.inner().clone();
        let activity_owner = owner_user_id.clone();
        let room_items = tauri::async_runtime::spawn_blocking(move || {
            activity_store.reconcile_direct_activity(&activity_owner, &binding, &detail)
        })
        .await
        .map_err(|_| "Agent 运行动态读取任务异常退出。".to_owned())??;
        items.extend(room_items);
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    items.sort_by(|left, right| {
        right
            .updated_at_unix_ms
            .cmp(&left.updated_at_unix_ms)
            .then_with(|| right.run_id.cmp(&left.run_id))
    });
    items.truncate(100);
    Ok(AgentProfileActivitySnapshot {
        agent_id,
        has_active_delivery: items
            .iter()
            .any(|item| matches!(item.delivery_status.as_str(), "pending" | "running")),
        items,
    })
}

#[tauri::command]
pub(crate) async fn list_agent_teams(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamSummary>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let teams = tauri::async_runtime::spawn_blocking(move || store.list_project_teams(&owner))
        .await
        .map_err(|_| "Agent 团队读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(teams)
}

#[tauri::command]
pub(crate) async fn create_agent_team(
    app: AppHandle,
    input: CreateAgentTeamInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamSummary, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.project_id).map_err(|_| invalid_team_input())?;
    validate_text(&input.name, 160, false).map_err(|_| invalid_team_input())?;
    validate_text(&input.goal, 8_000, true).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "项目注册表不可用。".to_owned())?;
    let project_database = data_dir.join("NativeConnector").join("Projects.sqlite3");
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let project_id = input.project_id.clone();
    let name = input.name.clone();
    let goal = input.goal.clone();
    let team = tauri::async_runtime::spawn_blocking(move || {
        require_active_project(&project_database, &owner, &project_id)?;
        store.create_project_team(&owner, &project_id, &name, &goal)
    })
    .await
    .map_err(|_| "Agent 团队创建任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(team)
}

#[tauri::command]
pub(crate) async fn load_agent_team(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let selected_team = team_id.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team(&owner, &selected_team)
    })
    .await
    .map_err(|_| "Agent 团队成员读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(detail)
}

#[tauri::command]
pub(crate) async fn load_agent_team_conversation(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamConversationSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team_conversation(&owner, &team)
    })
    .await
    .map_err(|_| "Agent 团队消息读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn load_agent_team_runs(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<AgentTeamRunHistorySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let binding_store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_team = team_id.clone();
    let bindings = tauri::async_runtime::spawn_blocking(move || {
        binding_store.active_project_team_run_bindings(&binding_owner, &binding_team)
    })
    .await
    .map_err(|_| "Agent 团队运行绑定读取任务异常退出。".to_owned())??;
    let mut reconciliations = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let result = manager
            .execute_detailed(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: owner_user_id.clone(),
                    conversation_id: binding.conversation_id.clone(),
                },
            ))
            .await;
        let detail = match result {
            Ok(HostResult::Conversation { conversation }) => conversation,
            Ok(_) => return Err("Local Agent Host 返回了错误的团队运行响应。".into()),
            Err(error) if error.code.as_deref() == Some("not_found") => continue,
            Err(error) => return Err(error.message),
        };
        validate_host_conversation(&detail, &owner_user_id, &binding.conversation_id)?;
        let Some(turn) = detail
            .turns
            .iter()
            .find(|turn| turn.turn_id == binding.turn_id && turn.run_id == binding.run_id)
        else {
            continue;
        };
        reconciliations.push(team_todo::TeamRunReconciliation {
            delivery_id: binding.delivery_id,
            turn_id: turn.turn_id.clone(),
            run_id: turn.run_id.clone(),
            status: turn.status,
            created_at_unix_ms: turn.created_at_unix_ms,
            updated_at_unix_ms: turn.updated_at_unix_ms,
            project_write_evidence_verified: terminal_worker
                .project_write_completion_verified(&owner_user_id, &turn.run_id),
        });
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    if !reconciliations.is_empty() {
        let reconcile_store = conversations.inner().clone();
        let reconcile_owner = owner_user_id.clone();
        let reconcile_team = team_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            reconcile_store.reconcile_project_team_runs(
                &reconcile_owner,
                &reconcile_team,
                &reconciliations,
            )
        })
        .await
        .map_err(|_| "Agent 团队运行对账任务异常退出。".to_owned())??;
    }
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || store.load_project_team_runs(&owner, &team))
            .await
            .map_err(|_| "Agent 团队运行记录读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}

pub(crate) async fn reconcile_agent_communication_runs(
    owner_user_id: String,
    room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&room_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let binding_store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_room = room_id.clone();
    let bindings = tauri::async_runtime::spawn_blocking(move || {
        binding_store.active_agent_communication_run_bindings(&binding_owner, &binding_room)
    })
    .await
    .map_err(|_| "Agent 通讯运行绑定读取任务异常退出。".to_owned())??;
    let mut reconciliations = Vec::with_capacity(bindings.len());
    let mut orphaned_deliveries = Vec::new();
    for binding in bindings {
        let result = manager
            .execute_detailed(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: owner_user_id.clone(),
                    conversation_id: binding.conversation_id.clone(),
                },
            ))
            .await;
        let detail = match result {
            Ok(HostResult::Conversation { conversation }) => conversation,
            Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 通讯运行响应。".into()),
            Err(error) if error.code.as_deref() == Some("not_found") => {
                orphaned_deliveries.push(binding.delivery_id);
                continue;
            }
            Err(error) => return Err(error.message),
        };
        validate_host_conversation(&detail, &owner_user_id, &binding.conversation_id)?;
        let Some(turn) = detail
            .turns
            .iter()
            .find(|turn| turn.turn_id == binding.turn_id && turn.run_id == binding.run_id)
        else {
            orphaned_deliveries.push(binding.delivery_id);
            continue;
        };
        reconciliations.push(AgentCommunicationRunReconciliation {
            delivery_id: binding.delivery_id,
            turn_id: turn.turn_id.clone(),
            run_id: turn.run_id.clone(),
            status: turn.status,
            created_at_unix_ms: turn.created_at_unix_ms,
            updated_at_unix_ms: turn.updated_at_unix_ms,
        });
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    if !reconciliations.is_empty() {
        let reconcile_store = conversations.inner().clone();
        let reconcile_owner = owner_user_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            reconcile_store.reconcile_agent_communication_runs(
                &reconcile_owner,
                &room_id,
                &reconciliations,
            )
        })
        .await
        .map_err(|_| "Agent 通讯运行对账任务异常退出。".to_owned())??;
    }
    for delivery_id in orphaned_deliveries {
        let failure_store = conversations.inner().clone();
        let failure_owner = owner_user_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            failure_store.fail_project_team_delivery(
                &failure_owner,
                &delivery_id,
                "Agent 通讯 Delivery 的 Host Turn 不存在，恢复时已终止该运行。",
            )
        })
        .await
        .map_err(|_| "Agent 通讯孤立 Delivery 清理任务异常退出。".to_owned())??;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn list_agent_team_assets(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamAsset>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let assets =
        tauri::async_runtime::spawn_blocking(move || store.list_project_team_assets(&owner, &team))
            .await
            .map_err(|_| "团队共享资产读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(assets)
}

#[tauri::command]
pub(crate) async fn save_agent_team_asset(
    input: SaveAgentTeamAssetInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamAsset, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    if let Some(asset_id) = input.asset_id.as_deref() {
        validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
    }
    validate_team_asset_category(&input.category)?;
    let title = input.title.trim().to_owned();
    validate_text(&title, 240, false).map_err(|_| invalid_team_asset_input())?;
    validate_text(&input.markdown, 128_000, true).map_err(|_| invalid_team_asset_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let asset_id = input.asset_id.clone();
    let category = input.category.clone();
    let markdown = input.markdown.clone();
    let expected_revision = input.expected_revision;
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let asset = tauri::async_runtime::spawn_blocking(move || {
        store.save_project_team_asset(
            &owner,
            &team,
            asset_id.as_deref(),
            &category,
            &title,
            &markdown,
            expected_revision,
            now,
        )
    })
    .await
    .map_err(|_| "团队共享资产保存任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(asset)
}

#[tauri::command]
pub(crate) async fn archive_agent_team_asset(
    input: ArchiveAgentTeamAssetInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamAsset, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&input.asset_id).map_err(|_| invalid_team_asset_input())?;
    if input.expected_revision <= 0 {
        return Err(invalid_team_asset_input());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let asset_id = input.asset_id.clone();
    let revision = input.expected_revision;
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let asset = tauri::async_runtime::spawn_blocking(move || {
        store.archive_project_team_asset(&owner, &team, &asset_id, revision, now)
    })
    .await
    .map_err(|_| "团队共享资产归档任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(asset)
}

#[tauri::command]
pub(crate) async fn list_agent_team_asset_revisions(
    owner_user_id: String,
    team_id: String,
    asset_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamAssetRevision>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&asset_id).map_err(|_| invalid_team_asset_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let selected_asset = asset_id.clone();
    let revisions = tauri::async_runtime::spawn_blocking(move || {
        store.list_project_team_asset_revisions(&owner, &team, &selected_asset)
    })
    .await
    .map_err(|_| "团队共享资产版本读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(revisions)
}

#[tauri::command]
pub(crate) async fn invite_agent_to_team(
    input: InviteAgentToTeamInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&input.agent_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let agent = input.agent_id.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        store.invite_agent_to_project_team(&owner, &team, &agent)
    })
    .await
    .map_err(|_| "Agent 团队邀请任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(detail)
}

#[tauri::command]
pub(crate) async fn update_agent_team_member(
    input: UpdateAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&input.agent_id).map_err(|_| invalid_team_input())?;
    validate_text(&input.role, 160, false).map_err(|_| invalid_team_input())?;
    validate_text(&input.responsibility, 8_000, true).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let agent = input.agent_id.clone();
    let role = input.role.clone();
    let responsibility = input.responsibility.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        store.update_project_team_member(&owner, &team, &agent, &role, &responsibility)
    })
    .await
    .map_err(|_| "Agent 团队成员保存任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(detail)
}

async fn manage_agent_team_member(
    input: ManageAgentTeamMemberInput,
    authentication: &AuthenticationManager,
    conversations: &AgentConversationStore,
    operation: fn(&AgentConversationStore, &str, &str, &str) -> Result<AgentTeamDetail, String>,
    task_error: &'static str,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&input.agent_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let agent = input.agent_id.clone();
    let detail =
        tauri::async_runtime::spawn_blocking(move || operation(&store, &owner, &team, &agent))
            .await
            .map_err(|_| task_error.to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(detail)
}

#[tauri::command]
pub(crate) async fn set_agent_team_default_agent(
    input: ManageAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    manage_agent_team_member(
        input,
        authentication.inner(),
        conversations.inner(),
        AgentConversationStore::set_project_team_default_agent,
        "默认 Agent 设置任务异常退出。",
    )
    .await
}

#[tauri::command]
pub(crate) async fn set_agent_team_project_manager(
    input: ManageAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    manage_agent_team_member(
        input,
        authentication.inner(),
        conversations.inner(),
        AgentConversationStore::set_project_team_manager,
        "项目经理设置任务异常退出。",
    )
    .await
}

#[tauri::command]
pub(crate) async fn remove_agent_from_team(
    input: ManageAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    manage_agent_team_member(
        input,
        authentication.inner(),
        conversations.inner(),
        AgentConversationStore::remove_agent_from_project_team,
        "Agent 移出团队任务异常退出。",
    )
    .await
}

#[tauri::command]
pub(crate) async fn queue_agent_team_message(
    input: QueueAgentTeamMessageInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<QueueAgentTeamMessageResult, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    let content = input.content.trim().to_owned();
    validate_text(&content, 1_000_000, false).map_err(|_| invalid_team_message())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let mentions = input.mentioned_agent_ids.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        store.queue_project_team_message(&owner, &team, &content, &mentions)
    })
    .await
    .map_err(|_| "Agent 团队消息入队任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn dispatch_next_agent_team_delivery(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    manager: State<'_, HostManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<Option<DispatchedAgentTeamDelivery>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let enqueue_store = conversations.inner().clone();
    let enqueue_owner = owner_user_id.clone();
    let enqueue_team = team_id.clone();
    let enqueue_now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        enqueue_store.enqueue_ready_project_team_todos(&enqueue_owner, &enqueue_team, enqueue_now)
    })
    .await
    .map_err(|_| "团队 Todo 调度任务异常退出。".to_owned())??;
    let store = conversations.inner().clone();
    let pending_owner = owner_user_id.clone();
    let pending_team = team_id.clone();
    let Some(pending) = tauri::async_runtime::spawn_blocking(move || {
        store.next_project_team_delivery(&pending_owner, &pending_team)
    })
    .await
    .map_err(|_| "Agent Delivery 读取任务异常退出。".to_owned())??
    else {
        return Ok(None);
    };
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let skill_owner = owner_user_id.clone();
    let profession_key = pending.binding.agent.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    let conversation_id = if pending.trigger_kind == "todo" {
        format!("agent-team-executor-{}", pending.delivery_id)
    } else {
        format!(
            "agent-team-{}-{}",
            pending.binding.agent.room_id, pending.binding.agent.agent_id
        )
    };
    let title = format!(
        "{} · {}",
        pending.binding.team_name, pending.binding.agent.name
    );
    let detail = if pending.trigger_kind == "todo" {
        ensure_team_executor_host_conversation(
            &manager,
            &owner_user_id,
            &conversation_id,
            &title,
            &pending.binding.project_id,
        )
        .await?
    } else {
        ensure_agent_host_conversation(&manager, &owner_user_id, &conversation_id, &title).await?;
        agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?
    };
    let model = selected_agent_model(
        &manager,
        &owner_user_id,
        &pending.binding.agent.model_config_id,
    )
    .await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &owner_user_id,
        &conversation_id,
        &model,
        pending.binding.agent.thinking_level.as_deref(),
    )
    .await?;
    let profile_key = if pending.trigger_kind == "todo" {
        "task_execution"
    } else {
        MAIN_CHAT_PROFILE
    };
    let capability_revision =
        latest_capability_revision(&manager, &owner_user_id, profile_key).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let run_id = identifier("run");
    let host_message_id = identifier("message");
    let command = StartConversationTurnCommand {
        owner_user_id: owner_user_id.clone(),
        conversation_id,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: host_message_id.clone(),
        run_id: run_id.clone(),
        message: pending.content.clone(),
        message_metadata: team_agent_execution_metadata(&pending, &profession)?,
        attachments: vec![],
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    command
        .validate()
        .map_err(|_| "Agent 团队消息或执行快照超过安全限制。".to_owned())?;
    let claim_store = conversations.inner().clone();
    let claim_owner = owner_user_id.clone();
    let claim_pending = pending.clone();
    let claim_turn = turn_id.clone();
    let claim_run = run_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        claim_store.claim_project_team_delivery(
            &claim_owner,
            &claim_pending,
            &claim_turn,
            &claim_run,
            now,
        )
    })
    .await
    .map_err(|_| "Agent Delivery 领取任务异常退出。".to_owned())??;
    let started = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await;
    let result = match started {
        Ok(HostResult::ConversationTurnStarted { result }) => result,
        Ok(_) => {
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                "Local Agent Host 返回了错误的团队 Delivery 响应。".into(),
            )
            .await;
            return Err("Local Agent Host 返回了错误的团队 Delivery 响应。".into());
        }
        Err(failure) => {
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                failure.clone(),
            )
            .await;
            return Err(failure);
        }
    };
    if result.turn.turn_id != turn_id
        || result.turn.run_id != run_id
        || result.turn.user_message_id != host_message_id
        || result.message.message_id != host_message_id
        || result.run.run_id != run_id
    {
        let failure = "Local Agent Host 返回了不匹配的团队 Delivery。".to_owned();
        fail_dispatched_team_delivery(
            conversations.inner().clone(),
            owner_user_id.clone(),
            pending.delivery_id.clone(),
            failure.clone(),
        )
        .await;
        return Err(failure);
    }
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(Some(DispatchedAgentTeamDelivery {
        delivery_id: pending.delivery_id,
        team_id: pending.binding.agent.room_id,
        agent_id: pending.binding.agent.agent_id,
        turn_id,
        run_id,
    }))
}

pub(crate) async fn dispatch_next_agent_communication_delivery(
    owner_user_id: String,
    room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    manager: State<'_, HostManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<bool, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&room_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let pending_owner = owner_user_id.clone();
    let pending_room = room_id.clone();
    let Some(pending) = tauri::async_runtime::spawn_blocking(move || {
        store.next_agent_communication_delivery(&pending_owner, &pending_room)
    })
    .await
    .map_err(|_| "Agent 通讯 Delivery 读取任务异常退出。".to_owned())??
    else {
        return Ok(false);
    };
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let skill_owner = owner_user_id.clone();
    let profession_key = pending.binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    let conversation_id =
        agent_communication_conversation_id(&pending.binding.room_id, &pending.binding.agent_id);
    let title = format!("{} · {}", pending.room_name, pending.binding.name);
    ensure_agent_host_conversation(&manager, &owner_user_id, &conversation_id, &title).await?;
    let detail = agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?;
    let model =
        selected_agent_model(&manager, &owner_user_id, &pending.binding.model_config_id).await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &owner_user_id,
        &conversation_id,
        &model,
        pending.binding.thinking_level.as_deref(),
    )
    .await?;
    let capability_revision =
        latest_capability_revision(&manager, &owner_user_id, MAIN_CHAT_PROFILE).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let run_id = identifier("run");
    let host_message_id = identifier("message");
    let command = StartConversationTurnCommand {
        owner_user_id: owner_user_id.clone(),
        conversation_id,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: host_message_id.clone(),
        run_id: run_id.clone(),
        message: pending.content.clone(),
        message_metadata: agent_communication_execution_metadata(&pending, &profession),
        attachments: vec![],
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    command
        .validate()
        .map_err(|_| "Agent 通讯消息或执行快照超过安全限制。".to_owned())?;
    let claim_store = conversations.inner().clone();
    let claim_owner = owner_user_id.clone();
    let claim_pending = pending.clone();
    let claim_turn = turn_id.clone();
    let claim_run = run_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        claim_store.claim_agent_communication_delivery(
            &claim_owner,
            &claim_pending,
            &claim_turn,
            &claim_run,
            now,
        )
    })
    .await
    .map_err(|_| "Agent 通讯 Delivery 领取任务异常退出。".to_owned())??;
    let started = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await;
    let result = match started {
        Ok(HostResult::ConversationTurnStarted { result }) => result,
        Ok(_) => {
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                "Local Agent Host 返回了错误的 Agent 通讯 Delivery 响应。".into(),
            )
            .await;
            return Err("Local Agent Host 返回了错误的 Agent 通讯 Delivery 响应。".into());
        }
        Err(failure) => {
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                failure.clone(),
            )
            .await;
            return Err(failure);
        }
    };
    if result.turn.turn_id != turn_id
        || result.turn.run_id != run_id
        || result.turn.user_message_id != host_message_id
        || result.message.message_id != host_message_id
        || result.run.run_id != run_id
    {
        let failure = "Local Agent Host 返回了不匹配的 Agent 通讯 Delivery。".to_owned();
        fail_dispatched_team_delivery(
            conversations.inner().clone(),
            owner_user_id.clone(),
            pending.delivery_id.clone(),
            failure.clone(),
        )
        .await;
        return Err(failure);
    }
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(true)
}

async fn fail_dispatched_team_delivery(
    store: AgentConversationStore,
    owner_user_id: String,
    delivery_id: String,
    failure: String,
) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        store.fail_project_team_delivery(&owner_user_id, &delivery_id, &failure)
    })
    .await;
}

#[tauri::command]
pub(crate) async fn send_agent_conversation_draft(
    input: SendAgentConversationDraftInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    manager: State<'_, HostManager>,
    attachment_manager: State<'_, AttachmentManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<AgentConversationSendAck, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.room_id)?;
    let content = input.submission.content.trim().to_owned();
    if content.is_empty() && input.submission.attachment_ids.is_empty() {
        return Err("消息或附件不能为空。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let room = input.room_id.clone();
    let binding =
        tauri::async_runtime::spawn_blocking(move || store.load_execution_binding(&owner, &room))
            .await
            .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let skill_owner = input.owner_user_id.clone();
    let profession_key = binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    let conversation_id = format!("agent-direct-{}", binding.room_id);
    let attachment_scope = AttachmentScope::new(&input.owner_user_id, &conversation_id);
    let attachment_ids = input.submission.attachment_ids.clone();
    let attachments =
        attachment_manager.resolve_specs(attachment_scope.clone(), &attachment_ids)?;
    let detail =
        agent_conversation_context(&manager, &input.owner_user_id, &conversation_id).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    if let Some(turn_id) = input.guidance_turn_id.as_deref() {
        if detail.turns.iter().any(|turn| {
            turn.turn_id == turn_id && turn.status == LocalConversationTurnStatus::Running
        }) {
            let ack = send_agent_guidance(
                &manager,
                &input.owner_user_id,
                &conversation_id,
                detail.conversation.version,
                turn_id,
                &content,
                attachments,
            )
            .await?;
            authentication
                .require_session_generation(&input.owner_user_id, generation)
                .await?;
            attachment_manager.commit(attachment_scope, &attachment_ids)?;
            attachment_worker.wake();
            terminal_worker.wake();
            return Ok(ack);
        }
    }

    let model =
        selected_agent_model(&manager, &input.owner_user_id, &binding.model_config_id).await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &input.owner_user_id,
        &conversation_id,
        &model,
        binding.thinking_level.as_deref(),
    )
    .await?;
    let capability_revision =
        latest_capability_revision(&manager, &input.owner_user_id, MAIN_CHAT_PROFILE).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let message_id = identifier("message");
    let run_id = identifier("run");
    let command = StartConversationTurnCommand {
        owner_user_id: input.owner_user_id.clone(),
        conversation_id,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: message_id.clone(),
        run_id: run_id.clone(),
        message: content.clone(),
        message_metadata: agent_execution_metadata(&binding, &profession),
        attachments,
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    command
        .validate()
        .map_err(|_| "Agent 私聊消息或执行快照超过安全限制。".to_owned())?;
    let delivery_store = conversations.inner().clone();
    let delivery_owner = input.owner_user_id.clone();
    let delivery_binding = binding.clone();
    let delivery_content = content.clone();
    let stored_delivery_id = uuid::Uuid::new_v4().to_string().to_lowercase();
    let stored_message_id = message_id.clone();
    let stored_turn_id = turn_id.clone();
    let stored_run_id = run_id.clone();
    let now_unix_ms = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        delivery_store.record_direct_delivery(
            &delivery_owner,
            &delivery_binding,
            &stored_delivery_id,
            &stored_message_id,
            &stored_turn_id,
            &stored_run_id,
            &delivery_content,
            now_unix_ms,
        )
    })
    .await
    .map_err(|_| "Agent Delivery 写入任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await?;
    let HostResult::ConversationTurnStarted { result } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 发送响应。".into());
    };
    if result.turn.turn_id != turn_id
        || result.turn.run_id != run_id
        || result.turn.user_message_id != message_id
        || result.message.message_id != message_id
        || result.message.turn_id != turn_id
        || result.run.run_id != run_id
    {
        return Err("Local Agent Host 返回了不匹配的 Agent Delivery。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    attachment_manager.commit(attachment_scope, &attachment_ids)?;
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(AgentConversationSendAck {
        delivery_kind: "newTurn",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result.message.message_id.clone(),
    })
}

async fn agent_conversation_context(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<LocalConversationDetail, String> {
    let result = manager
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into());
    };
    validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
    Ok(conversation)
}

async fn ensure_agent_host_conversation(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    title: &str,
) -> Result<(), String> {
    let existing = manager
        .execute_detailed(HostCommand::GetConversation(GetConversationCommand {
            owner_user_id: owner_user_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
        }))
        .await;
    match existing {
        Ok(HostResult::Conversation { conversation }) => {
            validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
            Ok(())
        }
        Ok(_) => Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.to_owned(),
                    owner_user_id: owner_user_id.to_owned(),
                    title: title.to_owned(),
                    resource: None,
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_host_conversation(&conversation, owner_user_id, conversation_id)
                }
                Ok(_) => Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
                Err(_) => {
                    let retry =
                        agent_conversation_context(manager, owner_user_id, conversation_id).await?;
                    validate_host_conversation(&retry, owner_user_id, conversation_id)
                }
            }
        }
        Err(error) => Err(error.message),
    }
}

async fn ensure_team_executor_host_conversation(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    title: &str,
    project_id: &str,
) -> Result<LocalConversationDetail, String> {
    validate_identifier(project_id).map_err(|_| invalid_team_input())?;
    let existing = manager
        .execute_detailed(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await;
    match existing {
        Ok(HostResult::Conversation { conversation }) => {
            validate_team_executor_host_conversation(
                &conversation,
                owner_user_id,
                conversation_id,
                project_id,
            )?;
            Ok(conversation)
        }
        Ok(_) => Err("Local Agent Host 返回了错误的 executor 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.to_owned(),
                    owner_user_id: owner_user_id.to_owned(),
                    title: title.to_owned(),
                    resource: Some(LocalConversationResourceBinding {
                        kind: LocalConversationResourceKind::Project,
                        resource_id: project_id.to_owned(),
                    }),
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_team_executor_host_conversation(
                        &conversation,
                        owner_user_id,
                        conversation_id,
                        project_id,
                    )?;
                    Ok(conversation)
                }
                Ok(_) => Err("Local Agent Host 返回了错误的 executor 会话响应。".into()),
                Err(_) => {
                    let retry = manager
                        .execute(HostCommand::GetConversationContext(
                            GetConversationCommand {
                                owner_user_id: owner_user_id.to_owned(),
                                conversation_id: conversation_id.to_owned(),
                            },
                        ))
                        .await?;
                    let HostResult::Conversation { conversation } = retry else {
                        return Err("Local Agent Host 返回了错误的 executor 会话响应。".into());
                    };
                    validate_team_executor_host_conversation(
                        &conversation,
                        owner_user_id,
                        conversation_id,
                        project_id,
                    )?;
                    Ok(conversation)
                }
            }
        }
        Err(error) => Err(error.message),
    }
}

async fn send_agent_guidance(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    expected_conversation_version: u64,
    turn_id: &str,
    content: &str,
    attachments: Vec<LocalConversationAttachmentSpec>,
) -> Result<AgentConversationSendAck, String> {
    let message_id = identifier("message");
    let result = manager
        .execute(HostCommand::GuideConversationTurn(
            GuideConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version,
                turn_id: turn_id.to_owned(),
                expected_run_version: None,
                message_id: message_id.clone(),
                message: content.to_owned(),
                message_metadata: json!({"source": "native_agent_direct_guidance"}),
                attachments,
            },
        ))
        .await?;
    let HostResult::ConversationTurnUpdated { result } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 引导响应。".into());
    };
    Ok(AgentConversationSendAck {
        delivery_kind: "guidance",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result
            .message
            .as_ref()
            .map(|message| message.message_id.clone())
            .unwrap_or(message_id),
    })
}

async fn selected_agent_model(
    manager: &HostManager,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<LocalModelConfigSnapshot, String> {
    let result = manager
        .execute(HostCommand::ListLatestModelConfigSnapshots(
            ListLatestModelConfigSnapshotsCommand {
                owner_user_id: owner_user_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::ModelConfigSnapshots { snapshots } = result else {
        return Err("Local Agent Host 返回了错误的模型配置响应。".into());
    };
    snapshots
        .into_iter()
        .find(|snapshot| {
            snapshot.owner_user_id == owner_user_id && snapshot.model_config_ref == model_config_id
        })
        .ok_or_else(|| "该 Agent 绑定的模型当前不可用，请编辑 Agent 后重试。".to_owned())
}

async fn ensure_agent_runtime_settings(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    model: &LocalModelConfigSnapshot,
    thinking_level: Option<&str>,
) -> Result<LocalConversationRuntimeSettings, String> {
    let current = manager
        .execute_detailed(HostCommand::GetConversationRuntimeSettings(
            GetConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await;
    let expected_version = match current {
        Ok(HostResult::ConversationRuntimeSettings { settings }) => {
            let matches = settings.selected_model_config_ref == model.model_config_ref
                && settings.selected_model_config_revision == model.model_config_revision
                && settings.selected_thinking_level.as_deref() == thinking_level
                && settings.remote_connection_id.is_none()
                && settings.reasoning_enabled
                    == thinking_level.is_some_and(|level| level != "none");
            if matches {
                return Ok(settings);
            }
            let version = settings.version;
            Some(version)
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的运行配置响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => None,
        Err(error) => return Err(error.message),
    };
    let result = manager
        .execute(HostCommand::PutConversationRuntimeSettings(
            PutConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                selected_model_config_ref: model.model_config_ref.clone(),
                selected_model_config_revision: model.model_config_revision.clone(),
                selected_thinking_level: thinking_level.map(str::to_owned),
                remote_connection_id: None,
                reasoning_enabled: thinking_level.is_some_and(|level| level != "none"),
                expected_version,
            },
        ))
        .await?;
    let HostResult::ConversationRuntimeSettings { settings } = result else {
        return Err("Local Agent Host 返回了错误的运行配置响应。".into());
    };
    Ok(settings)
}

async fn latest_capability_revision(
    manager: &HostManager,
    owner_user_id: &str,
    profile_key: &str,
) -> Result<String, String> {
    let result = manager
        .execute(HostCommand::GetLatestCapabilityPolicySnapshot(
            GetLatestCapabilityPolicySnapshotCommand {
                owner_user_id: owner_user_id.to_owned(),
                profile_key: profile_key.to_owned(),
            },
        ))
        .await?;
    let HostResult::CapabilityPolicySnapshot { snapshot } = result else {
        return Err("Local Agent Host 返回了错误的能力配置响应。".into());
    };
    if snapshot.owner_user_id != owner_user_id || snapshot.profile_key != profile_key {
        return Err("Local Agent Host 返回了不匹配的能力配置。".into());
    }
    Ok(snapshot.capability_policy_revision)
}

fn agent_execution_metadata(
    binding: &AgentExecutionBinding,
    profession: &AgentProfessionRuntimeSnapshot,
) -> serde_json::Value {
    json!({
        "source": "native_agent_direct",
        "agent_execution_context": {
            "schema_version": 1,
            "room_id": binding.room_id,
            "agent_id": binding.agent_id,
            "agent_name": binding.name,
            "role_prompt": binding.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": binding.default_plugin_ids,
            "default_skill_ids": binding.default_skill_ids,
        }
    })
}

fn team_agent_execution_metadata(
    pending: &PendingTeamDelivery,
    profession: &AgentProfessionRuntimeSnapshot,
) -> Result<serde_json::Value, String> {
    let binding = &pending.binding;
    let mut metadata = json!({
        "source": "native_agent_team",
        "agent_execution_context": {
            "schema_version": 2,
            "room_id": binding.agent.room_id,
            "agent_id": binding.agent.agent_id,
            "agent_name": binding.agent.name,
            "role_prompt": binding.agent.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": binding.agent.default_plugin_ids,
            "default_skill_ids": binding.agent.default_skill_ids,
            "project_id": binding.project_id,
            "team_name": binding.team_name,
            "team_goal": binding.team_goal,
            "team_role": binding.team_role,
            "team_responsibility": binding.team_responsibility,
            "is_project_manager": binding.is_project_manager,
        }
    });
    if pending.trigger_kind == "todo" {
        let todo_id = pending.todo_id.as_deref().ok_or_else(team_store_error)?;
        let plan = pending
            .execution_plan
            .as_ref()
            .ok_or_else(team_store_error)?;
        metadata["agent_team_execution"] = json!({
            "schema_version": 1,
            "delivery_lane": "executor",
            "trigger_kind": "todo",
            "delivery_id": pending.delivery_id,
            "todo_id": todo_id,
            "team_id": binding.agent.room_id,
            "agent_id": binding.agent.agent_id,
            "project_id": binding.project_id,
            "execution_plan": {
                "requires_execution": plan.requires_execution,
                "builtin_capabilities": plan.builtin_capabilities,
            }
        });
    }
    Ok(metadata)
}

fn agent_communication_execution_metadata(
    pending: &PendingAgentCommunicationDelivery,
    profession: &AgentProfessionRuntimeSnapshot,
) -> serde_json::Value {
    json!({
        "source": "native_agent_communication",
        "agent_execution_context": {
            "schema_version": 3,
            "conversation_kind": pending.conversation_kind,
            "room_id": pending.binding.room_id,
            "agent_id": pending.binding.agent_id,
            "agent_name": pending.binding.name,
            "role_prompt": pending.binding.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": pending.binding.default_plugin_ids,
            "default_skill_ids": pending.binding.default_skill_ids,
            "delivery_id": pending.delivery_id,
            "trigger_kind": pending.trigger_kind,
        }
    })
}

fn delivery_state(turn: &LocalConversationTurnRecord) -> (&'static str, Option<i64>) {
    match turn.status {
        LocalConversationTurnStatus::Running => ("running", None),
        LocalConversationTurnStatus::Succeeded => ("completed", Some(turn.updated_at_unix_ms)),
        LocalConversationTurnStatus::Failed => ("failed", Some(turn.updated_at_unix_ms)),
        LocalConversationTurnStatus::Cancelled => ("cancelled", Some(turn.updated_at_unix_ms)),
    }
}

fn projected_message_text(content: &serde_json::Value) -> String {
    if let Some(value) = content.as_str() {
        return value.chars().take(1_000_000).collect();
    }
    let projected = content.as_object().and_then(|object| {
        ["text", "content", "report", "answer", "error", "message"]
            .into_iter()
            .find_map(|key| object.get(key).and_then(serde_json::Value::as_str))
    });
    projected
        .unwrap_or_default()
        .chars()
        .take(1_000_000)
        .collect()
}

fn identifier(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

fn validate_host_conversation(
    conversation: &chatos_local_agent_protocol::LocalConversationDetail,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
        || conversation.conversation.resource.is_some()
    {
        return Err("Local Agent Host 返回了不匹配的 Agent 会话。".into());
    }
    Ok(())
}

fn validate_team_executor_host_conversation(
    conversation: &LocalConversationDetail,
    owner_user_id: &str,
    conversation_id: &str,
    project_id: &str,
) -> Result<(), String> {
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
        || !matches!(
            conversation.conversation.resource.as_ref(),
            Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Project,
                resource_id,
            }) if resource_id == project_id
        )
    {
        return Err("Local Agent Host 返回了不匹配的 executor 项目会话。".into());
    }
    Ok(())
}

fn load_direct_room(
    connection: &Connection,
    owner_user_id: &str,
    direct_key: &str,
) -> Result<Option<DirectRoom>, String> {
    connection
        .query_row(
            r#"
            SELECT room.id, member.agent_id, room.name,
                   room.created_at_unix_ms, room.updated_at_unix_ms
            FROM project_agent_rooms room
            JOIN project_agent_room_members member
              ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
            WHERE room.owner_user_id = ?1 AND room.direct_key = ?2
              AND room.conversation_kind = 'human_agent_direct'
              AND room.status = 'active' AND member.status = 'active'
            ORDER BY member.joined_at_unix_ms, member.agent_id
            LIMIT 1
            "#,
            params![owner_user_id, direct_key],
            |row| {
                Ok(DirectRoom {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    name: row.get(2)?,
                    created_at_unix_ms: row.get(3)?,
                    updated_at_unix_ms: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|_| store_error())
}

fn open_database(path: &Path, read_only: bool) -> Result<Connection, String> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let connection = Connection::open_with_flags(path, flags).map_err(|_| store_error())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| store_error())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|_| store_error())?;
    Ok(connection)
}

fn confined_connector_directory(app_data_directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(app_data_directory).map_err(|_| store_error())?;
    let app_data = fs::canonicalize(app_data_directory).map_err(|_| store_error())?;
    let requested = app_data.join("NativeConnector");
    fs::create_dir_all(&requested).map_err(|_| store_error())?;
    let connector = fs::canonicalize(requested).map_err(|_| store_error())?;
    if !connector.starts_with(&app_data) {
        return Err(store_error());
    }
    Ok(connector)
}

fn reject_link_or_non_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(store_error())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(store_error()),
    }
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.chars().count() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("Agent 私聊标识无效。".into());
    }
    Ok(())
}

fn validate_agent_identifier(value: &str) -> Result<(), String> {
    validate_identifier(value)?;
    if value.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    }) {
        return Err("Agent 私聊标识无效。".into());
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize, optional: bool) -> Result<(), String> {
    if (!optional && value.is_empty())
        || value.chars().count() > maximum
        || value.trim() != value
        || value.contains('\0')
    {
        return Err(store_error());
    }
    Ok(())
}

fn validate_team_summary(team: &AgentTeamSummary) -> Result<(), String> {
    validate_identifier(&team.id).map_err(|_| team_store_error())?;
    validate_identifier(&team.project_id).map_err(|_| team_store_error())?;
    validate_text(&team.name, 160, false).map_err(|_| team_store_error())?;
    validate_text(&team.goal, 8_000, true).map_err(|_| team_store_error())?;
    if let Some(agent_id) = team.default_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    if let Some(agent_id) = team.project_manager_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    if team.member_count < 0
        || team.created_at_unix_ms < 0
        || team.updated_at_unix_ms < team.created_at_unix_ms
    {
        return Err(team_store_error());
    }
    Ok(())
}

fn validate_team_member(member: &AgentTeamMemberSummary) -> Result<(), String> {
    validate_agent_identifier(&member.agent_id).map_err(|_| team_store_error())?;
    validate_text(&member.role, 160, false).map_err(|_| team_store_error())?;
    validate_text(&member.responsibility, 8_000, true).map_err(|_| team_store_error())?;
    if member.joined_at_unix_ms < 0 {
        return Err(team_store_error());
    }
    Ok(())
}

fn map_team_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTeamAsset> {
    Ok(AgentTeamAsset {
        id: row.get(0)?,
        team_id: row.get(1)?,
        category: row.get(2)?,
        title: row.get(3)?,
        markdown: row.get(4)?,
        revision: row.get(5)?,
        status: row.get(6)?,
        created_by_agent_id: row.get(7)?,
        updated_by_agent_id: row.get(8)?,
        created_at_unix_ms: row.get(9)?,
        updated_at_unix_ms: row.get(10)?,
    })
}

fn load_agent_asset_tool_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<AgentAssetToolBinding, String> {
    let binding = connection
        .query_row(
            r#"
            SELECT delivery.room_id, delivery.target_agent_id, todo.id,
                   CASE WHEN room.project_manager_agent_id = delivery.target_agent_id
                        THEN 1 ELSE 0 END,
                   delivery.trigger_kind
            FROM windows_agent_delivery_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = delivery.owner_user_id
             AND room.id = delivery.room_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = delivery.owner_user_id
             AND member.room_id = delivery.room_id
             AND member.agent_id = delivery.target_agent_id
            LEFT JOIN local_agent_todos todo
              ON todo.owner_user_id = delivery.owner_user_id
             AND todo.team_room_id = delivery.room_id
             AND todo.agent_id = delivery.target_agent_id
             AND delivery.trigger_kind = 'todo'
             AND (delivery.deduplication_key = 'todo:' || todo.id
                  OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
            WHERE run.owner_user_id = ?1 AND run.run_id = ?2
              AND run.status IN ('ready', 'running')
              AND delivery.status = 'running'
              AND room.status = 'active' AND room.conversation_kind = 'project_team'
              AND member.status = 'active'
            LIMIT 1
            "#,
            params![owner_user_id, run_id],
            |row| {
                Ok((
                    AgentAssetToolBinding {
                        team_id: row.get(0)?,
                        agent_id: row.get(1)?,
                        todo_id: row.get(2)?,
                        is_project_manager: row.get::<_, i64>(3)? == 1,
                    },
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| team_store_error())?
        .ok_or_else(|| "The Agent team asset Run binding is unavailable.".to_owned())?;
    match (binding.1.as_str(), binding.0.todo_id.is_some()) {
        ("todo", true) => {}
        ("todo", false) | (_, true) => return Err(team_store_error()),
        _ => {}
    }
    Ok(binding.0)
}

fn load_agent_communication_tool_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<AgentCommunicationToolBinding, String> {
    connection
        .query_row(
            r#"
            SELECT delivery.room_id, delivery.target_agent_id,
                   room.conversation_kind,
                   CASE WHEN room.project_manager_agent_id = delivery.target_agent_id
                        THEN 1 ELSE 0 END
            FROM windows_agent_delivery_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = delivery.owner_user_id
             AND room.id = delivery.room_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = delivery.owner_user_id
             AND member.room_id = delivery.room_id
             AND member.agent_id = delivery.target_agent_id
            WHERE run.owner_user_id = ?1 AND run.run_id = ?2
              AND run.status IN ('ready', 'running')
              AND delivery.status = 'running' AND delivery.trigger_kind != 'todo'
              AND room.status = 'active'
              AND (room.conversation_kind IN ('project_team', 'agent_agent_direct')
                   OR (room.conversation_kind = 'human_agent_direct'
                       AND delivery.trigger_kind = 'heartbeat'))
              AND member.status = 'active'
            LIMIT 1
            "#,
            params![owner_user_id, run_id],
            |row| {
                Ok(AgentCommunicationToolBinding {
                    room_id: row.get(0)?,
                    agent_id: row.get(1)?,
                    conversation_kind: row.get(2)?,
                    is_project_manager: row.get::<_, i64>(3)? == 1,
                })
            },
        )
        .optional()
        .map_err(|_| team_store_error())?
        .ok_or_else(|| "The Agent communication Run binding is unavailable.".to_owned())
}

fn require_active_project_team(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_agent_rooms WHERE owner_user_id = ?1 \
             AND id = ?2 AND status = 'active' AND conversation_kind = 'project_team')",
            params![owner_user_id, team_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| team_store_error())?;
    if !exists {
        return Err("Agent 团队不存在或已停用。".into());
    }
    Ok(())
}

fn require_project_team_manager(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    let allowed = connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id
                 AND member.room_id = room.id
                 AND member.agent_id = room.project_manager_agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.project_manager_agent_id = ?3
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                  AND member.status = 'active'
            )
            "#,
            params![owner_user_id, team_id, agent_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| team_store_error())?;
    if !allowed {
        return Err("Only the active project manager Agent may change team assets.".into());
    }
    Ok(())
}

fn validate_team_asset_category(category: &str) -> Result<(), String> {
    if matches!(
        category,
        "overview"
            | "current_progress"
            | "tech_stack"
            | "architecture"
            | "conventions"
            | "decision"
            | "reference"
    ) {
        Ok(())
    } else {
        Err(invalid_team_asset_input())
    }
}

fn validate_team_asset(asset: &AgentTeamAsset) -> Result<(), String> {
    validate_identifier(&asset.id).map_err(|_| team_store_error())?;
    validate_identifier(&asset.team_id).map_err(|_| team_store_error())?;
    validate_team_asset_category(&asset.category).map_err(|_| team_store_error())?;
    validate_text(&asset.title, 240, false).map_err(|_| team_store_error())?;
    validate_text(&asset.markdown, 128_000, true).map_err(|_| team_store_error())?;
    if !matches!(asset.status.as_str(), "active" | "archived")
        || asset.revision <= 0
        || asset.created_at_unix_ms < 0
        || asset.updated_at_unix_ms < asset.created_at_unix_ms
    {
        return Err(team_store_error());
    }
    if let Some(agent_id) = asset.created_by_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    if let Some(agent_id) = asset.updated_by_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    Ok(())
}

fn validate_team_execution_binding(binding: &TeamExecutionBinding) -> Result<(), String> {
    let agent = &binding.agent;
    validate_agent_execution_binding(agent)?;
    validate_identifier(&binding.project_id).map_err(|_| team_store_error())?;
    validate_text(&binding.team_name, 160, false).map_err(|_| team_store_error())?;
    validate_text(&binding.team_goal, 8_000, true).map_err(|_| team_store_error())?;
    validate_text(&binding.team_role, 160, false).map_err(|_| team_store_error())?;
    validate_text(&binding.team_responsibility, 8_000, true).map_err(|_| team_store_error())
}

fn validate_agent_execution_binding(binding: &AgentExecutionBinding) -> Result<(), String> {
    let agent = binding;
    validate_identifier(&agent.room_id).map_err(|_| team_store_error())?;
    validate_agent_identifier(&agent.agent_id).map_err(|_| team_store_error())?;
    validate_text(&agent.name, 120, false).map_err(|_| team_store_error())?;
    validate_text(&agent.role_prompt, 32_000, false).map_err(|_| team_store_error())?;
    validate_identifier(&agent.model_config_id).map_err(|_| team_store_error())?;
    validate_identifier(&agent.profession_key).map_err(|_| team_store_error())?;
    if agent.thinking_level.as_deref().is_some_and(|level| {
        !matches!(
            level,
            "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        )
    }) {
        return Err(team_store_error());
    }
    Ok(())
}

fn agent_communication_conversation_id(room_id: &str, agent_id: &str) -> String {
    format!("agent-communication-{room_id}-{agent_id}")
}

fn require_active_project(
    database: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<(), String> {
    let connection = crate::workspace_index::open_project_registry(database)?;
    let project = connection
        .query_row(
            "SELECT name, revision FROM local_project_records \
             WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
            params![owner_user_id, project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|_| "项目注册表包含无效数据。".to_owned())?;
    let Some((name, revision)) = project else {
        return Err("项目不存在或已停用，不能创建 Agent 团队。".into());
    };
    if name.trim().is_empty()
        || name.chars().count() > 1_024
        || name.chars().any(char::is_control)
        || revision <= 0
    {
        return Err("项目注册表包含无效数据。".into());
    }
    Ok(())
}

fn parse_identifier_array(json: &str) -> Result<Vec<String>, String> {
    let values: Vec<String> = serde_json::from_str(json).map_err(|_| store_error())?;
    if values.len() > 100 {
        return Err(store_error());
    }
    let mut unique = std::collections::HashSet::new();
    for value in &values {
        validate_identifier(value).map_err(|_| store_error())?;
        if !unique.insert(value) {
            return Err(store_error());
        }
    }
    Ok(values)
}

fn store_error() -> String {
    "Agent 私聊数据不可用，请检查本机存储后重试。".into()
}

fn team_store_error() -> String {
    "Agent 团队数据不可用，请检查本机存储后重试。".into()
}

fn invalid_team_input() -> String {
    "Agent 团队信息无效，请检查后重试。".into()
}

fn invalid_team_message() -> String {
    "Agent 团队消息无效，请检查内容和 @Agent 后重试。".into()
}

fn invalid_team_asset_input() -> String {
    "团队共享资产内容无效。".into()
}

fn team_asset_conflict() -> String {
    "团队共享资产已被更新，请刷新后重试。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, AgentConversationStore) {
        let directory = tempfile::tempdir().unwrap();
        let connector = directory.path().join("NativeConnector");
        fs::create_dir_all(&connector).unwrap();
        let database = connector.join(DATABASE_FILENAME);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE local_agent_profiles (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    role_prompt TEXT NOT NULL DEFAULT '负责测试',
                    model_config_id TEXT NOT NULL DEFAULT 'model-1',
                    thinking_level TEXT DEFAULT 'high',
                    profession_key TEXT NOT NULL DEFAULT 'software_engineer',
                    default_plugin_ids_json TEXT NOT NULL DEFAULT '[]',
                    default_skill_ids_json TEXT NOT NULL DEFAULT '["local.project.list"]',
                    heartbeat_enabled INTEGER NOT NULL DEFAULT 0,
                    heartbeat_interval_seconds INTEGER NOT NULL DEFAULT 900,
                    heartbeat_prompt TEXT NOT NULL DEFAULT '',
                    last_heartbeat_at_unix_ms INTEGER,
                    next_heartbeat_at_unix_ms INTEGER,
                    status TEXT NOT NULL,
                    PRIMARY KEY(owner_user_id, id)
                );
                "#,
            )
            .unwrap();
        drop(connection);
        let store = AgentConversationStore::new(directory.path().to_path_buf()).unwrap();
        (directory, store)
    }

    fn insert_agent(store: &AgentConversationStore, owner: &str, id: &str, status: &str) {
        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "INSERT INTO local_agent_profiles (owner_user_id, id, name, description, status) \
                 VALUES (?1, ?2, '测试 Agent', '负责测试', ?3)",
                params![owner, id, status],
            )
            .unwrap();
    }

    #[test]
    fn direct_room_is_idempotent_and_binds_exactly_one_active_agent() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");

        let first = store.open_human_agent_direct("alice", "agent-1").unwrap();
        let second = store.open_human_agent_direct("alice", "agent-1").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.agent_id, "agent-1");

        let connection = Connection::open(store.database()).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = 'alice' AND room_id = ?1 AND status = 'active'",
                params![first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn direct_room_rejects_foreign_archived_and_malformed_agents() {
        let (_directory, store) = store();
        insert_agent(&store, "bob", "foreign", "active");
        insert_agent(&store, "alice", "archived", "archived");

        assert!(store.open_human_agent_direct("alice", "foreign").is_err());
        assert!(store.open_human_agent_direct("alice", "archived").is_err());
        assert!(store.open_human_agent_direct("alice", "../agent").is_err());
    }

    #[test]
    fn execution_binding_is_loaded_only_from_the_exact_room_member_and_profile() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let room = store.open_human_agent_direct("alice", "agent-1").unwrap();

        let binding = store.load_execution_binding("alice", &room.id).unwrap();
        assert_eq!(binding.room_id, room.id);
        assert_eq!(binding.agent_id, "agent-1");
        assert_eq!(binding.model_config_id, "model-1");
        assert_eq!(binding.thinking_level.as_deref(), Some("high"));
        assert_eq!(binding.profession_key, "software_engineer");
        assert_eq!(binding.default_skill_ids, vec!["local.project.list"]);
        assert!(store.load_execution_binding("bob", &room.id).is_err());
    }

    #[test]
    fn project_teams_are_account_scoped_and_unique_per_active_project() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        insert_agent(&store, "bob", "agent-2", "active");
        let created = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        assert_eq!(created.project_id, "project-1");
        assert_eq!(created.member_count, 0);
        assert_eq!(
            store.list_project_teams("alice").unwrap(),
            vec![created.clone()]
        );
        assert!(store
            .load_project_team("alice", &created.id)
            .unwrap()
            .members
            .is_empty());
        let invited = store
            .invite_agent_to_project_team("alice", &created.id, "agent-1")
            .unwrap();
        assert_eq!(invited.team.member_count, 1);
        assert_eq!(invited.team.default_agent_id.as_deref(), Some("agent-1"));
        assert_eq!(invited.members.len(), 1);
        assert_eq!(invited.members[0].agent_id, "agent-1");
        assert_eq!(invited.members[0].role, "测试 Agent");
        assert!(store
            .invite_agent_to_project_team("alice", &created.id, "agent-1")
            .is_err());
        assert!(store
            .invite_agent_to_project_team("alice", &created.id, "agent-2")
            .is_err());
        assert!(store.list_project_teams("bob").unwrap().is_empty());
        assert!(store
            .create_project_team("alice", "project-1", "重复团队", "")
            .is_err());
        assert!(store
            .create_project_team("bob", "project-1", "Bob 团队", "")
            .is_ok());
    }

    #[test]
    fn project_team_member_management_matches_macos_invariants() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        insert_agent(&store, "alice", "manager", "active");
        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
                [],
            )
            .unwrap();
        drop(connection);
        let created = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "agent-1")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "manager")
            .unwrap();

        let updated = store
            .update_project_team_member(
                "alice",
                &created.id,
                "agent-1",
                "前端工程师",
                "负责 Windows UI",
            )
            .unwrap();
        assert_eq!(updated.members[0].role, "前端工程师");
        assert_eq!(updated.members[0].responsibility, "负责 Windows UI");
        assert!(store
            .set_project_team_manager("alice", &created.id, "agent-1")
            .is_err());

        let managed = store
            .set_project_team_manager("alice", &created.id, "manager")
            .unwrap();
        assert_eq!(
            managed.team.project_manager_agent_id.as_deref(),
            Some("manager")
        );
        assert!(store
            .remove_agent_from_project_team("alice", &created.id, "manager")
            .is_err());

        let defaulted = store
            .set_project_team_default_agent("alice", &created.id, "manager")
            .unwrap();
        assert_eq!(defaulted.team.default_agent_id.as_deref(), Some("manager"));
        let removed = store
            .remove_agent_from_project_team("alice", &created.id, "agent-1")
            .unwrap();
        assert_eq!(removed.team.member_count, 1);
        assert_eq!(removed.members[0].agent_id, "manager");
        assert!(store
            .update_project_team_member("bob", &created.id, "manager", "项目经理", "负责交付",)
            .is_err());
    }

    #[test]
    fn removing_the_default_team_agent_selects_the_next_active_member() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        insert_agent(&store, "alice", "agent-2", "active");
        let created = store
            .create_project_team("alice", "project-1", "Alpha 团队", "")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "agent-1")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "agent-2")
            .unwrap();

        let removed = store
            .remove_agent_from_project_team("alice", &created.id, "agent-1")
            .unwrap();
        assert_eq!(removed.team.default_agent_id.as_deref(), Some("agent-2"));
        assert_eq!(removed.team.member_count, 1);
    }

    #[test]
    fn project_team_assets_keep_revision_history_and_reject_stale_writes() {
        let (_directory, store) = store();
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();

        let created = store
            .save_project_team_asset(
                "alice",
                &team.id,
                None,
                "overview",
                "项目概览",
                "# Alpha\n\n第一版",
                None,
                100,
            )
            .unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            store.list_project_team_assets("alice", &team.id).unwrap(),
            vec![created.clone()]
        );

        let updated = store
            .save_project_team_asset(
                "alice",
                &team.id,
                Some(&created.id),
                "current_progress",
                "当前进度",
                "# Alpha\n\n第二版",
                Some(1),
                200,
            )
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert!(store
            .save_project_team_asset(
                "alice",
                &team.id,
                Some(&created.id),
                "overview",
                "过期写入",
                "不会保存",
                Some(1),
                300,
            )
            .is_err());
        let revisions = store
            .list_project_team_asset_revisions("alice", &team.id, &created.id)
            .unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].revision, 2);
        assert_eq!(revisions[1].revision, 1);

        let archived = store
            .archive_project_team_asset("alice", &team.id, &created.id, 2, 300)
            .unwrap();
        assert_eq!(archived.status, "archived");
        assert_eq!(archived.revision, 3);
        assert!(store
            .list_project_team_assets("alice", &team.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .list_project_team_asset_revisions("alice", &team.id, &created.id)
                .unwrap()
                .len(),
            3
        );
        assert!(store.list_project_team_assets("bob", &team.id).is_err());
    }

    #[test]
    fn only_the_active_project_manager_agent_can_mutate_team_assets() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "manager", "active");
        insert_agent(&store, "alice", "member", "active");
        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
                [],
            )
            .unwrap();
        drop(connection);
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "manager")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "member")
            .unwrap();
        store
            .set_project_team_manager("alice", &team.id, "manager")
            .unwrap();
        assert!(store
            .teams_requiring_todo_scheduler("alice")
            .unwrap()
            .is_empty());

        assert!(store
            .save_project_team_asset_as_manager(
                "alice",
                &team.id,
                "member",
                None,
                "overview",
                "越权",
                "不能保存",
                None,
                100,
            )
            .is_err());
        let created = store
            .save_project_team_asset_as_manager(
                "alice",
                &team.id,
                "manager",
                None,
                "overview",
                "项目概览",
                "经理创建",
                None,
                100,
            )
            .unwrap();
        assert_eq!(created.created_by_agent_id.as_deref(), Some("manager"));
        let updated = store
            .save_project_team_asset_as_manager(
                "alice",
                &team.id,
                "manager",
                Some(&created.id),
                "current_progress",
                "当前进度",
                "经理更新",
                Some(1),
                200,
            )
            .unwrap();
        assert_eq!(updated.updated_by_agent_id.as_deref(), Some("manager"));
        let archived = store
            .archive_project_team_asset_as_manager(
                "alice",
                &team.id,
                "manager",
                &created.id,
                2,
                300,
            )
            .unwrap();
        assert_eq!(archived.updated_by_agent_id.as_deref(), Some("manager"));
        let revisions = store
            .list_project_team_asset_revisions("alice", &team.id, &created.id)
            .unwrap();
        assert!(revisions
            .iter()
            .all(|revision| revision.editor_agent_id.as_deref() == Some("manager")));
    }

    #[test]
    fn project_team_todos_are_transactional_account_scoped_and_mac_compatible() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "agent-1")
            .unwrap();
        let execution_plan = serde_json::json!({
            "requiresExecution": true,
            "builtinCapabilities": ["project_read", "project_write"],
            "plugins": [],
            "selectionRevision": "local-v1",
            "selectedAtUnixMs": 100
        })
        .to_string();
        let execution_contract = serde_json::json!({
            "objective": "实现任务看板",
            "scope": "Windows React 团队页",
            "expectedOutputs": ["可核验的任务列表"],
            "acceptanceCriteria": ["严格隔离团队与账号"],
            "constraints": ["参考 macOS 语义"]
        })
        .to_string();

        let mut connection = Connection::open(store.database()).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .unwrap();
        {
            let transaction = connection.transaction().unwrap();
            transaction
                .execute(
                    r#"
                    INSERT INTO local_agent_todos (
                        owner_user_id, id, agent_id, team_room_id, request_key,
                        title, detail, priority, sort_order, status, blocked_reason, result,
                        created_at_unix_ms, updated_at_unix_ms,
                        execution_plan_json, execution_contract_json
                    ) VALUES ('alice', 'rolled-back', 'agent-1', ?1, 'request-rollback',
                              '不应保留', '', 50, 0, 'pending', '', '', 100, 100, ?2, ?3)
                    "#,
                    params![team.id, execution_plan, execution_contract],
                )
                .unwrap();
            assert!(transaction
                .execute(
                    "INSERT INTO local_agent_todo_dependencies (owner_user_id, todo_id, \
                     prerequisite_todo_id, created_at_unix_ms) \
                     VALUES ('alice', 'rolled-back', 'rolled-back', 100)",
                    [],
                )
                .is_err());
        }
        let rolled_back: i64 = connection
            .query_row(
                "SELECT count(*) FROM local_agent_todos WHERE id = 'rolled-back'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rolled_back, 0);

        let transaction = connection.transaction().unwrap();
        for (id, request_key, title, priority, sort_order, status) in [
            (
                "todo-prerequisite",
                "request-1",
                "先完成底座",
                80,
                0,
                "completed",
            ),
            ("todo-active", "request-2", "再实现看板", 70, 1, "pending"),
        ] {
            transaction
                .execute(
                    r#"
                    INSERT INTO local_agent_todos (
                        owner_user_id, id, agent_id, team_room_id, request_key,
                        title, detail, priority, sort_order, status, blocked_reason, result,
                        created_at_unix_ms, updated_at_unix_ms,
                        execution_plan_json, execution_contract_json
                    ) VALUES ('alice', ?1, 'agent-1', ?2, ?3, ?4, '', ?5, ?6, ?7,
                              '', '', 100, 100, ?8, ?9)
                    "#,
                    params![
                        id,
                        team.id,
                        request_key,
                        title,
                        priority,
                        sort_order,
                        status,
                        execution_plan,
                        execution_contract,
                    ],
                )
                .unwrap();
        }
        transaction
            .execute(
                "INSERT INTO local_agent_todo_dependencies (owner_user_id, todo_id, \
                 prerequisite_todo_id, created_at_unix_ms) \
                 VALUES ('alice', 'todo-active', 'todo-prerequisite', 100)",
                [],
            )
            .unwrap();
        transaction.commit().unwrap();
        drop(connection);

        let active = store
            .list_project_team_todos("alice", &team.id, false)
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "todo-active");
        assert_eq!(active[0].agent_name, "测试 Agent");
        assert_eq!(active[0].dependency_ids, vec!["todo-prerequisite"]);
        assert_eq!(active[0].execution_contract.objective, "实现任务看板");
        assert_eq!(
            active[0].execution_plan.builtin_capabilities,
            vec!["project_read", "project_write"]
        );
        assert_eq!(
            store
                .list_project_team_todos("alice", &team.id, true)
                .unwrap()
                .len(),
            2
        );
        assert!(store
            .list_project_team_todos("bob", &team.id, true)
            .is_err());
    }

    #[test]
    fn project_team_todo_mutations_reject_stale_cycles_and_unsafe_running_cancel() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "agent-1")
            .unwrap();
        let foundation = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "实现持久化底座",
                "先完成任务事实源",
                80,
                &[],
                None,
                100,
            )
            .unwrap();
        let board = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "实现任务看板",
                "再连接 React",
                70,
                std::slice::from_ref(&foundation.id),
                None,
                101,
            )
            .unwrap();
        assert!(store
            .update_project_team_todo(
                "alice",
                &team.id,
                &foundation.id,
                foundation.updated_at_unix_ms,
                &foundation.title,
                "循环依赖不应保存",
                foundation.priority,
                std::slice::from_ref(&board.id),
                None,
                102,
            )
            .is_err());

        let revised = store
            .update_project_team_todo(
                "alice",
                &team.id,
                &board.id,
                board.updated_at_unix_ms,
                "实现 Apple 风格任务看板",
                "连接真实 Todo",
                75,
                std::slice::from_ref(&foundation.id),
                None,
                103,
            )
            .unwrap();
        assert_eq!(revised.title, "实现 Apple 风格任务看板");
        assert!(store
            .update_project_team_todo(
                "alice",
                &team.id,
                &board.id,
                board.updated_at_unix_ms,
                "过期更新",
                "不应保存",
                20,
                &[],
                None,
                104,
            )
            .is_err());

        let reordered = store
            .reorder_project_team_todos(
                "alice",
                &team.id,
                &[board.id.clone(), foundation.id.clone()],
                105,
            )
            .unwrap();
        assert_eq!(reordered[0].id, board.id);
        assert_eq!(reordered[0].priority, 100);

        let foundation_after_reorder = reordered
            .iter()
            .find(|todo| todo.id == foundation.id)
            .unwrap();
        let cancelled = store
            .cancel_project_team_todo(
                "alice",
                &team.id,
                &foundation.id,
                foundation_after_reorder.updated_at_unix_ms,
                106,
            )
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");

        let running = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "执行中的任务",
                "不能伪装成已取消",
                60,
                &[],
                None,
                107,
            )
            .unwrap();
        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "UPDATE local_agent_todos SET status = 'in_progress' \
                 WHERE owner_user_id = 'alice' AND id = ?1",
                params![running.id],
            )
            .unwrap();
        assert!(store
            .cancel_project_team_todo(
                "alice",
                &team.id,
                &running.id,
                running.updated_at_unix_ms,
                108,
            )
            .is_err());
        assert_eq!(
            store
                .list_project_team_todos("alice", &team.id, true)
                .unwrap()
                .into_iter()
                .find(|todo| todo.id == running.id)
                .unwrap()
                .status,
            "in_progress"
        );
    }

    #[test]
    fn running_team_todo_cancel_is_journaled_before_atomic_host_reconciliation() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "agent-1")
            .unwrap();
        let todo = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "停止中的任务",
                "必须先停止 Host Run",
                80,
                &[],
                None,
                100,
            )
            .unwrap();
        assert_eq!(
            store
                .enqueue_ready_project_team_todos("alice", &team.id, 101)
                .unwrap(),
            1
        );
        let pending = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery("alice", &pending, "turn-cancel", "run-cancel", 102)
            .unwrap();
        let running = store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == todo.id)
            .unwrap();
        let intent = store
            .record_project_team_todo_cancel_intent(
                "alice",
                &team.id,
                &todo.id,
                running.updated_at_unix_ms,
                103,
            )
            .unwrap()
            .unwrap();
        assert_eq!(intent.action, "cancel");
        let replay = store
            .record_project_team_todo_cancel_intent(
                "alice",
                &team.id,
                &todo.id,
                running.updated_at_unix_ms,
                104,
            )
            .unwrap()
            .unwrap();
        assert_eq!(replay.id, intent.id);
        assert_eq!(
            store.pending_team_run_control_intents("alice").unwrap(),
            vec![intent.clone()]
        );
        assert_eq!(
            store
                .list_project_team_todos("alice", &team.id, true)
                .unwrap()
                .into_iter()
                .find(|candidate| candidate.id == todo.id)
                .unwrap()
                .status,
            "in_progress"
        );

        store
            .reconcile_project_team_runs(
                "alice",
                &team.id,
                &[team_todo::TeamRunReconciliation {
                    delivery_id: intent.delivery_id.clone(),
                    turn_id: "turn-cancel".into(),
                    run_id: "run-cancel".into(),
                    status: LocalConversationTurnStatus::Cancelled,
                    created_at_unix_ms: 102,
                    updated_at_unix_ms: 104,
                    project_write_evidence_verified: false,
                }],
            )
            .unwrap();
        assert_eq!(
            store
                .list_project_team_todos("alice", &team.id, true)
                .unwrap()
                .into_iter()
                .find(|candidate| candidate.id == todo.id)
                .unwrap()
                .status,
            "blocked"
        );
        assert_eq!(
            store.pending_team_run_control_intents("alice").unwrap(),
            vec![intent.clone()]
        );

        let cancelled = store
            .complete_team_run_cancel_intent(
                &intent,
                &LocalConversationTurnRecord {
                    turn_id: "turn-cancel".into(),
                    conversation_id: intent.conversation_id.clone(),
                    user_message_id: pending.message_id,
                    run_id: "run-cancel".into(),
                    status: LocalConversationTurnStatus::Cancelled,
                    created_at_unix_ms: 102,
                    updated_at_unix_ms: 105,
                },
            )
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(store
            .pending_team_run_control_intents("alice")
            .unwrap()
            .is_empty());
        let connection = Connection::open(store.database()).unwrap();
        let states: (String, String, String) = connection
            .query_row(
                "SELECT delivery.status, run.status, journal.status \
                 FROM project_agent_deliveries delivery \
                 JOIN windows_agent_delivery_runs run \
                   ON run.owner_user_id = delivery.owner_user_id \
                  AND run.delivery_id = delivery.id \
                 JOIN windows_agent_run_control_journal journal \
                   ON journal.owner_user_id = delivery.owner_user_id \
                  AND journal.delivery_id = delivery.id \
                 WHERE delivery.owner_user_id = 'alice' AND delivery.id = ?1",
                params![intent.delivery_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            states,
            ("cancelled".into(), "cancelled".into(), "completed".into())
        );
    }

    #[test]
    fn interrupted_team_todo_retry_is_journaled_and_reopens_only_after_host_acceptance() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "agent-1")
            .unwrap();
        let todo = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "中断任务",
                "等待 Human 核验后重试",
                80,
                &[],
                None,
                100,
            )
            .unwrap();
        store
            .enqueue_ready_project_team_todos("alice", &team.id, 101)
            .unwrap();
        let pending = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery("alice", &pending, "turn-retry", "run-retry", 102)
            .unwrap();
        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "UPDATE local_agent_todos SET status = 'blocked', \
                 blocked_reason = '写入结果需要 Human 核验' \
                 WHERE owner_user_id = 'alice' AND id = ?1",
                params![todo.id],
            )
            .unwrap();
        drop(connection);

        let intent = store
            .record_project_team_run_retry_intent("alice", &team.id, "run-retry", 103)
            .unwrap();
        assert_eq!(intent.action, "retry");
        assert_eq!(
            store
                .list_project_team_todos("alice", &team.id, true)
                .unwrap()
                .into_iter()
                .find(|candidate| candidate.id == todo.id)
                .unwrap()
                .status,
            "blocked"
        );
        let resumed = store
            .complete_team_run_retry_intent(
                &intent,
                &LocalConversationTurnRecord {
                    turn_id: "turn-retry".into(),
                    conversation_id: intent.conversation_id.clone(),
                    user_message_id: pending.message_id,
                    run_id: "run-retry".into(),
                    status: LocalConversationTurnStatus::Running,
                    created_at_unix_ms: 102,
                    updated_at_unix_ms: 104,
                },
                LocalAgentRunStatus::ContinuationReady,
                104,
            )
            .unwrap();
        assert_eq!(resumed.status, "in_progress");
        assert!(resumed.blocked_reason.is_empty());
        assert!(store
            .pending_team_run_control_intents("alice")
            .unwrap()
            .is_empty());
        let connection = Connection::open(store.database()).unwrap();
        let journal: (String, String) = connection
            .query_row(
                "SELECT status, host_status FROM windows_agent_run_control_journal \
                 WHERE owner_user_id = 'alice' AND id = ?1",
                params![intent.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(journal, ("completed".into(), "continuation_ready".into()));
    }

    #[test]
    fn project_team_messages_route_to_mentions_or_the_default_agent() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        insert_agent(&store, "alice", "agent-2", "active");
        let created = store
            .create_project_team("alice", "project-1", "Alpha 团队", "")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "agent-1")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "agent-2")
            .unwrap();

        let default_routed = store
            .queue_project_team_message("alice", &created.id, "请开始处理", &[])
            .unwrap();
        assert_eq!(default_routed.deliveries.len(), 1);
        assert_eq!(default_routed.deliveries[0].target_agent_id, "agent-1");
        assert_eq!(default_routed.deliveries[0].trigger_kind, "default_agent");

        let mentioned = store
            .queue_project_team_message(
                "alice",
                &created.id,
                "请分别评估",
                &["agent-2".into(), "agent-1".into()],
            )
            .unwrap();
        assert_eq!(mentioned.deliveries.len(), 2);
        assert_eq!(mentioned.deliveries[0].target_agent_id, "agent-2");
        assert!(mentioned
            .deliveries
            .iter()
            .all(|delivery| delivery.trigger_kind == "mention"));
        let connection = Connection::open(store.database()).unwrap();
        let mention_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_message_mentions \
                 WHERE owner_user_id = 'alice' AND message_id = ?1",
                params![mentioned.message_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mention_count, 2);
        drop(connection);

        let snapshot = store
            .load_project_team_conversation("alice", &created.id)
            .unwrap();
        assert_eq!(snapshot.team_id, created.id);
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(
            snapshot.messages[1].mentioned_agent_ids,
            vec!["agent-2", "agent-1"]
        );
        assert_eq!(snapshot.deliveries.len(), 3);

        assert!(store
            .queue_project_team_message("alice", &created.id, "错误 @", &["missing".into()])
            .is_err());
        assert!(store
            .queue_project_team_message("bob", &created.id, "越权消息", &["agent-1".into()],)
            .is_err());
    }

    #[test]
    fn project_team_delivery_claims_the_exact_scoped_agent_binding() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let created = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &created.id, "agent-1")
            .unwrap();
        store
            .update_project_team_member(
                "alice",
                &created.id,
                "agent-1",
                "前端工程师",
                "负责 Windows UI",
            )
            .unwrap();
        let queued = store
            .queue_project_team_message("alice", &created.id, "实现团队页面", &[])
            .unwrap();
        let pending = store
            .next_project_team_delivery("alice", &created.id)
            .unwrap()
            .unwrap();
        assert_eq!(pending.delivery_id, queued.deliveries[0].delivery_id);
        assert_eq!(pending.binding.project_id, "project-1");
        assert_eq!(pending.binding.team_name, "Alpha 团队");
        assert_eq!(pending.binding.team_role, "前端工程师");
        assert_eq!(pending.binding.team_responsibility, "负责 Windows UI");
        assert!(!pending.binding.is_project_manager);

        store
            .claim_project_team_delivery("alice", &pending, "turn-1", "run-1", 100)
            .unwrap();
        assert!(store
            .next_project_team_delivery("alice", &created.id)
            .unwrap()
            .is_none());
        assert!(store
            .claim_project_team_delivery("alice", &pending, "turn-2", "run-2", 101)
            .is_err());
        store
            .fail_project_team_delivery("alice", &pending.delivery_id, "host unavailable")
            .unwrap();
        let connection = Connection::open(store.database()).unwrap();
        let state: (String, String, String) = connection
            .query_row(
                "SELECT delivery.status, run.status, delivery.last_error \
                 FROM project_agent_deliveries delivery JOIN windows_agent_delivery_runs run \
                   ON run.owner_user_id = delivery.owner_user_id \
                  AND run.delivery_id = delivery.id \
                 WHERE delivery.owner_user_id = 'alice' AND delivery.id = ?1",
                params![pending.delivery_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            ("failed".into(), "failed".into(), "host unavailable".into())
        );
        drop(connection);
        let history = store.load_project_team_runs("alice", &created.id).unwrap();
        assert_eq!(history.team_id, created.id);
        assert_eq!(history.message_count, 1);
        assert_eq!(history.failed_delivery_count, 1);
        assert_eq!(history.pending_delivery_count, 0);
        assert_eq!(history.running_delivery_count, 0);
        assert_eq!(history.runs.len(), 1);
        assert_eq!(history.runs[0].delivery_id, pending.delivery_id);
        assert_eq!(history.runs[0].agent_id, "agent-1");
        assert_eq!(history.runs[0].lane, "manager");
        assert_eq!(history.runs[0].trigger_kind, "default_agent");
        assert_eq!(history.runs[0].delivery_status, "failed");
        assert_eq!(history.runs[0].run_status, "failed");
        assert_eq!(
            history.runs[0].last_error.as_deref(),
            Some("host unavailable")
        );
        assert!(store.load_project_team_runs("bob", &created.id).is_err());
    }

    #[test]
    fn ready_todo_uses_an_independent_executor_lane_and_blocks_on_host_failure() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let team = store
            .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "agent-1")
            .unwrap();
        let manager_message = store
            .queue_project_team_message("alice", &team.id, "经理通道消息", &[])
            .unwrap();
        let manager = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            manager.delivery_id,
            manager_message.deliveries[0].delivery_id
        );
        assert!(manager.execution_plan.is_none());
        store
            .claim_project_team_delivery("alice", &manager, "turn-manager", "run-manager", 100)
            .unwrap();
        let shared_asset = store
            .save_project_team_asset(
                "alice",
                &team.id,
                None,
                "overview",
                "项目说明",
                "启动时版本",
                None,
                100,
            )
            .unwrap();

        let first = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "实现任务执行",
                "连接 executor lane",
                90,
                &[],
                None,
                101,
            )
            .unwrap();
        let second = store
            .create_project_team_todo(
                "alice",
                &team.id,
                "agent-1",
                "后续任务",
                "等待第一个任务结束",
                80,
                &[],
                None,
                102,
            )
            .unwrap();
        let project_write_plan = serde_json::json!({
            "requiresExecution": true,
            "builtinCapabilities": ["project_read", "project_write"],
            "plugins": [],
            "selectionRevision": "test-project-write-v1",
            "selectedAtUnixMs": 102
        })
        .to_string();
        Connection::open(store.database())
            .unwrap()
            .execute(
                "UPDATE local_agent_todos SET execution_plan_json = ?1 \
                 WHERE owner_user_id = 'alice' AND id = ?2",
                params![project_write_plan, second.id],
            )
            .unwrap();
        assert_eq!(
            store
                .enqueue_ready_project_team_todos("alice", &team.id, 103)
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .enqueue_ready_project_team_todos("alice", &team.id, 104)
                .unwrap(),
            0
        );
        let executor = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        assert_eq!(executor.trigger_kind, "todo");
        assert_eq!(executor.todo_id.as_deref(), Some(first.id.as_str()));
        assert_eq!(
            executor
                .execution_plan
                .as_ref()
                .map(|plan| plan.builtin_capabilities.as_slice()),
            Some(["project_read".to_owned()].as_slice())
        );
        assert!(executor.content.contains("独立 Todo"));
        assert!(executor.content.contains("实现任务执行"));
        store
            .claim_project_team_delivery("alice", &executor, "turn-executor", "run-executor", 105)
            .unwrap();
        assert!(store
            .team_chat_member_snapshot("alice", "run-executor")
            .is_err());
        let (executor_binding, executor_assets) = store
            .load_agent_asset_tool_records("alice", "run-executor")
            .unwrap();
        assert_eq!(executor_binding.todo_id.as_deref(), Some(first.id.as_str()));
        assert_eq!(executor_assets.len(), 1);
        assert_eq!(executor_assets[0].markdown, "启动时版本");
        store
            .save_project_team_asset(
                "alice",
                &team.id,
                Some(&shared_asset.id),
                "overview",
                "项目说明",
                "任务启动后的版本",
                Some(1),
                106,
            )
            .unwrap();
        let (_, frozen_assets) = store
            .load_agent_asset_tool_records("alice", "run-executor")
            .unwrap();
        assert_eq!(frozen_assets[0].revision, 1);
        assert_eq!(frozen_assets[0].markdown, "启动时版本");
        let (manager_binding, manager_assets) = store
            .load_agent_asset_tool_records("alice", "run-manager")
            .unwrap();
        assert!(manager_binding.todo_id.is_none());
        assert_eq!(manager_assets[0].revision, 2);
        let (planning_binding, planning_snapshot) = store
            .todo_planning_tool_snapshot("alice", "run-manager", true)
            .unwrap();
        assert_eq!(planning_binding, manager_binding);
        assert_eq!(planning_snapshot["team"]["name"], "Alpha 团队");
        assert_eq!(planning_snapshot["todos"].as_array().unwrap().len(), 2);
        assert_eq!(
            planning_snapshot["builtin_capabilities"],
            serde_json::json!(["project_read", "project_write", "terminal"])
        );
        let todo_reference = planning_snapshot["todos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|todo| todo["title"] == "实现任务执行")
            .unwrap()["todo_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(!planning_snapshot.to_string().contains(&first.id));
        let (_, progress) = store
            .todo_planning_progress("alice", "run-manager", &todo_reference, 100)
            .unwrap();
        assert_eq!(progress["title"], "实现任务执行");
        assert_eq!(progress["progress"].as_array().unwrap().len(), 1);
        assert_eq!(progress["progress"][0]["kind"], "started");
        assert_eq!(progress["progress"][0]["run_id"], "run-executor");
        assert!(store
            .todo_planning_progress("alice", "run-manager", "todo_ref_stale", 100)
            .is_err());

        let connection = Connection::open(store.database()).unwrap();
        let running_lanes: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_deliveries WHERE owner_user_id = 'alice' \
                 AND target_agent_id = 'agent-1' AND status = 'running'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(running_lanes, 2);
        drop(connection);

        store
            .fail_project_team_delivery("alice", &executor.delivery_id, "host unavailable")
            .unwrap();
        let failed_todo = store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|todo| todo.id == first.id)
            .unwrap();
        assert_eq!(failed_todo.status, "blocked");
        assert_eq!(failed_todo.blocked_reason, "host unavailable");

        assert_eq!(
            store
                .enqueue_ready_project_team_todos("alice", &team.id, 106)
                .unwrap(),
            1
        );
        let second_executor = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        assert_eq!(second_executor.todo_id.as_deref(), Some(second.id.as_str()));
        store
            .claim_project_team_delivery(
                "alice",
                &second_executor,
                "turn-executor-2",
                "run-executor-2",
                107,
            )
            .unwrap();
        let (_, execution_context) = store
            .todo_execution_tool_context("alice", "run-executor-2")
            .unwrap();
        assert_eq!(execution_context["title"], "后续任务");
        let (_, progress) = store
            .append_todo_execution_progress(
                "alice",
                "run-executor-2",
                "implementation",
                "已完成事务写入，准备核对。",
                108,
            )
            .unwrap();
        assert_eq!(progress["kind"], "progress");
        let visible_progress = store
            .list_project_team_todo_progress("alice", &team.id, &second.id, 100)
            .unwrap();
        assert_eq!(visible_progress.len(), 2);
        assert_eq!(visible_progress[0]["kind"], "started");
        assert_eq!(visible_progress[1]["detail"], "已完成事务写入，准备核对。");
        assert!(store
            .list_project_team_todo_progress("bob", &team.id, &second.id, 100)
            .is_err());
        assert!(store
            .finish_todo_execution(
                "alice",
                "run-executor-2",
                true,
                "已完成并核验。",
                false,
                109,
            )
            .is_err());
        let (_, completion) = store
            .finish_todo_execution("alice", "run-executor-2", true, "已完成并核验。", true, 109)
            .unwrap();
        assert_eq!(completion["todo_status"], "completed");
        store
            .reconcile_project_team_runs(
                "alice",
                &team.id,
                &[team_todo::TeamRunReconciliation {
                    delivery_id: second_executor.delivery_id,
                    turn_id: "turn-executor-2".into(),
                    run_id: "run-executor-2".into(),
                    status: LocalConversationTurnStatus::Succeeded,
                    created_at_unix_ms: 107,
                    updated_at_unix_ms: 110,
                    project_write_evidence_verified: true,
                }],
            )
            .unwrap();
        let completed = store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|todo| todo.id == second.id)
            .unwrap();
        assert_eq!(completed.status, "completed");
    }

    #[test]
    fn manager_todo_planning_tools_create_update_and_reorder_transactionally() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "manager", "active");
        insert_agent(&store, "alice", "worker", "active");
        Connection::open(store.database())
            .unwrap()
            .execute(
                "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
                [],
            )
            .unwrap();
        let team = store
            .create_project_team("alice", "project-1", "计划团队", "完成可靠交付")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "manager")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "worker")
            .unwrap();
        store
            .set_project_team_manager("alice", &team.id, "manager")
            .unwrap();
        let message = store
            .queue_project_team_message("alice", &team.id, "请拆分并执行这项工作", &[])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        assert_eq!(delivery.delivery_id, message.deliveries[0].delivery_id);
        store
            .claim_project_team_delivery("alice", &delivery, "turn-manager", "run-manager", 10)
            .unwrap();
        let (member_binding, members) = store
            .team_chat_member_snapshot("alice", "run-manager")
            .unwrap();
        assert_eq!(member_binding.room_id, team.id);
        assert_eq!(members.as_array().unwrap().len(), 2);
        assert!(members
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["name"] == "测试 Agent" && member["is_project_manager"] == true));
        assert!(!members.to_string().contains("\"manager\""));
        assert!(!members.to_string().contains("\"worker\""));
        let (_, options) = store
            .todo_planning_tool_snapshot("alice", "run-manager", true)
            .unwrap();
        let team_reference = options["team"]["team_ref"].as_str().unwrap().to_owned();
        let worker_reference = options["team"]["assignees"]
            .as_array()
            .unwrap()
            .iter()
            .find(|assignee| {
                assignee["name"] == "测试 Agent" && !assignee["is_current_agent"].as_bool().unwrap()
            })
            .unwrap()["assignee_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let contract = team_todo::AgentTodoExecutionContract {
            objective: "实现核心功能".into(),
            scope: "只修改项目内代码".into(),
            expected_outputs: vec!["可运行实现".into()],
            acceptance_criteria: vec!["测试通过".into()],
            constraints: vec!["不得伪造完成".into()],
        };
        let (binding, first) = store
            .create_todo_from_planning_tool(
                "alice",
                "run-manager",
                team_todo::TodoPlanningCreateInput {
                    request_key: "invocation-create-1".into(),
                    team_reference: team_reference.clone(),
                    assignee_reference: worker_reference.clone(),
                    title: "实现核心".into(),
                    detail: "按验收条件完成".into(),
                    priority: 80,
                    dependency_references: vec![],
                    execution_contract: contract.clone(),
                    requires_execution: true,
                    builtin_capabilities: vec!["project_write".into()],
                },
                11,
            )
            .unwrap();
        assert!(binding.is_project_manager);
        assert_eq!(
            first["builtin_capabilities"],
            json!(["project_read", "project_write"])
        );
        let first_reference = first["todo_ref"].as_str().unwrap().to_owned();
        let (_, second) = store
            .create_todo_from_planning_tool(
                "alice",
                "run-manager",
                team_todo::TodoPlanningCreateInput {
                    request_key: "invocation-create-2".into(),
                    team_reference,
                    assignee_reference: worker_reference,
                    title: "执行验证".into(),
                    detail: "验证第一项交付".into(),
                    priority: 70,
                    dependency_references: vec![first_reference.clone()],
                    execution_contract: team_todo::AgentTodoExecutionContract {
                        objective: "验证交付".into(),
                        scope: "运行真实测试".into(),
                        expected_outputs: vec!["测试记录".into()],
                        acceptance_criteria: vec!["结果可复现".into()],
                        constraints: vec![],
                    },
                    requires_execution: true,
                    builtin_capabilities: vec!["terminal".into()],
                },
                12,
            )
            .unwrap();
        assert_eq!(second["dependencies"][0]["todo_ref"], first_reference);
        let second_reference = second["todo_ref"].as_str().unwrap().to_owned();
        let (_, updated) = store
            .update_todo_from_planning_tool(
                "alice",
                "run-manager",
                team_todo::TodoPlanningUpdateInput {
                    todo_reference: first_reference.clone(),
                    title: Some("实现核心并自测".into()),
                    detail: None,
                    priority: Some(95),
                    status: None,
                    dependency_references: None,
                    objective: None,
                    scope: None,
                    expected_outputs: None,
                    acceptance_criteria: None,
                    constraints: None,
                },
                13,
            )
            .unwrap();
        assert_eq!(updated["title"], "实现核心并自测");
        let (_, reordered) = store
            .reorder_todos_from_planning_tool(
                "alice",
                "run-manager",
                &[second_reference, first_reference],
                14,
            )
            .unwrap();
        assert_eq!(reordered["todos"][0]["title"], "执行验证");
        let source_count: i64 = Connection::open(store.database())
            .unwrap()
            .query_row(
                "SELECT count(*) FROM local_agent_todo_sources source \
                 JOIN project_agent_messages message \
                   ON message.owner_user_id = source.owner_user_id \
                  AND message.id = source.message_id \
                 WHERE source.owner_user_id = 'alice' AND message.id = ?1",
                params![message.message_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_count, 3);
        assert!(store
            .team_todo_scheduler_has_work("alice", &team.id)
            .unwrap());
        store
            .reconcile_project_team_runs(
                "alice",
                &team.id,
                &[team_todo::TeamRunReconciliation {
                    delivery_id: delivery.delivery_id,
                    turn_id: "turn-manager".into(),
                    run_id: "run-manager".into(),
                    status: LocalConversationTurnStatus::Succeeded,
                    created_at_unix_ms: 10,
                    updated_at_unix_ms: 15,
                    project_write_evidence_verified: false,
                }],
            )
            .unwrap();
        assert!(!store
            .team_todo_scheduler_has_work("alice", &team.id)
            .unwrap());
        assert_eq!(
            store
                .enqueue_ready_project_team_todos("alice", &team.id, 16)
                .unwrap(),
            1
        );
        assert!(store
            .team_todo_scheduler_has_work("alice", &team.id)
            .unwrap());
        assert_eq!(
            store.teams_requiring_todo_scheduler("alice").unwrap(),
            vec![team.id]
        );
    }

    #[test]
    fn team_agent_messages_use_run_scoped_mentions_and_idempotent_receipts() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "sender", "active");
        insert_agent(&store, "alice", "target", "active");
        let team = store
            .create_project_team("alice", "project-1", "通讯团队", "交付消息")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "sender")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "target")
            .unwrap();
        let trigger = store
            .queue_project_team_message("alice", &team.id, "请协调目标成员", &["sender".into()])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery("alice", &delivery, "turn-chat", "run-chat", 10)
            .unwrap();
        let (_, members) = store
            .team_chat_member_snapshot("alice", "run-chat")
            .unwrap();
        let target_reference = members
            .as_array()
            .unwrap()
            .iter()
            .find(|member| member["is_current_agent"] == false)
            .unwrap()["agent_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let (_, first) = store
            .send_team_chat_message(
                "alice",
                "run-chat",
                "invocation-chat-1",
                "请处理接口验证。",
                None,
                None,
                None,
                &[],
                std::slice::from_ref(&target_reference),
                11,
            )
            .unwrap();
        assert_eq!(first["completed"], false);
        assert_eq!(first["spawned_delivery_count"], 1);
        assert!(!first.to_string().contains("target"));
        let (_, replayed) = store
            .send_team_chat_message(
                "alice",
                "run-chat",
                "invocation-chat-1",
                "请处理接口验证。",
                None,
                None,
                None,
                &[],
                std::slice::from_ref(&target_reference),
                12,
            )
            .unwrap();
        assert_eq!(replayed, first);
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-chat",
                "invocation-chat-1",
                "复用调用 ID 的不同消息。",
                None,
                None,
                None,
                &[],
                std::slice::from_ref(&target_reference),
                13,
            )
            .is_err());
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-chat",
                "invocation-chat-stale-ref",
                "错误引用。",
                None,
                None,
                None,
                &[],
                &["agent_ref_stale".into()],
                13,
            )
            .is_err());
        let connection = Connection::open(store.database()).unwrap();
        let message: (String, String, String, String, i64) = connection
            .query_row(
                "SELECT sender_id, reply_to_message_id, source_run_id, causation_id, hop_count \
                 FROM project_agent_messages WHERE owner_user_id = 'alice' \
                   AND causation_id = 'invocation-chat-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            message,
            (
                "sender".into(),
                trigger.message_id.clone(),
                "run-chat".into(),
                "invocation-chat-1".into(),
                1,
            )
        );
        let spawned: (i64, String, String) = connection
            .query_row(
                "SELECT count(*), min(target_agent_id), min(trigger_kind) \
                 FROM project_agent_deliveries WHERE owner_user_id = 'alice' \
                   AND trigger_kind = 'agent_mention'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(spawned, (1, "target".into(), "agent_mention".into()));
        connection
            .execute(
                "UPDATE project_agent_messages SET hop_count = 4 \
                 WHERE owner_user_id = 'alice' AND id = ?1",
                params![trigger.message_id],
            )
            .unwrap();
        drop(connection);
        let (_, unread) = store
            .read_all_unread_team_chat("alice", "run-chat", "read-routing-limit", 20, 14)
            .unwrap();
        let trigger_reference = unread["conversations"][0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|message| message["content"] == "请协调目标成员")
            .unwrap()["message_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let (_, stopped) = store
            .send_team_chat_message(
                "alice",
                "run-chat",
                "invocation-routing-limit",
                "这条回复只持久化，不再继续唤醒。",
                Some(&trigger_reference),
                None,
                None,
                &[],
                std::slice::from_ref(&target_reference),
                15,
            )
            .unwrap();
        assert_eq!(stopped["spawned_delivery_count"], 0);
        assert_eq!(
            stopped["routing_stop_reason"],
            "本轮 Agent 协作已达到最大唤醒深度。"
        );
        let stopped_delivery_count: i64 = Connection::open(store.database())
            .unwrap()
            .query_row(
                "SELECT count(*) FROM project_agent_deliveries delivery \
                 JOIN project_agent_messages message \
                   ON message.owner_user_id = delivery.owner_user_id \
                  AND message.id = delivery.message_id \
                 WHERE message.causation_id = 'invocation-routing-limit'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stopped_delivery_count, 0);
    }

    #[test]
    fn agent_documents_are_run_scoped_idempotent_and_limited() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "sender", "active");
        let team = store
            .create_project_team("alice", "project-1", "文档团队", "验证文档草稿")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "sender")
            .unwrap();
        store
            .queue_project_team_message("alice", &team.id, "创建文档", &["sender".into()])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery("alice", &delivery, "turn-doc", "run-doc", 10)
            .unwrap();

        let (_, first) = store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                "create-doc-1",
                "../../交付:说明",
                "交付说明",
                "# 交付\n正文",
                11,
            )
            .unwrap();
        assert_eq!(first["name"], "交付-说明.md");
        assert_eq!(first["mime_type"], "text/markdown");
        let (_, replayed) = store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                "create-doc-1",
                "../../交付:说明",
                "交付说明",
                "# 交付\n正文",
                12,
            )
            .unwrap();
        assert_eq!(replayed, first);
        assert!(store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                "create-doc-1",
                "不同名称",
                "交付说明",
                "# 交付\n正文",
                12,
            )
            .is_err());

        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "DELETE FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-doc'",
                [],
            )
            .unwrap();
        drop(connection);
        let maximum_document = "x".repeat(2 * 1_024 * 1_024);
        for index in 1..=4 {
            store
                .create_agent_chat_document(
                    "alice",
                    "run-doc",
                    &format!("large-doc-{index}"),
                    &format!("大文档-{index}"),
                    &format!("大文档 {index}"),
                    &maximum_document,
                    40 + index,
                )
                .unwrap();
        }
        assert!(store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                "run-bytes-overflow",
                "总量超限",
                "总量超限",
                "x",
                45,
            )
            .is_err());
        assert!(store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                "too-large",
                "超限",
                "超限",
                &"x".repeat(2 * 1_024 * 1_024 + 1),
                12,
            )
            .is_err());

        let connection = Connection::open(store.database()).unwrap();
        connection
            .execute(
                "DELETE FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-doc'",
                [],
            )
            .unwrap();
        drop(connection);
        for index in 1..=20 {
            store
                .create_agent_chat_document(
                    "alice",
                    "run-doc",
                    &format!("create-doc-{index}"),
                    &format!("文档-{index}"),
                    &format!("文档 {index}"),
                    "正文",
                    10 + index,
                )
                .unwrap();
        }
        assert!(store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                "create-doc-21",
                "文档-21",
                "文档 21",
                "正文",
                31,
            )
            .is_err());
    }

    #[test]
    fn agent_documents_attach_to_team_messages_and_consume_once() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "sender", "active");
        let team = store
            .create_project_team("alice", "project-1", "文档团队", "接收文档")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "sender")
            .unwrap();
        store
            .queue_project_team_message("alice", &team.id, "请提交说明", &["sender".into()])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery("alice", &delivery, "turn-team-doc", "run-team-doc", 10)
            .unwrap();
        let (_, document) = store
            .create_agent_chat_document(
                "alice",
                "run-team-doc",
                "create-team-doc",
                "team-notes",
                "团队说明",
                "# 团队说明\n真实正文",
                11,
            )
            .unwrap();
        let document_reference = document["document_ref"].as_str().unwrap().to_owned();
        let connection = Connection::open(store.database()).unwrap();
        let draft_path: String = connection
            .query_row(
                "SELECT relative_path FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-team-doc'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        let absolute_draft_path = store
            .database()
            .parent()
            .unwrap()
            .join("AgentMessageAttachments")
            .join(draft_path);
        assert!(absolute_draft_path.is_file());
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-team-doc",
                "duplicate-team-doc",
                "不得重复引用。",
                None,
                None,
                None,
                &[document_reference.clone(), document_reference.clone()],
                &[],
                12,
            )
            .is_err());

        let (_, sent) = store
            .send_team_chat_message(
                "alice",
                "run-team-doc",
                "send-team-doc",
                "说明已完成。",
                None,
                None,
                None,
                std::slice::from_ref(&document_reference),
                &[],
                12,
            )
            .unwrap();
        assert!(!absolute_draft_path.exists());
        let (_, replayed) = store
            .send_team_chat_message(
                "alice",
                "run-team-doc",
                "send-team-doc",
                "说明已完成。",
                None,
                None,
                None,
                std::slice::from_ref(&document_reference),
                &[],
                13,
            )
            .unwrap();
        assert_eq!(replayed, sent);
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-team-doc",
                "reuse-consumed-doc",
                "不得再次消费。",
                None,
                None,
                None,
                std::slice::from_ref(&document_reference),
                &[],
                14,
            )
            .is_err());

        let connection = Connection::open(store.database()).unwrap();
        let attachment: (i64, String, String) = connection
            .query_row(
                "SELECT count(*), min(attachment.mime_type), min(room.conversation_kind) \
                 FROM project_agent_message_attachments attachment \
                 JOIN project_agent_messages message \
                   ON message.owner_user_id = attachment.owner_user_id \
                  AND message.id = attachment.message_id \
                 JOIN project_agent_rooms room ON room.owner_user_id = message.owner_user_id \
                  AND room.id = message.room_id \
                 WHERE message.causation_id = 'send-team-doc'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            attachment,
            (
                1,
                "text/markdown; charset=utf-8".into(),
                "project_team".into()
            )
        );
    }

    #[test]
    fn direct_message_documents_issue_refs_page_text_and_reject_mismatches() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "sender", "active");
        insert_agent(&store, "alice", "target", "active");
        let team = store
            .create_project_team("alice", "project-1", "私聊文档团队", "发送私聊文档")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "sender")
            .unwrap();
        store
            .queue_project_team_message("alice", &team.id, "发送两份文档", &["sender".into()])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery(
                "alice",
                &delivery,
                "turn-sender-doc",
                "run-sender-doc",
                10,
            )
            .unwrap();
        let (_, workspace) = store
            .agent_workspace_snapshot("alice", "run-sender-doc")
            .unwrap();
        let target_reference = workspace["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["is_current_agent"] == false)
            .unwrap()["agent_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        for (index, markdown) in [(1, "abcdef甲乙"), (2, "第二份正文")] {
            let (_, document) = store
                .create_agent_chat_document(
                    "alice",
                    "run-sender-doc",
                    &format!("create-direct-doc-{index}"),
                    &format!("direct-{index}"),
                    &format!("私聊文档 {index}"),
                    markdown,
                    10 + index,
                )
                .unwrap();
            let reference = document["document_ref"].as_str().unwrap().to_owned();
            store
                .send_team_chat_message(
                    "alice",
                    "run-sender-doc",
                    &format!("send-direct-doc-{index}"),
                    &format!("发送文档 {index}"),
                    None,
                    Some(&target_reference),
                    None,
                    std::slice::from_ref(&reference),
                    &[],
                    12 + index,
                )
                .unwrap();
        }
        let (_, unsent_document) = store
            .create_agent_chat_document(
                "alice",
                "run-sender-doc",
                "create-unsent-doc",
                "unsent",
                "未发送文档",
                "仅属于发送方 Run",
                15,
            )
            .unwrap();
        let unsent_document_reference =
            unsent_document["document_ref"].as_str().unwrap().to_owned();
        let connection = Connection::open(store.database()).unwrap();
        let direct_room_id: String = connection
            .query_row(
                "SELECT id FROM project_agent_rooms WHERE owner_user_id = 'alice' \
                 AND conversation_kind = 'agent_agent_direct'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        let incoming = store
            .next_agent_communication_delivery("alice", &direct_room_id)
            .unwrap()
            .unwrap();
        store
            .claim_agent_communication_delivery(
                "alice",
                &incoming,
                "turn-target-doc",
                "run-target-doc",
                20,
            )
            .unwrap();
        let (_, unread) = store
            .read_all_unread_team_chat("alice", "run-target-doc", "read-direct-docs", 20, 21)
            .unwrap();
        assert_eq!(unread["message_count"], 2);
        let messages = unread["conversations"][0]["messages"].as_array().unwrap();
        let first_message = messages
            .iter()
            .find(|message| message["content"] == "发送文档 1")
            .unwrap();
        let second_message = messages
            .iter()
            .find(|message| message["content"] == "发送文档 2")
            .unwrap();
        let first_message_reference = first_message["message_ref"].as_str().unwrap();
        let first_attachment_reference = first_message["attachments"][0]["attachment_ref"]
            .as_str()
            .unwrap();
        let second_attachment_reference = second_message["attachments"][0]["attachment_ref"]
            .as_str()
            .unwrap();
        let (_, first_page) = store
            .read_agent_chat_attachment(
                "alice",
                "run-target-doc",
                first_message_reference,
                first_attachment_reference,
                0,
                4,
            )
            .unwrap();
        assert_eq!(first_page["content"], "abcd");
        assert_eq!(first_page["next_offset"], 4);
        assert_eq!(first_page["has_more"], true);
        assert!(store
            .read_agent_chat_attachment(
                "alice",
                "run-target-doc",
                first_message_reference,
                second_attachment_reference,
                0,
                12_000,
            )
            .is_err());
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-target-doc",
                "cross-run-document",
                "不得消费其他 Run 的草稿。",
                None,
                None,
                None,
                std::slice::from_ref(&unsent_document_reference),
                &[],
                22,
            )
            .is_err());
        let unsent_status: String = Connection::open(store.database())
            .unwrap()
            .query_row(
                "SELECT status FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-sender-doc' \
                   AND document_ref = ?1",
                params![unsent_document_reference],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unsent_status, "available");

        let connection = Connection::open(store.database()).unwrap();
        let relative_path: String = connection
            .query_row(
                "SELECT attachment.relative_path \
                 FROM project_agent_message_attachments attachment \
                 JOIN windows_agent_chat_attachment_references reference \
                   ON reference.owner_user_id = attachment.owner_user_id \
                  AND reference.attachment_id = attachment.id \
                 WHERE reference.owner_user_id = 'alice' \
                   AND reference.run_id = 'run-target-doc' \
                   AND reference.attachment_ref = ?1",
                params![first_attachment_reference],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        let attachment_path = store
            .database()
            .parent()
            .unwrap()
            .join("AgentMessageAttachments")
            .join(relative_path);
        let size = fs::metadata(&attachment_path).unwrap().len() as usize;
        fs::write(&attachment_path, vec![b'x'; size]).unwrap();
        assert!(store
            .read_agent_chat_attachment(
                "alice",
                "run-target-doc",
                first_message_reference,
                first_attachment_reference,
                0,
                12_000,
            )
            .is_err());
    }

    #[test]
    fn due_heartbeats_create_one_durable_cycle_and_advance_the_schedule() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "heartbeat-agent", "active");
        Connection::open(store.database())
            .unwrap()
            .execute(
                "UPDATE local_agent_profiles SET heartbeat_enabled = 1, \
                 heartbeat_interval_seconds = 60, heartbeat_prompt = '检查全局未读', \
                 next_heartbeat_at_unix_ms = 10 \
                 WHERE owner_user_id = 'alice' AND id = 'heartbeat-agent'",
                [],
            )
            .unwrap();

        assert!(store
            .enqueue_due_agent_heartbeats("alice", 9, 16)
            .unwrap()
            .is_empty());
        let rooms = store.enqueue_due_agent_heartbeats("alice", 10, 16).unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(
            store
                .communication_rooms_requiring_scheduler("alice")
                .unwrap(),
            rooms
        );
        let schedule: (Option<i64>, Option<i64>) = Connection::open(store.database())
            .unwrap()
            .query_row(
                "SELECT last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms \
                 FROM local_agent_profiles WHERE owner_user_id = 'alice' \
                   AND id = 'heartbeat-agent'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(schedule, (Some(10), Some(60_010)));

        assert!(store
            .enqueue_due_agent_heartbeats("alice", 60_010, 16)
            .unwrap()
            .is_empty());
        let connection = Connection::open(store.database()).unwrap();
        let facts: (i64, i64, Option<i64>) = connection
            .query_row(
                "SELECT count(*), \
                        sum(CASE WHEN trigger_kind = 'heartbeat' THEN 1 ELSE 0 END), \
                        (SELECT next_heartbeat_at_unix_ms FROM local_agent_profiles \
                         WHERE owner_user_id = 'alice' AND id = 'heartbeat-agent') \
                 FROM project_agent_deliveries WHERE owner_user_id = 'alice'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(facts, (1, 1, Some(120_010)));
        drop(connection);

        let pending = store
            .next_agent_communication_delivery("alice", &rooms[0])
            .unwrap()
            .unwrap();
        assert_eq!(pending.trigger_kind, "heartbeat");
        assert_eq!(pending.conversation_kind, "human_agent_direct");
        assert_eq!(pending.content, "检查全局未读");
        store
            .claim_agent_communication_delivery(
                "alice",
                &pending,
                "turn-heartbeat",
                "run-heartbeat",
                60_011,
            )
            .unwrap();
        let (binding, outcome) = store
            .validate_agent_communication_cycle_completion("alice", "run-heartbeat", true)
            .unwrap();
        assert_eq!(binding.conversation_kind, "human_agent_direct");
        assert_eq!(outcome["status"], "completed");
        store
            .reconcile_agent_communication_runs(
                "alice",
                &rooms[0],
                &[AgentCommunicationRunReconciliation {
                    delivery_id: pending.delivery_id,
                    turn_id: "turn-heartbeat".into(),
                    run_id: "run-heartbeat".into(),
                    status: LocalConversationTurnStatus::Succeeded,
                    created_at_unix_ms: 60_011,
                    updated_at_unix_ms: 60_012,
                }],
            )
            .unwrap();
        assert!(!store
            .communication_scheduler_has_work("alice", &rooms[0])
            .unwrap());
    }

    #[test]
    fn agent_direct_messages_create_two_member_rooms_and_routable_deliveries() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "sender", "active");
        insert_agent(&store, "alice", "target", "active");
        let team = store
            .create_project_team("alice", "project-1", "通讯团队", "启动私聊")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", &team.id, "sender")
            .unwrap();
        let trigger = store
            .queue_project_team_message("alice", &team.id, "请联系目标 Agent", &["sender".into()])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &team.id)
            .unwrap()
            .unwrap();
        assert_eq!(delivery.message_id, trigger.message_id);
        store
            .claim_project_team_delivery("alice", &delivery, "turn-sender", "run-sender", 10)
            .unwrap();
        let (_, workspace) = store
            .agent_workspace_snapshot("alice", "run-sender")
            .unwrap();
        let target_reference = workspace["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["is_current_agent"] == false)
            .unwrap()["agent_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let sender_reference = workspace["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["is_current_agent"] == true)
            .unwrap()["agent_ref"]
            .as_str()
            .unwrap()
            .to_owned();

        let (_, sent) = store
            .send_team_chat_message(
                "alice",
                "run-sender",
                "invocation-direct-1",
                "这是一条真实私聊。",
                None,
                Some(&target_reference),
                None,
                &[],
                &[],
                11,
            )
            .unwrap();
        assert_eq!(sent["spawned_delivery_count"], 1);
        let (_, replayed) = store
            .send_team_chat_message(
                "alice",
                "run-sender",
                "invocation-direct-1",
                "这是一条真实私聊。",
                None,
                Some(&target_reference),
                None,
                &[],
                &[],
                12,
            )
            .unwrap();
        assert_eq!(replayed, sent);
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-sender",
                "invocation-direct-forged",
                "不得发送。",
                None,
                Some("agent_ref_forged"),
                None,
                &[],
                &[],
                12,
            )
            .is_err());
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-sender",
                "invocation-direct-self",
                "不得发送给自己。",
                None,
                Some(&sender_reference),
                None,
                &[],
                &[],
                12,
            )
            .is_err());
        let connection = Connection::open(store.database()).unwrap();
        let (direct_room_id, conversation_kind, direct_key): (String, String, String) = connection
            .query_row(
                "SELECT id, conversation_kind, direct_key FROM project_agent_rooms \
                 WHERE owner_user_id = 'alice' AND conversation_kind = 'agent_agent_direct'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(conversation_kind, "agent_agent_direct");
        assert_eq!(direct_key, "agent:sender|agent:target");
        let member_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = 'alice' AND room_id = ?1 AND status = 'active'",
                params![direct_room_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(member_count, 2);
        drop(connection);
        assert_eq!(
            store
                .communication_rooms_requiring_scheduler("alice")
                .unwrap(),
            vec![direct_room_id.clone()]
        );

        let pending = store
            .next_agent_communication_delivery("alice", &direct_room_id)
            .unwrap()
            .unwrap();
        assert_eq!(pending.binding.agent_id, "target");
        assert_eq!(pending.trigger_kind, "default_agent");
        store
            .claim_agent_communication_delivery("alice", &pending, "turn-target", "run-target", 12)
            .unwrap();
        assert!(store
            .validate_agent_communication_cycle_completion("alice", "run-target", true)
            .is_err());
        assert_eq!(
            store
                .validate_agent_communication_cycle_completion("alice", "run-target", false)
                .unwrap()
                .1["status"],
            "completed"
        );
        let (binding, unread) = store
            .read_all_unread_team_chat("alice", "run-target", "read-direct", 20, 13)
            .unwrap();
        assert_eq!(binding.conversation_kind, "agent_agent_direct");
        assert_eq!(unread["message_count"], 1);
        assert_eq!(unread["conversations"][0]["kind"], "agent_agent_direct");
        let (_, direct_members) = store
            .team_chat_member_snapshot("alice", "run-target")
            .unwrap();
        assert_eq!(direct_members.as_array().unwrap().len(), 2);
        assert_eq!(
            store
                .active_agent_communication_run_bindings("alice", &direct_room_id)
                .unwrap()[0]
                .conversation_id,
            agent_communication_conversation_id(&direct_room_id, "target")
        );
        let (_, reply) = store
            .send_team_chat_message(
                "alice",
                "run-target",
                "invocation-direct-reply",
                "已收到并回复。",
                None,
                None,
                None,
                &[],
                &[],
                14,
            )
            .unwrap();
        assert_eq!(reply["spawned_delivery_count"], 1);
        store
            .reconcile_agent_communication_runs(
                "alice",
                &direct_room_id,
                &[AgentCommunicationRunReconciliation {
                    delivery_id: pending.delivery_id,
                    turn_id: "turn-target".into(),
                    run_id: "run-target".into(),
                    status: LocalConversationTurnStatus::Succeeded,
                    created_at_unix_ms: 12,
                    updated_at_unix_ms: 15,
                }],
            )
            .unwrap();
        store
            .reconcile_project_team_runs(
                "alice",
                &team.id,
                &[team_todo::TeamRunReconciliation {
                    delivery_id: delivery.delivery_id,
                    turn_id: "turn-sender".into(),
                    run_id: "run-sender".into(),
                    status: LocalConversationTurnStatus::Succeeded,
                    created_at_unix_ms: 10,
                    updated_at_unix_ms: 15,
                    project_write_evidence_verified: false,
                }],
            )
            .unwrap();
        let reply_pending = store
            .next_agent_communication_delivery("alice", &direct_room_id)
            .unwrap()
            .unwrap();
        assert_eq!(reply_pending.binding.agent_id, "sender");
        assert!(store
            .communication_scheduler_has_work("alice", &direct_room_id)
            .unwrap());
    }

    #[test]
    fn team_unread_messages_advance_cursors_and_route_replies_by_run_scoped_reference() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "sender", "active");
        insert_agent(&store, "alice", "target", "active");
        let first_team = store
            .create_project_team("alice", "project-1", "第一团队", "接收唤醒")
            .unwrap();
        let second_team = store
            .create_project_team("alice", "project-2", "第二团队", "接收跨团队回复")
            .unwrap();
        for team_id in [&first_team.id, &second_team.id] {
            store
                .invite_agent_to_project_team("alice", team_id, "sender")
                .unwrap();
            store
                .invite_agent_to_project_team("alice", team_id, "target")
                .unwrap();
        }
        let trigger = store
            .queue_project_team_message("alice", &first_team.id, "处理全部未读", &["sender".into()])
            .unwrap();
        let delivery = store
            .next_project_team_delivery("alice", &first_team.id)
            .unwrap()
            .unwrap();
        store
            .claim_project_team_delivery("alice", &delivery, "turn-unread", "run-unread", 10)
            .unwrap();
        let other = store
            .queue_project_team_message(
                "alice",
                &second_team.id,
                "回复第二团队",
                &["sender".into()],
            )
            .unwrap();
        let (_, workspace) = store
            .agent_workspace_snapshot("alice", "run-unread")
            .unwrap();
        assert!(!workspace.to_string().contains(&first_team.id));
        assert!(!workspace.to_string().contains(&second_team.id));
        assert!(!workspace.to_string().contains("\"sender\""));
        assert!(!workspace.to_string().contains("\"target\""));
        let target_reference = workspace["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["is_current_agent"] == false)
            .unwrap()["agent_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let second_team_reference = workspace["teams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|team| team["name"] == "第二团队")
            .unwrap()["team_ref"]
            .as_str()
            .unwrap()
            .to_owned();

        let (_, unread) = store
            .read_all_unread_team_chat("alice", "run-unread", "read-unread-1", 200, 11)
            .unwrap();
        assert_eq!(unread["message_count"], 2);
        assert_eq!(unread["marked_read"], true);
        assert!(!unread.to_string().contains(&trigger.message_id));
        assert!(!unread.to_string().contains(&other.message_id));
        assert!(!unread.to_string().contains(&first_team.id));
        assert!(!unread.to_string().contains(&second_team.id));
        let other_reference = unread["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|conversation| conversation["messages"].as_array().unwrap())
            .find(|message| message["content"] == "回复第二团队")
            .unwrap()["message_ref"]
            .as_str()
            .unwrap()
            .to_owned();

        let (_, sent) = store
            .send_team_chat_message(
                "alice",
                "run-unread",
                "invocation-reply-other-team",
                "第二团队已收到。",
                Some(&other_reference),
                None,
                None,
                &[],
                std::slice::from_ref(&target_reference),
                12,
            )
            .unwrap();
        assert_eq!(sent["spawned_delivery_count"], 1);
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-unread",
                "invocation-forged-reply",
                "不得写入。",
                Some("message_ref_forged"),
                None,
                None,
                &[],
                &[],
                13,
            )
            .is_err());
        let (_, proactive) = store
            .send_team_chat_message(
                "alice",
                "run-unread",
                "invocation-proactive-team",
                "主动同步第二团队。",
                None,
                None,
                Some(&second_team_reference),
                &[],
                std::slice::from_ref(&target_reference),
                13,
            )
            .unwrap();
        assert_eq!(proactive["spawned_delivery_count"], 1);
        assert!(store
            .send_team_chat_message(
                "alice",
                "run-unread",
                "invocation-forged-team",
                "不得写入。",
                None,
                None,
                Some("team_ref_forged"),
                &[],
                &[],
                13,
            )
            .is_err());
        let connection = Connection::open(store.database()).unwrap();
        let routed: (String, String, String) = connection
            .query_row(
                "SELECT room_id, reply_to_message_id, root_message_id \
                 FROM project_agent_messages WHERE owner_user_id = 'alice' \
                   AND causation_id = 'invocation-reply-other-team'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            routed,
            (
                second_team.id.clone(),
                other.message_id.clone(),
                other.message_id.clone(),
            )
        );
        let proactive_routing: (String, Option<String>, String, String) = connection
            .query_row(
                "SELECT room_id, reply_to_message_id, root_message_id, id \
                 FROM project_agent_messages WHERE owner_user_id = 'alice' \
                   AND causation_id = 'invocation-proactive-team'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(proactive_routing.0, second_team.id);
        assert_eq!(proactive_routing.1, None);
        assert_eq!(proactive_routing.2, proactive_routing.3);
        let cursor_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_read_cursors \
                 WHERE owner_user_id = 'alice' AND agent_id = 'sender'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cursor_count, 2);
        drop(connection);

        let (_, replayed) = store
            .read_all_unread_team_chat("alice", "run-unread", "read-unread-1", 200, 14)
            .unwrap();
        assert_eq!(replayed, unread);
        assert!(store
            .read_all_unread_team_chat("alice", "run-unread", "read-unread-1", 1, 14)
            .is_err());
        let (_, after) = store
            .read_all_unread_team_chat("alice", "run-unread", "read-unread-2", 200, 14)
            .unwrap();
        assert_eq!(after["message_count"], 0);
        assert_eq!(after["conversations"], json!([]));
    }

    #[test]
    fn team_creation_requires_an_active_account_scoped_project() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("Projects.sqlite3");
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(crate::workspace_index::PROJECT_REGISTRY_SCHEMA)
            .unwrap();
        connection
            .execute(
                "INSERT INTO local_project_records VALUES \
                 ('alice', 'project-1', 'Alpha', '', 'workspace-1', '', \
                  'software_development', 1, 'active', 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO local_project_records VALUES \
                 ('alice', 'removed', 'Removed', '', 'workspace-1', '', \
                  'software_development', 1, 'removed', 1, 1)",
                [],
            )
            .unwrap();
        drop(connection);

        assert!(require_active_project(&database, "alice", "project-1").is_ok());
        assert!(require_active_project(&database, "bob", "project-1").is_err());
        assert!(require_active_project(&database, "alice", "removed").is_err());
    }

    #[test]
    fn direct_delivery_persists_and_reconciles_from_the_bound_host_turn() {
        let (_directory, store) = store();
        insert_agent(&store, "alice", "agent-1", "active");
        let room = store.open_human_agent_direct("alice", "agent-1").unwrap();
        let binding = store.load_execution_binding("alice", &room.id).unwrap();
        store
            .record_direct_delivery(
                "alice",
                &binding,
                "delivery-1",
                "message-1",
                "turn-1",
                "run-1",
                "请完成任务",
                100,
            )
            .unwrap();
        assert!(store
            .communication_rooms_requiring_scheduler("alice")
            .unwrap()
            .is_empty());
        assert!(store
            .next_agent_communication_delivery("alice", &room.id)
            .unwrap()
            .is_none());

        let pending = store.list_direct_activity("alice", &binding).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].delivery_status, "pending");
        assert_eq!(pending[0].run_status, "ready");

        let detail = LocalConversationDetail {
            conversation: chatos_local_agent_protocol::LocalConversationRecord {
                conversation_id: format!("agent-direct-{}", room.id),
                owner_user_id: "alice".into(),
                title: "测试 Agent".into(),
                resource: None,
                version: 2,
                created_at_unix_ms: 100,
                updated_at_unix_ms: 220,
            },
            turns: vec![LocalConversationTurnRecord {
                turn_id: "turn-1".into(),
                conversation_id: format!("agent-direct-{}", room.id),
                user_message_id: "message-1".into(),
                run_id: "run-1".into(),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms: 110,
                updated_at_unix_ms: 220,
            }],
            messages: vec![
                chatos_local_agent_protocol::LocalConversationMessageRecord {
                    message_id: "answer-1".into(),
                    conversation_id: format!("agent-direct-{}", room.id),
                    turn_id: "turn-1".into(),
                    ordinal: 2,
                    role: LocalConversationMessageRole::Assistant,
                    content: json!({"text": "已完成"}),
                    metadata: json!({}),
                    created_at_unix_ms: 210,
                },
            ],
            attachments: vec![],
        };
        let completed = store
            .reconcile_direct_activity("alice", &binding, &detail)
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].delivery_status, "completed");
        assert_eq!(completed[0].run_status, "succeeded");
        assert_eq!(
            completed[0].response_message_id.as_deref(),
            Some("answer-1")
        );
        assert_eq!(completed[0].updated_at_unix_ms, 220);

        let connection = Connection::open(store.database()).unwrap();
        let response: (String, String, String) = connection
            .query_row(
                "SELECT sender_kind, sender_id, content FROM project_agent_messages \
                 WHERE owner_user_id = 'alice' AND id = 'answer-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            response,
            ("agent".into(), "agent-1".into(), "已完成".into())
        );
        assert_eq!(
            store.activity_room_ids("alice", "agent-1").unwrap(),
            vec![room.id]
        );
        insert_agent(&store, "alice", "agent-2", "active");
        assert!(store
            .activity_room_ids("alice", "agent-2")
            .unwrap()
            .is_empty());
        assert!(store.activity_room_ids("bob", "agent-1").is_err());
    }
}
