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
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

use crate::{
    agent_skill_management::{AgentProfessionRuntimeSnapshot, AgentSkillStore},
    attachment_tool_worker::AttachmentToolWorker,
    authentication::AuthenticationManager,
    connector_storage,
    conversation_attachments::{AttachmentManager, AttachmentScope},
    general_settings::GeneralSettingsStore,
    host_lifecycle::{validate_owner, HostManager},
    terminal_tool_worker::TerminalToolWorker,
};

pub(crate) mod documents;
pub(crate) mod heartbeat;
pub(crate) mod proposals;
pub(crate) mod team_todo;

const DATABASE_FILENAME: &str = "AgentGroupChat.sqlite3";
const MAIN_CHAT_PROFILE: &str = "main_chat";
const MAX_ITERATIONS: u32 = 32;
const SCHEMA: &str = include_str!("agent_conversation/schema.sql");

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
    #[serde(default)]
    attachment_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CancelAgentTeamRunInput {
    owner_user_id: String,
    team_id: String,
    run_id: String,
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
    attachments: Vec<AgentTeamMessageAttachmentSummary>,
    created_at_unix_ms: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamMessageAttachmentSummary {
    id: String,
    name: String,
    mime_type: String,
    size: i64,
    kind: String,
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
    has_older: bool,
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
    pub(crate) team_id: String,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTeamAssetUpdateSuggestion {
    pub(crate) category: String,
    pub(crate) title: String,
    pub(crate) markdown: String,
    pub(crate) rationale: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamAssetReview {
    pub(crate) id: String,
    pub(crate) team_id: String,
    pub(crate) todo_id: String,
    pub(crate) todo_title: String,
    pub(crate) source_agent_id: String,
    pub(crate) source_agent_name: String,
    pub(crate) suggestion: AgentTeamAssetUpdateSuggestion,
    pub(crate) status: String,
    pub(crate) resolved_asset_id: Option<String>,
    pub(crate) resolved_asset_revision: Option<i64>,
    pub(crate) reviewed_by_agent_id: Option<String>,
    pub(crate) review_reason: String,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) updated_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentProjectMilestone {
    pub(crate) id: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) detail: String,
    pub(crate) status: String,
    pub(crate) progress_percent: i64,
    #[serde(default)]
    pub(crate) acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub(crate) linked_todo_ids: Vec<String>,
    pub(crate) target_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentProjectIssue {
    pub(crate) id: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) detail: String,
    pub(crate) requested_action: String,
    pub(crate) severity: String,
    pub(crate) owner: String,
    pub(crate) related_todo_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentProjectDashboardUpdate {
    pub(crate) phase: String,
    pub(crate) health: String,
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) next_steps: Vec<String>,
    #[serde(default)]
    pub(crate) milestones: Vec<AgentProjectMilestone>,
    #[serde(default)]
    pub(crate) issues: Vec<AgentProjectIssue>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProjectDashboard {
    team_id: String,
    phase: String,
    health: String,
    summary: String,
    next_steps: Vec<String>,
    milestones: Vec<AgentProjectMilestone>,
    issues: Vec<AgentProjectIssue>,
    revision: i64,
    updated_by_agent_id: String,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProjectDashboardSnapshot {
    pub(crate) dashboard: Option<AgentProjectDashboard>,
    pub(crate) pending_approval_count: i64,
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
    pub(crate) team_id: String,
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

    pub(crate) async fn cleanup_stale_attachment_files(&self) -> usize {
        let database = self.database.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let cutoff = SystemTime::now()
                .checked_sub(Duration::from_secs(24 * 60 * 60))
                .unwrap_or(UNIX_EPOCH);
            documents::cleanup_stale_attachment_files(&database, cutoff)
        })
        .await
        .unwrap_or_default()
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
                           AND delivery.trigger_kind IN ('heartbeat', 'mention')))
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
            "mention" | "default_agent" | "agent_mention" | "heartbeat"
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
                           AND delivery.trigger_kind IN ('heartbeat', 'mention')))
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

    #[cfg(test)]
    fn queue_project_team_message(
        &self,
        owner_user_id: &str,
        team_id: &str,
        content: &str,
        mentioned_agent_ids: &[String],
    ) -> Result<QueueAgentTeamMessageResult, String> {
        self.queue_project_team_message_with_attachments(
            owner_user_id,
            team_id,
            content,
            mentioned_agent_ids,
            &[],
        )
    }

    fn queue_project_team_message_with_attachments(
        &self,
        owner_user_id: &str,
        team_id: &str,
        content: &str,
        mentioned_agent_ids: &[String],
        attachments: &[crate::conversation_attachments::VerifiedAttachmentPayload],
    ) -> Result<QueueAgentTeamMessageResult, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_text(content, 1_000_000, true).map_err(|_| invalid_team_message())?;
        if content.is_empty() && attachments.is_empty() {
            return Err(invalid_team_message());
        }
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
        let persisted_attachment_paths = documents::persist_external_attachments(
            &transaction,
            &self.database,
            owner_user_id,
            &message_id,
            attachments,
        )?;
        if transaction.commit().is_err() {
            documents::remove_files(&persisted_attachment_paths);
            return Err(team_store_error());
        }
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
        before_message_id: Option<&str>,
        limit: u32,
    ) -> Result<AgentTeamConversationSnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        if let Some(message_id) = before_message_id {
            validate_identifier(message_id).map_err(|_| invalid_team_input())?;
        }
        let limit = limit.clamp(1, 200) as i64;
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
        let before_key = if let Some(message_id) = before_message_id {
            Some(
                connection
                    .query_row(
                        "SELECT created_at_unix_ms, id FROM project_agent_messages \
                         WHERE owner_user_id = ?1 AND room_id = ?2 AND id = ?3",
                        params![owner_user_id, team_id, message_id],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|_| team_store_error())?
                    .ok_or_else(|| "要加载的历史消息不存在。".to_owned())?,
            )
        } else {
            None
        };
        let message_rows = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT id, sender_kind, sender_id, content, created_at_unix_ms
                    FROM (
                        SELECT id, sender_kind, sender_id, content, created_at_unix_ms
                        FROM project_agent_messages
                        WHERE owner_user_id = ?1 AND room_id = ?2
                          AND (
                            ?3 IS NULL
                            OR created_at_unix_ms < ?3
                            OR (created_at_unix_ms = ?3 AND id < ?4)
                          )
                        ORDER BY created_at_unix_ms DESC, id DESC LIMIT ?5
                    ) recent
                    ORDER BY created_at_unix_ms, id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let (before_created_at, before_id) = before_key
                .as_ref()
                .map(|(created_at, id)| (Some(*created_at), Some(id.as_str())))
                .unwrap_or((None, None));
            let rows = statement
                .query_map(
                    params![
                        owner_user_id,
                        team_id,
                        before_created_at,
                        before_id,
                        limit + 1
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        let has_older = message_rows.len() as i64 > limit;
        let message_rows = if has_older {
            message_rows.into_iter().skip(1).collect::<Vec<_>>()
        } else {
            message_rows
        };
        let message_ids = message_rows
            .iter()
            .map(|row| row.0.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut mentions_by_message: std::collections::HashMap<String, Vec<(i64, String)>> =
            std::collections::HashMap::new();
        let mut attachments_by_message =
            std::collections::HashMap::<String, Vec<AgentTeamMessageAttachmentSummary>>::new();
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
        {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT attachment.message_id, attachment.id, attachment.name,
                           attachment.mime_type, attachment.size_bytes, attachment.kind
                    FROM project_agent_message_attachments attachment
                    JOIN project_agent_messages message
                      ON message.owner_user_id = attachment.owner_user_id
                     AND message.id = attachment.message_id
                    WHERE attachment.owner_user_id = ?1 AND message.room_id = ?2
                    ORDER BY attachment.message_id, attachment.position
                    LIMIT 4000
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        AgentTeamMessageAttachmentSummary {
                            id: row.get(1)?,
                            name: row.get(2)?,
                            mime_type: row.get(3)?,
                            size: row.get(4)?,
                            kind: row.get(5)?,
                        },
                    ))
                })
                .map_err(|_| team_store_error())?;
            for row in rows {
                let (message_id, attachment) = row.map_err(|_| team_store_error())?;
                if message_ids.contains(message_id.as_str()) {
                    attachments_by_message
                        .entry(message_id)
                        .or_default()
                        .push(attachment);
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
            let attachments = attachments_by_message.remove(&id).unwrap_or_default();
            messages.push(AgentTeamMessageSummary {
                id,
                sender_kind,
                sender_id,
                content,
                mentioned_agent_ids,
                attachments,
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
            has_older,
        })
    }

    fn load_message_attachment_payloads(
        &self,
        owner_user_id: &str,
        message_id: &str,
    ) -> Result<Vec<crate::conversation_attachments::VerifiedAttachmentPayload>, String> {
        validate_identifier(message_id).map_err(|_| team_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        documents::load_message_attachment_payloads(
            &connection,
            &self.database,
            owner_user_id,
            message_id,
        )
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

    pub(crate) fn load_project_dashboard_snapshot(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentProjectDashboardSnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let dashboard = read_project_dashboard(&connection, owner_user_id, team_id)?;
        let pending_approval_count = connection
            .query_row(
                r#"
                SELECT
                    (SELECT count(*) FROM local_agent_creation_proposals
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'pending') +
                    (SELECT count(*) FROM local_agent_removal_proposals
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'pending') +
                    (SELECT count(*) FROM local_agent_membership_proposals
                     WHERE owner_user_id = ?1 AND source_room_id = ?2 AND status = 'pending')
                "#,
                params![owner_user_id, team_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| team_store_error())?;
        Ok(AgentProjectDashboardSnapshot {
            dashboard,
            pending_approval_count,
        })
    }

    pub(crate) fn save_project_dashboard_as_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        editor_agent_id: &str,
        expected_revision: Option<i64>,
        update: AgentProjectDashboardUpdate,
        now_unix_ms: i64,
    ) -> Result<AgentProjectDashboard, String> {
        validate_identifier(team_id).map_err(|_| invalid_project_dashboard_input())?;
        validate_agent_identifier(editor_agent_id)
            .map_err(|_| invalid_project_dashboard_input())?;
        validate_project_dashboard_update(&update)?;
        if now_unix_ms < 0 {
            return Err(invalid_project_dashboard_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        require_project_team_manager(&transaction, owner_user_id, team_id, editor_agent_id)?;
        let existing = read_project_dashboard(&transaction, owner_user_id, team_id)?;
        let revision = match existing.as_ref() {
            Some(current) if expected_revision == Some(current.revision) => current.revision + 1,
            None if expected_revision.is_none() => 1,
            _ => return Err(project_dashboard_conflict()),
        };
        validate_project_dashboard_todo_links(&transaction, owner_user_id, team_id, &update)?;
        let encoded = serde_json::to_string(&update).map_err(|_| team_store_error())?;
        if let Some(current) = existing {
            let changed = transaction
                .execute(
                    r#"
                    UPDATE local_agent_project_dashboards
                    SET dashboard_json = ?1, revision = ?2, updated_by_agent_id = ?3,
                        updated_at_unix_ms = ?4
                    WHERE owner_user_id = ?5 AND team_room_id = ?6 AND revision = ?7
                    "#,
                    params![
                        encoded,
                        revision,
                        editor_agent_id,
                        now_unix_ms,
                        owner_user_id,
                        team_id,
                        current.revision,
                    ],
                )
                .map_err(|_| team_store_error())?;
            if changed != 1 {
                return Err(project_dashboard_conflict());
            }
        } else {
            transaction
                .execute(
                    r#"
                    INSERT INTO local_agent_project_dashboards (
                        owner_user_id, team_room_id, dashboard_json, revision,
                        updated_by_agent_id, updated_at_unix_ms
                    ) VALUES (?1, ?2, ?3, 1, ?4, ?5)
                    "#,
                    params![
                        owner_user_id,
                        team_id,
                        encoded,
                        editor_agent_id,
                        now_unix_ms,
                    ],
                )
                .map_err(|_| team_store_error())?;
        }
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(AgentProjectDashboard {
            team_id: team_id.to_owned(),
            phase: update.phase,
            health: update.health,
            summary: update.summary,
            next_steps: update.next_steps,
            milestones: update.milestones,
            issues: update.issues,
            revision,
            updated_by_agent_id: editor_agent_id.to_owned(),
            updated_at_unix_ms: now_unix_ms,
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
                    SELECT team_room_id, asset_id, category, title, markdown, revision
                    FROM local_agent_todo_asset_snapshots
                    WHERE owner_user_id = ?1 AND todo_id = ?2 AND team_room_id = ?3
                    ORDER BY category, title, asset_id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, todo_id, binding.team_id], |row| {
                    Ok(AgentAssetToolRecord {
                        team_id: row.get(0)?,
                        asset_id: row.get(1)?,
                        category: row.get(2)?,
                        title: row.get(3)?,
                        markdown: row.get(4)?,
                        revision: row.get(5)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        } else {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT asset.team_room_id, asset.id, asset.category, asset.title,
                           asset.markdown, asset.revision
                    FROM local_agent_team_assets asset
                    JOIN project_agent_rooms room
                      ON room.owner_user_id = asset.owner_user_id
                     AND room.id = asset.team_room_id
                    JOIN project_agent_room_members member
                      ON member.owner_user_id = room.owner_user_id
                     AND member.room_id = room.id AND member.agent_id = ?2
                    JOIN local_agent_profiles profile
                      ON profile.owner_user_id = member.owner_user_id
                     AND profile.id = member.agent_id
                    WHERE asset.owner_user_id = ?1 AND asset.status = 'active'
                      AND room.status = 'active' AND room.conversation_kind = 'project_team'
                      AND member.status = 'active' AND profile.status = 'active'
                    ORDER BY asset.team_room_id, asset.category, asset.title, asset.id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, binding.agent_id], |row| {
                    Ok(AgentAssetToolRecord {
                        team_id: row.get(0)?,
                        asset_id: row.get(1)?,
                        category: row.get(2)?,
                        title: row.get(3)?,
                        markdown: row.get(4)?,
                        revision: row.get(5)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        Ok((binding, records))
    }

    pub(crate) fn resolve_agent_asset_team_reference(
        &self,
        owner_user_id: &str,
        run_id: &str,
        reference: &str,
    ) -> Result<String, String> {
        validate_identifier(run_id).map_err(|_| team_store_error())?;
        validate_text(reference, 600, false).map_err(|_| team_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        if binding.todo_id.is_some() {
            let expected =
                team_todo::todo_planning_reference(owner_user_id, run_id, "team", &binding.team_id);
            return (reference == expected)
                .then_some(binding.team_id)
                .ok_or_else(|| "The team reference is unavailable for this Run.".to_owned());
        }
        team_todo::resolve_team_reference(
            &connection,
            owner_user_id,
            run_id,
            &binding.agent_id,
            reference,
        )
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

    pub(crate) fn list_team_asset_reviews(
        &self,
        owner_user_id: &str,
        team_id: Option<&str>,
        reviewer_agent_id: Option<&str>,
        status: &str,
        limit: usize,
    ) -> Result<Vec<AgentTeamAssetReview>, String> {
        if let Some(team_id) = team_id {
            validate_identifier(team_id).map_err(|_| invalid_team_asset_review_input())?;
        }
        if let Some(agent_id) = reviewer_agent_id {
            validate_identifier(agent_id).map_err(|_| invalid_team_asset_review_input())?;
        }
        if !matches!(status, "pending" | "applied" | "rejected") || !(1..=500).contains(&limit) {
            return Err(invalid_team_asset_review_input());
        }
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT review.id, review.team_room_id, review.todo_id, todo.title,
                       review.source_agent_id, profile.name, review.category, review.title,
                       review.markdown, review.rationale, review.status,
                       review.resolved_asset_id, review.resolved_asset_revision,
                       review.reviewed_by_agent_id, review.review_reason,
                       review.created_at_unix_ms, review.updated_at_unix_ms
                FROM local_agent_team_asset_reviews review
                JOIN local_agent_todos todo
                  ON todo.owner_user_id = review.owner_user_id AND todo.id = review.todo_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = review.owner_user_id
                 AND profile.id = review.source_agent_id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = review.owner_user_id
                 AND room.id = review.team_room_id
                WHERE review.owner_user_id = ?1 AND review.status = ?2
                  AND (?3 IS NULL OR review.team_room_id = ?3)
                  AND (?4 IS NULL OR room.project_manager_agent_id = ?4)
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                ORDER BY review.created_at_unix_ms, review.id
                LIMIT ?5
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(
                params![
                    owner_user_id,
                    status,
                    team_id,
                    reviewer_agent_id,
                    limit as i64
                ],
                map_team_asset_review,
            )
            .map_err(|_| team_store_error())?;
        let reviews = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for review in &reviews {
            validate_team_asset_review(review)?;
        }
        Ok(reviews)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_team_asset_review(
        &self,
        owner_user_id: &str,
        review_id: &str,
        reviewer_agent_id: &str,
        outcome: &str,
        resolved_asset: Option<(&str, i64)>,
        reason: &str,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAssetReview, String> {
        validate_identifier(review_id).map_err(|_| invalid_team_asset_review_input())?;
        validate_identifier(reviewer_agent_id).map_err(|_| invalid_team_asset_review_input())?;
        validate_text(reason, 4_000, true).map_err(|_| invalid_team_asset_review_input())?;
        if now_unix_ms < 0
            || !matches!(outcome, "applied" | "rejected")
            || (outcome == "applied" && resolved_asset.is_none())
            || (outcome == "rejected" && (resolved_asset.is_some() || reason.trim().is_empty()))
        {
            return Err(invalid_team_asset_review_input());
        }
        if let Some((asset_id, revision)) = resolved_asset {
            validate_identifier(asset_id).map_err(|_| invalid_team_asset_review_input())?;
            if revision <= 0 {
                return Err(invalid_team_asset_review_input());
            }
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let team_id: String = transaction
            .query_row(
                r#"
                SELECT review.team_room_id
                FROM local_agent_team_asset_reviews review
                JOIN project_agent_rooms room
                  ON room.owner_user_id = review.owner_user_id
                 AND room.id = review.team_room_id
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id
                 AND member.room_id = room.id AND member.agent_id = ?3
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE review.owner_user_id = ?1 AND review.id = ?2
                  AND review.status = 'pending' AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                  AND room.project_manager_agent_id = ?3
                  AND member.status = 'active' AND profile.status = 'active'
                "#,
                params![owner_user_id, review_id, reviewer_agent_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(team_asset_review_conflict)?;
        if let Some((asset_id, revision)) = resolved_asset {
            let valid_asset: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM local_agent_team_assets WHERE owner_user_id = ?1 \
                     AND team_room_id = ?2 AND id = ?3 AND revision = ?4 AND status = 'active')",
                    params![owner_user_id, team_id, asset_id, revision],
                    |row| row.get(0),
                )
                .map_err(|_| team_store_error())?;
            if !valid_asset {
                return Err(team_asset_review_conflict());
            }
        }
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_team_asset_reviews
                SET status = ?1, resolved_asset_id = ?2, resolved_asset_revision = ?3,
                    reviewed_by_agent_id = ?4, review_reason = ?5,
                    updated_at_unix_ms = ?6
                WHERE owner_user_id = ?7 AND id = ?8 AND status = 'pending'
                "#,
                params![
                    outcome,
                    resolved_asset.map(|value| value.0),
                    resolved_asset.map(|value| value.1),
                    reviewer_agent_id,
                    reason,
                    now_unix_ms,
                    owner_user_id,
                    review_id,
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err(team_asset_review_conflict());
        }
        let review = transaction
            .query_row(
                r#"
                SELECT review.id, review.team_room_id, review.todo_id, todo.title,
                       review.source_agent_id, profile.name, review.category, review.title,
                       review.markdown, review.rationale, review.status,
                       review.resolved_asset_id, review.resolved_asset_revision,
                       review.reviewed_by_agent_id, review.review_reason,
                       review.created_at_unix_ms, review.updated_at_unix_ms
                FROM local_agent_team_asset_reviews review
                JOIN local_agent_todos todo
                  ON todo.owner_user_id = review.owner_user_id AND todo.id = review.todo_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = review.owner_user_id
                 AND profile.id = review.source_agent_id
                WHERE review.owner_user_id = ?1 AND review.id = ?2
                "#,
                params![owner_user_id, review_id],
                map_team_asset_review,
            )
            .map_err(|_| team_store_error())?;
        validate_team_asset_review(&review)?;
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(review)
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
                pending
                    .execution_plan
                    .as_ref()
                    .ok_or_else(team_store_error)?;
            }
            ("todo", None) | (_, Some(_)) => return Err(team_store_error()),
            _ => {}
        }
        if pending.trigger_kind != "todo" && pending.execution_plan.is_some() {
            return Err(team_store_error());
        }
        validate_team_execution_binding(&pending.binding)?;
        validate_text(&pending.content, 1_000_000, true).map_err(|_| team_store_error())?;
        if pending.content.is_empty() {
            let has_attachment = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM project_agent_message_attachments \
                     WHERE owner_user_id = ?1 AND message_id = ?2)",
                    params![owner_user_id, pending.message_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|_| team_store_error())?;
            if !has_attachment {
                return Err(team_store_error());
            }
        }
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
                &[],
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

    // Bind the Host transcript to the exact Agent Contact.  The account-scoped Agent room above
    // remains the authority for the rest of the execution profile, while the Host resource
    // binding lets memory history resolve the same Agent thread without an unbound conversation.
    let conversation_id = agent_direct_conversation_id(&room.id);
    ensure_agent_host_conversation(
        &manager,
        &owner_user_id,
        &conversation_id,
        &room.name,
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id: room.agent_id.clone(),
        },
    )
    .await?;
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
    let conversation_id = agent_direct_conversation_id(&binding.room_id);
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
    let conversation_id =
        agent_team_conversation_id(&binding.agent.room_id, &binding.agent.agent_id);
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
        let conversation_id = agent_direct_conversation_id(&binding.room_id);
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
    before_message_id: Option<String>,
    limit: Option<u32>,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamConversationSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    if let Some(message_id) = before_message_id.as_deref() {
        validate_identifier(message_id).map_err(|_| invalid_team_input())?;
    }
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let before = before_message_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team_conversation(&owner, &team, before.as_deref(), limit)
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

#[tauri::command]
pub(crate) async fn cancel_agent_team_run(
    input: CancelAgentTeamRunInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&input.run_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let binding_store = conversations.inner().clone();
    let binding_owner = input.owner_user_id.clone();
    let binding_team = input.team_id.clone();
    let bindings = tauri::async_runtime::spawn_blocking(move || {
        binding_store.active_project_team_run_bindings(&binding_owner, &binding_team)
    })
    .await
    .map_err(|_| "Agent 团队运行绑定读取任务异常退出。".to_owned())??;
    let binding = bindings
        .into_iter()
        .find(|binding| binding.run_id == input.run_id);
    let Some(binding) = binding else {
        // A poll or another client may have observed the terminal Host state between the user's
        // click and this command. Treat that exact recorded Run as an idempotent success, but do
        // not let an unrelated or cross-team Run ID turn into a successful cancellation.
        let snapshot_store = conversations.inner().clone();
        let snapshot_owner = input.owner_user_id.clone();
        let snapshot_team = input.team_id.clone();
        let snapshot = tauri::async_runtime::spawn_blocking(move || {
            snapshot_store.load_project_team_runs(&snapshot_owner, &snapshot_team)
        })
        .await
        .map_err(|_| "Agent 团队运行记录读取任务异常退出。".to_owned())??;
        if snapshot.runs.iter().any(|run| {
            run.run_id == input.run_id && !matches!(run.run_status.as_str(), "ready" | "running")
        }) {
            authentication
                .require_session_generation(&input.owner_user_id, generation)
                .await?;
            return Ok(());
        }
        return Err("找不到可结束的团队 Run，请刷新后重试。".into());
    };
    let turn = team_todo::cancel_active_team_run(
        manager.inner(),
        &input.owner_user_id,
        &binding.conversation_id,
        &binding.turn_id,
        &binding.run_id,
        "用户已结束这个未完成的 Agent Run。",
    )
    .await?;
    let reconciliation = team_todo::TeamRunReconciliation {
        delivery_id: binding.delivery_id,
        turn_id: turn.turn_id,
        run_id: turn.run_id,
        status: turn.status,
        created_at_unix_ms: turn.created_at_unix_ms,
        updated_at_unix_ms: turn.updated_at_unix_ms,
        project_write_evidence_verified: false,
    };
    let reconcile_store = conversations.inner().clone();
    let reconcile_owner = input.owner_user_id.clone();
    let reconcile_team = input.team_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        reconcile_store.reconcile_project_team_runs(
            &reconcile_owner,
            &reconcile_team,
            &[reconciliation],
        )
    })
    .await
    .map_err(|_| "Agent 团队运行停止对账任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    crate::attachment_tool_worker::schedule_team_todo_drain(
        app,
        input.owner_user_id,
        input.team_id,
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn load_agent_team_dashboard(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentProjectDashboardSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_dashboard_snapshot(&owner, &team)
    })
    .await
    .map_err(|_| "项目总览读取任务异常退出。".to_owned())??;
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
pub(crate) async fn list_agent_team_asset_reviews(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamAssetReview>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let reviews = tauri::async_runtime::spawn_blocking(move || {
        store.list_team_asset_reviews(&owner, Some(&team), None, "pending", 500)
    })
    .await
    .map_err(|_| "团队共享资产审核读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(reviews)
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
    attachment_manager: State<'_, AttachmentManager>,
) -> Result<QueueAgentTeamMessageResult, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    let content = input.content.trim().to_owned();
    validate_text(&content, 1_000_000, true).map_err(|_| invalid_team_message())?;
    if content.is_empty() && input.attachment_ids.is_empty() {
        return Err("消息或附件不能为空。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let mentions = input.mentioned_agent_ids.clone();
    let attachment_scope = AttachmentScope::new(
        &input.owner_user_id,
        &agent_team_attachment_conversation_id(&input.team_id),
    );
    let attachment_ids = input.attachment_ids.clone();
    let attachments =
        attachment_manager.verified_draft_payloads(attachment_scope.clone(), &attachment_ids)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        store.queue_project_team_message_with_attachments(
            &owner,
            &team,
            &content,
            &mentions,
            &attachments,
        )
    })
    .await
    .map_err(|_| "Agent 团队消息入队任务异常退出。".to_owned())??;
    attachment_manager.discard_ids(attachment_scope, attachment_ids);
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
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, HostManager>,
    attachment_manager: State<'_, AttachmentManager>,
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
    let settings_store = general_settings.inner().clone();
    let skill_owner = owner_user_id.clone();
    let profession_key = pending.binding.agent.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        let language = settings_store.context_language(&skill_owner)?;
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key, &language)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    let conversation_id = if pending.trigger_kind == "todo" {
        format!("agent-team-executor-{}", pending.delivery_id)
    } else {
        agent_team_conversation_id(
            &pending.binding.agent.room_id,
            &pending.binding.agent.agent_id,
        )
    };
    let title = format!(
        "{} · {}",
        pending.binding.team_name, pending.binding.agent.name
    );
    let detail = if pending.trigger_kind == "todo" {
        ensure_team_executor_host_conversation(&manager, &owner_user_id, &conversation_id, &title)
            .await?
    } else {
        ensure_agent_host_conversation(
            &manager,
            &owner_user_id,
            &conversation_id,
            &title,
            LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id: pending.binding.agent.agent_id.clone(),
            },
        )
        .await?;
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
    let attachment_store = conversations.inner().clone();
    let attachment_owner = owner_user_id.clone();
    let attachment_message = pending.message_id.clone();
    let payloads = tauri::async_runtime::spawn_blocking(move || {
        attachment_store.load_message_attachment_payloads(&attachment_owner, &attachment_message)
    })
    .await
    .map_err(|_| "Agent 团队附件读取任务异常退出。".to_owned())??;
    let attachment_scope = AttachmentScope::new(&owner_user_id, &conversation_id);
    let (delivery_attachment_ids, delivery_attachments) =
        attachment_manager.authorize_verified_payloads(attachment_scope.clone(), &payloads)?;
    let command = StartConversationTurnCommand {
        owner_user_id: owner_user_id.clone(),
        conversation_id: conversation_id.clone(),
        conversation_if_missing: None,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: host_message_id.clone(),
        run_id: run_id.clone(),
        message: pending.content.clone(),
        message_metadata: team_agent_execution_metadata(&pending, &profession)?,
        attachments: delivery_attachments,
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    if command.validate().is_err() {
        attachment_manager.discard_ids(attachment_scope, delivery_attachment_ids);
        return Err("Agent 团队消息或执行快照超过安全限制。".into());
    }
    let claim_store = conversations.inner().clone();
    let claim_owner = owner_user_id.clone();
    let claim_pending = pending.clone();
    let claim_turn = turn_id.clone();
    let claim_run = run_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
        claim_store.claim_project_team_delivery(
            &claim_owner,
            &claim_pending,
            &claim_turn,
            &claim_run,
            now,
        )
    })
    .await
    .map_err(|_| "Agent Delivery 领取任务异常退出。".to_owned())?
    {
        attachment_manager.discard_ids(attachment_scope, delivery_attachment_ids);
        return Err(error);
    }
    let started = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await;
    let result = match started {
        Ok(HostResult::ConversationTurnStarted { result }) => result,
        Ok(_) => {
            attachment_manager
                .discard_ids(attachment_scope.clone(), delivery_attachment_ids.clone());
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
            attachment_manager
                .discard_ids(attachment_scope.clone(), delivery_attachment_ids.clone());
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
        attachment_manager.discard_ids(attachment_scope.clone(), delivery_attachment_ids.clone());
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
    attachment_manager.commit(attachment_scope, &delivery_attachment_ids)?;
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
    general_settings: State<'_, GeneralSettingsStore>,
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
    let settings_store = general_settings.inner().clone();
    let skill_owner = owner_user_id.clone();
    let profession_key = pending.binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        let language = settings_store.context_language(&skill_owner)?;
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key, &language)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    let conversation_id =
        agent_communication_conversation_id(&pending.binding.room_id, &pending.binding.agent_id);
    let title = format!("{} · {}", pending.room_name, pending.binding.name);
    ensure_agent_host_conversation(
        &manager,
        &owner_user_id,
        &conversation_id,
        &title,
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id: pending.binding.agent_id.clone(),
        },
    )
    .await?;
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
        conversation_if_missing: None,
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
    general_settings: State<'_, GeneralSettingsStore>,
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
    let settings_store = general_settings.inner().clone();
    let skill_owner = input.owner_user_id.clone();
    let profession_key = binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        let language = settings_store.context_language(&skill_owner)?;
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key, &language)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    let conversation_id = agent_direct_conversation_id(&binding.room_id);
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
        conversation_if_missing: None,
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
    resource: LocalConversationResourceBinding,
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
            if conversation.conversation.resource.as_ref() != Some(&resource) {
                if conversation.conversation.resource.is_some() {
                    return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                }
                let migrated = manager
                    .execute(HostCommand::CreateConversation(CreateConversationCommand {
                        conversation_id: conversation_id.to_owned(),
                        owner_user_id: owner_user_id.to_owned(),
                        title: title.to_owned(),
                        resource: Some(resource.clone()),
                    }))
                    .await?;
                let HostResult::Conversation { conversation } = migrated else {
                    return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into());
                };
                validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
                if conversation.conversation.resource.as_ref() != Some(&resource) {
                    return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                }
            }
            Ok(())
        }
        Ok(_) => Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.to_owned(),
                    owner_user_id: owner_user_id.to_owned(),
                    title: title.to_owned(),
                    resource: Some(resource.clone()),
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
                    if conversation.conversation.resource.as_ref() != Some(&resource) {
                        return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                    }
                    Ok(())
                }
                Ok(_) => Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
                Err(create_error) => {
                    let retry = agent_conversation_context(manager, owner_user_id, conversation_id)
                        .await
                        .map_err(|_| create_error)?;
                    validate_host_conversation(&retry, owner_user_id, conversation_id)?;
                    if retry.conversation.resource.as_ref() != Some(&resource) {
                        return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                    }
                    Ok(())
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
) -> Result<LocalConversationDetail, String> {
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
                    resource: None,
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_team_executor_host_conversation(
                        &conversation,
                        owner_user_id,
                        conversation_id,
                    )?;
                    Ok(conversation)
                }
                Ok(_) => Err("Local Agent Host 返回了错误的 executor 会话响应。".into()),
                Err(create_error) => {
                    let retry = manager
                        .execute(HostCommand::GetConversationContext(
                            GetConversationCommand {
                                owner_user_id: owner_user_id.to_owned(),
                                conversation_id: conversation_id.to_owned(),
                            },
                        ))
                        .await
                        .map_err(|_| create_error)?;
                    let HostResult::Conversation { conversation } = retry else {
                        return Err("Local Agent Host 返回了错误的 executor 会话响应。".into());
                    };
                    validate_team_executor_host_conversation(
                        &conversation,
                        owner_user_id,
                        conversation_id,
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
    if pending.trigger_kind != "todo" && binding.is_project_manager {
        metadata["agent_project_management"] = json!({
            "schema_version": 1,
            "instructions": "你已被明确指定为当前团队的项目经理。请先调用 project_dashboard_get 读取系统事实，再用 project_dashboard_update 建立或更新结构化项目总览、里程碑、风险、下一步和需要 Human 处理的事项。普通成员阻塞由项目经理先行调查、拆单、重派和协调；只有权限、预算、凭据、产品方向决策或外部动作确实只能由 Human 完成时，才登记 Human issue。进度和健康判断必须基于真实任务、Run、调研或 Human 消息，不得用总结文字冒充已验收交付。",
        });
    }
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
                "plugins": plan.plugins,
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
    {
        return Err("Local Agent Host 返回了不匹配的 Agent 会话。".into());
    }
    Ok(())
}

fn validate_team_executor_host_conversation(
    conversation: &LocalConversationDetail,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
        || conversation.conversation.resource.is_some()
    {
        return Err("Local Agent Host 返回了不匹配的隔离 executor 会话。".into());
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
    connector_storage::confined_connector_directory(app_data_directory).map_err(|_| store_error())
}

fn reject_link_or_non_file(path: &Path) -> Result<(), String> {
    connector_storage::reject_link_or_non_file(path).map_err(|_| store_error())
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

fn read_project_dashboard(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
) -> Result<Option<AgentProjectDashboard>, String> {
    let stored = connection
        .query_row(
            r#"
            SELECT dashboard_json, revision, updated_by_agent_id, updated_at_unix_ms
            FROM local_agent_project_dashboards
            WHERE owner_user_id = ?1 AND team_room_id = ?2
            LIMIT 1
            "#,
            params![owner_user_id, team_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|_| team_store_error())?;
    let Some((encoded, revision, updated_by_agent_id, updated_at_unix_ms)) = stored else {
        return Ok(None);
    };
    let update = serde_json::from_str::<AgentProjectDashboardUpdate>(&encoded)
        .map_err(|_| team_store_error())?;
    validate_project_dashboard_update(&update).map_err(|_| team_store_error())?;
    validate_agent_identifier(&updated_by_agent_id).map_err(|_| team_store_error())?;
    if revision <= 0 || updated_at_unix_ms < 0 {
        return Err(team_store_error());
    }
    Ok(Some(AgentProjectDashboard {
        team_id: team_id.to_owned(),
        phase: update.phase,
        health: update.health,
        summary: update.summary,
        next_steps: update.next_steps,
        milestones: update.milestones,
        issues: update.issues,
        revision,
        updated_by_agent_id,
        updated_at_unix_ms,
    }))
}

fn validate_project_dashboard_update(update: &AgentProjectDashboardUpdate) -> Result<(), String> {
    validate_text(&update.phase, 240, false).map_err(|_| invalid_project_dashboard_input())?;
    validate_text(&update.summary, 16_000, false).map_err(|_| invalid_project_dashboard_input())?;
    if !matches!(
        update.health.as_str(),
        "on_track" | "at_risk" | "blocked" | "completed"
    ) || update.next_steps.len() > 32
        || update.milestones.len() > 64
        || update.issues.len() > 64
    {
        return Err(invalid_project_dashboard_input());
    }
    let mut milestone_ids = std::collections::HashSet::new();
    for step in &update.next_steps {
        validate_text(step, 2_000, false).map_err(|_| invalid_project_dashboard_input())?;
    }
    for milestone in &update.milestones {
        validate_identifier(&milestone.id).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&milestone.title, 240, false)
            .map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&milestone.detail, 8_000, true)
            .map_err(|_| invalid_project_dashboard_input())?;
        if !milestone_ids.insert(&milestone.id)
            || !matches!(
                milestone.status.as_str(),
                "pending" | "in_progress" | "blocked" | "completed"
            )
            || !(0..=100).contains(&milestone.progress_percent)
            || milestone.acceptance_criteria.len() > 32
            || milestone.linked_todo_ids.len() > 128
            || milestone.target_at_unix_ms.is_some_and(|value| value < 0)
        {
            return Err(invalid_project_dashboard_input());
        }
        let mut linked_ids = std::collections::HashSet::new();
        for criterion in &milestone.acceptance_criteria {
            validate_text(criterion, 2_000, false)
                .map_err(|_| invalid_project_dashboard_input())?;
        }
        for todo_id in &milestone.linked_todo_ids {
            validate_identifier(todo_id).map_err(|_| invalid_project_dashboard_input())?;
            if !linked_ids.insert(todo_id) {
                return Err(invalid_project_dashboard_input());
            }
        }
    }
    let mut issue_ids = std::collections::HashSet::new();
    for issue in &update.issues {
        validate_identifier(&issue.id).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&issue.title, 240, false).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&issue.detail, 8_000, true).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&issue.requested_action, 4_000, false)
            .map_err(|_| invalid_project_dashboard_input())?;
        if !issue_ids.insert(&issue.id)
            || !matches!(issue.severity.as_str(), "info" | "warning" | "critical")
            || !matches!(issue.owner.as_str(), "human" | "agent" | "external")
        {
            return Err(invalid_project_dashboard_input());
        }
        if let Some(todo_id) = issue.related_todo_id.as_deref() {
            validate_identifier(todo_id).map_err(|_| invalid_project_dashboard_input())?;
        }
    }
    Ok(())
}

fn validate_project_dashboard_todo_links(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    update: &AgentProjectDashboardUpdate,
) -> Result<(), String> {
    let todo_ids = update
        .milestones
        .iter()
        .flat_map(|milestone| milestone.linked_todo_ids.iter())
        .chain(
            update
                .issues
                .iter()
                .filter_map(|issue| issue.related_todo_id.as_ref()),
        )
        .collect::<std::collections::HashSet<_>>();
    for todo_id in todo_ids {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, todo_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| team_store_error())?;
        if !exists {
            return Err(invalid_project_dashboard_input());
        }
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

fn map_team_asset_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTeamAssetReview> {
    Ok(AgentTeamAssetReview {
        id: row.get(0)?,
        team_id: row.get(1)?,
        todo_id: row.get(2)?,
        todo_title: row.get(3)?,
        source_agent_id: row.get(4)?,
        source_agent_name: row.get(5)?,
        suggestion: AgentTeamAssetUpdateSuggestion {
            category: row.get(6)?,
            title: row.get(7)?,
            markdown: row.get(8)?,
            rationale: row.get(9)?,
        },
        status: row.get(10)?,
        resolved_asset_id: row.get(11)?,
        resolved_asset_revision: row.get(12)?,
        reviewed_by_agent_id: row.get(13)?,
        review_reason: row.get(14)?,
        created_at_unix_ms: row.get(15)?,
        updated_at_unix_ms: row.get(16)?,
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
              AND delivery.trigger_kind != 'todo'
              AND (
                (run.status IN ('ready', 'running') AND delivery.status = 'running')
                OR (
                  run.status = 'ready' AND delivery.status = 'pending'
                  AND delivery.trigger_kind = 'default_agent'
                  AND room.conversation_kind = 'human_agent_direct'
                )
              )
              AND room.status = 'active'
              AND room.conversation_kind IN (
                  'project_team', 'agent_agent_direct', 'human_agent_direct'
              )
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

fn validate_team_asset_suggestion(
    suggestion: &AgentTeamAssetUpdateSuggestion,
) -> Result<(), String> {
    validate_team_asset_category(&suggestion.category)
        .map_err(|_| invalid_team_asset_review_input())?;
    validate_text(&suggestion.title, 240, false).map_err(|_| invalid_team_asset_review_input())?;
    validate_text(&suggestion.markdown, 128_000, false)
        .map_err(|_| invalid_team_asset_review_input())?;
    validate_text(&suggestion.rationale, 4_000, false)
        .map_err(|_| invalid_team_asset_review_input())
}

fn validate_team_asset_review(review: &AgentTeamAssetReview) -> Result<(), String> {
    for value in [
        review.id.as_str(),
        review.team_id.as_str(),
        review.todo_id.as_str(),
        review.source_agent_id.as_str(),
    ] {
        validate_identifier(value).map_err(|_| team_store_error())?;
    }
    validate_text(&review.todo_title, 500, false).map_err(|_| team_store_error())?;
    validate_text(&review.source_agent_name, 120, false).map_err(|_| team_store_error())?;
    validate_team_asset_suggestion(&review.suggestion).map_err(|_| team_store_error())?;
    if review.created_at_unix_ms < 0 || review.updated_at_unix_ms < review.created_at_unix_ms {
        return Err(team_store_error());
    }
    if let Some(asset_id) = review.resolved_asset_id.as_deref() {
        validate_identifier(asset_id).map_err(|_| team_store_error())?;
    }
    if let Some(agent_id) = review.reviewed_by_agent_id.as_deref() {
        validate_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    validate_text(&review.review_reason, 4_000, true).map_err(|_| team_store_error())?;
    match review.status.as_str() {
        "pending"
            if review.resolved_asset_id.is_none()
                && review.resolved_asset_revision.is_none()
                && review.reviewed_by_agent_id.is_none()
                && review.review_reason.is_empty() => {}
        "applied"
            if review.resolved_asset_id.is_some()
                && review
                    .resolved_asset_revision
                    .is_some_and(|revision| revision > 0)
                && review.reviewed_by_agent_id.is_some() => {}
        "rejected"
            if review.resolved_asset_id.is_none()
                && review.resolved_asset_revision.is_none()
                && review.reviewed_by_agent_id.is_some()
                && !review.review_reason.trim().is_empty() => {}
        _ => return Err(team_store_error()),
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
    format!("agent-communication-v2-{room_id}-{agent_id}")
}

fn agent_direct_conversation_id(room_id: &str) -> String {
    format!("agent-direct-v2-{room_id}")
}

fn agent_team_conversation_id(room_id: &str, agent_id: &str) -> String {
    format!("agent-team-v2-{room_id}-{agent_id}")
}

fn agent_team_attachment_conversation_id(team_id: &str) -> String {
    format!("agent-team-compose-{team_id}")
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

fn invalid_team_asset_review_input() -> String {
    "团队共享资产审核内容无效。".into()
}

fn invalid_project_dashboard_input() -> String {
    "项目总览内容无效。".into()
}

fn project_dashboard_conflict() -> String {
    "项目总览已被更新，请重新读取后合并修改。".into()
}

fn team_asset_conflict() -> String {
    "团队共享资产已被更新，请刷新后重试。".into()
}

fn team_asset_review_conflict() -> String {
    "团队共享资产审核已处理或身份不匹配，请重新读取后重试。".into()
}

#[cfg(test)]
mod tests;
