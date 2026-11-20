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

mod activity_commands;
mod asset_read_store;
mod asset_review_store;
mod asset_support;
mod asset_write_store;
mod communication_reconcile_store;
mod communication_store;
mod dashboard_store;
mod direct_activity_store;
pub(crate) mod documents;
pub(crate) mod heartbeat;
mod host_runtime;
pub(crate) mod proposals;
mod runtime_support;
mod team_delivery_store;
mod team_management_store;
mod team_message_store;
mod team_query_commands;
mod team_run_store;
pub(crate) mod team_todo;
mod validation;

pub(crate) use activity_commands::{
    load_agent_conversation_activity, load_agent_profile_activity, load_agent_team_member_activity,
    prepare_agent_conversation,
};
use asset_support::*;
use host_runtime::*;
use runtime_support::*;
pub(crate) use team_query_commands::{
    cancel_agent_team_run, create_agent_team, list_agent_teams, load_agent_team,
    load_agent_team_conversation, load_agent_team_dashboard, load_agent_team_runs,
};
use validation::*;

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

    #[cfg(test)]
    fn database(&self) -> &Path {
        &self.database
    }
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

#[cfg(test)]
mod tests;
