use chatos_local_agent_protocol::{
    CreateConversationCommand, GetConversationCommand, GetConversationRuntimeSettingsCommand,
    GetLatestCapabilityPolicySnapshotCommand, GuideConversationTurnCommand, HostCommand,
    HostResult, ListLatestModelConfigSnapshotsCommand, LocalConversationAttachmentSpec,
    LocalConversationDetail, LocalConversationMessageRole, LocalConversationRuntimeSettings,
    LocalConversationTurnRecord, LocalConversationTurnStatus, LocalModelConfigSnapshot,
    PutConversationRuntimeSettingsCommand, StartConversationTurnCommand,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
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
    binding: TeamExecutionBinding,
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
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = delivery.owner_user_id
                      AND active.target_agent_id = delivery.target_agent_id
                      AND active.status = 'running' AND active.trigger_kind != 'todo'
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
                        binding: TeamExecutionBinding {
                            agent: AgentExecutionBinding {
                                room_id: row.get(3)?,
                                agent_id: row.get(4)?,
                                name: row.get(5)?,
                                role_prompt: row.get(6)?,
                                model_config_id: row.get(7)?,
                                thinking_level: row.get(8)?,
                                profession_key: row.get(9)?,
                                default_plugin_ids: parse_identifier_array(
                                    &row.get::<_, String>(10)?,
                                )
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                                default_skill_ids: parse_identifier_array(
                                    &row.get::<_, String>(11)?,
                                )
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            },
                            project_id: row.get(12)?,
                            team_name: row.get(13)?,
                            team_goal: row.get(14)?,
                            team_role: row.get(15)?,
                            team_responsibility: row.get(16)?,
                            is_project_manager: row.get::<_, i64>(17)? == 1,
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
        validate_team_execution_binding(&pending.binding)?;
        validate_text(&pending.content, 1_000_000, false).map_err(|_| team_store_error())?;
        Ok(Some(pending))
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
                     AND active.trigger_kind != 'todo')",
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
        let connection = open_database(&self.database, false)?;
        connection
            .execute(
                "UPDATE project_agent_deliveries SET status = 'failed', last_error = ?1, \
                 completed_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
                   AND status = 'running'",
                params![message, now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        connection
            .execute(
                "UPDATE windows_agent_delivery_runs SET status = 'failed', updated_at_unix_ms = ?1 \
                 WHERE owner_user_id = ?2 AND delivery_id = ?3 AND status IN ('ready', 'running')",
                params![now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        Ok(())
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
    let conversation_id = format!(
        "agent-team-{}-{}",
        pending.binding.agent.room_id, pending.binding.agent.agent_id
    );
    ensure_agent_host_conversation(
        &manager,
        &owner_user_id,
        &conversation_id,
        &format!(
            "{} · {}",
            pending.binding.team_name, pending.binding.agent.name
        ),
    )
    .await?;
    let detail = agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?;
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
    let capability_revision = latest_main_chat_capability(&manager, &owner_user_id).await?;
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
        message_metadata: team_agent_execution_metadata(&pending.binding, &profession),
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
    let capability_revision = latest_main_chat_capability(&manager, &input.owner_user_id).await?;
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

async fn latest_main_chat_capability(
    manager: &HostManager,
    owner_user_id: &str,
) -> Result<String, String> {
    let result = manager
        .execute(HostCommand::GetLatestCapabilityPolicySnapshot(
            GetLatestCapabilityPolicySnapshotCommand {
                owner_user_id: owner_user_id.to_owned(),
                profile_key: MAIN_CHAT_PROFILE.into(),
            },
        ))
        .await?;
    let HostResult::CapabilityPolicySnapshot { snapshot } = result else {
        return Err("Local Agent Host 返回了错误的能力配置响应。".into());
    };
    if snapshot.owner_user_id != owner_user_id || snapshot.profile_key != MAIN_CHAT_PROFILE {
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
    binding: &TeamExecutionBinding,
    profession: &AgentProfessionRuntimeSnapshot,
) -> serde_json::Value {
    json!({
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

fn validate_team_execution_binding(binding: &TeamExecutionBinding) -> Result<(), String> {
    let agent = &binding.agent;
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
    validate_identifier(&binding.project_id).map_err(|_| team_store_error())?;
    validate_text(&binding.team_name, 160, false).map_err(|_| team_store_error())?;
    validate_text(&binding.team_goal, 8_000, true).map_err(|_| team_store_error())?;
    validate_text(&binding.team_role, 160, false).map_err(|_| team_store_error())?;
    validate_text(&binding.team_responsibility, 8_000, true).map_err(|_| team_store_error())
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
