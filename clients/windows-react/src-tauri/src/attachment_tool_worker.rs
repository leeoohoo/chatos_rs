use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chatos_local_agent_protocol::{
    ClaimNextToolCommand, CommitToolCommand, GetConversationAttachmentCommand, GetRunCommand,
    HostCommand, HostResult, LocalAgentRunRecord, LocalAgentToolClaim, LocalAgentToolOutcome,
};
use image::metadata::Orientation;
use image::{
    codecs::jpeg::JpegEncoder, DynamicImage, GenericImageView, ImageDecoder, ImageFormat,
    ImageReader,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
};
use tauri::{AppHandle, Manager};
use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{
    agent_conversation::{
        dispatch_next_agent_communication_delivery, dispatch_next_agent_team_delivery,
        load_agent_team_runs, reconcile_agent_communication_runs,
        team_todo::{
            todo_planning_reference, AgentTodoExecutionContract, AgentTodoPluginSelection,
            TodoPlanningCreateInput, TodoPlanningUpdateInput,
        },
        AgentAssetToolBinding, AgentAssetToolRecord, AgentCommunicationToolBinding,
        AgentConversationStore, AgentProjectDashboardUpdate, AgentProjectIssue,
        AgentProjectMilestone, AgentTeamAssetReview, AgentTeamAssetUpdateSuggestion,
    },
    agent_plugin_runtime::{AgentPluginRunContext, CAPABILITY_BROKER_TOOL_NAMES},
    agent_skill_management::AgentSkillStore,
    authentication::AuthenticationManager,
    conversation_attachments::AttachmentManager,
    host_lifecycle::HostManager,
    platform_execution_context::resolve_platform_execution_context,
    plugin_application::{PluginApplicationManager, TrustedAgentPluginOption},
    plugin_installer_tool_worker::{execute_plugin_installer_tool, PLUGIN_INSTALLER_TOOL_NAMES},
    product_skill_runtime::ProductSkillRuntime,
    terminal_tool_worker::TerminalToolWorker,
};

mod asset_context;
mod attachment_read;
mod project_dashboard;
mod team_asset;
mod team_chat;
mod todo_execution;
mod todo_planning;

use asset_context::{
    asset_reference, asset_summary, invalid_asset_arguments, team_asset_review_output,
    team_asset_review_reference, validate_asset_run_context, validate_communication_run_context,
};
#[cfg(test)]
use attachment_read::{content_output, model_image_output, MAXIMUM_CONTENT_READ_BYTES};
use attachment_read::{execute_attachment_read, safe_error};
use project_dashboard::execute_project_dashboard_tool;
#[cfg(test)]
use team_asset::require_asset_manager_lane;
use team_asset::{execute_team_asset_tool, now_unix_ms};
use team_chat::{execute_team_chat_read_tool, execute_team_chat_write_tool};
pub(crate) use team_chat::{schedule_agent_communication_drain, schedule_team_todo_drain};
use todo_execution::execute_todo_execution_tool;
use todo_planning::{execute_todo_planning_read_tool, execute_todo_planning_write_tool};

pub const ATTACHMENT_READ_TOOL_NAME: &str = "local_attachment_read";
const TEAM_ASSET_TOOL_NAMES: [&str; 7] = [
    "team_asset_list",
    "team_asset_get",
    "team_asset_create",
    "team_asset_update",
    "team_asset_archive",
    "team_asset_review_list",
    "team_asset_review_resolve",
];
const PROJECT_DASHBOARD_TOOL_NAMES: [&str; 2] =
    ["project_dashboard_get", "project_dashboard_update"];
const TODO_EXECUTION_TOOL_NAMES: [&str; 4] = [
    "todo_get_context",
    "todo_progress_append",
    "todo_complete",
    "todo_block",
];
const TODO_PLANNING_READ_TOOL_NAMES: [&str; 4] = [
    "todo_execution_options",
    "todo_dependency_options",
    "todo_list",
    "todo_read_progress",
];
const TEAM_CHAT_READ_TOOL_NAMES: [&str; 4] = [
    "agent_workspace_snapshot",
    "chat_list_members",
    "chat_read_all_unread",
    "chat_read_attachment",
];
const TEAM_CHAT_WRITE_TOOL_NAMES: [&str; 7] = [
    "chat_document_create",
    "agent_propose_member",
    "agent_propose_existing_member",
    "agent_propose_member_removal",
    "chat_send_message",
    "chat_heartbeat_complete",
    "agent_cycle_complete",
];
const TODO_PLANNING_WRITE_TOOL_NAMES: [&str; 3] = ["todo_add", "todo_update", "todo_reorder"];
const WORKER_ID: &str = "windows-attachment-tool-worker";
const LEASE_DURATION_MILLISECONDS: u64 = 30_000;

pub struct AttachmentToolWorker {
    generation: Arc<AtomicU64>,
    wake: Arc<Notify>,
    task: Mutex<Option<JoinHandle<()>>>,
    product_skills: ProductSkillRuntime,
}

impl Default for AttachmentToolWorker {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            wake: Arc::new(Notify::new()),
            task: Mutex::new(None),
            product_skills: ProductSkillRuntime::default(),
        }
    }
}

impl AttachmentToolWorker {
    pub async fn configure(&self, app: AppHandle, owner_user_id: String) {
        self.reset().await;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let expected_generation = self.generation.clone();
        let wake = self.wake.clone();
        *self.task.lock().await = Some(tokio::spawn(async move {
            poll(app, owner_user_id, expected_generation, generation, wake).await;
        }));
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }

    pub async fn reset(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.wake.notify_waiters();
        self.product_skills.reset().await;
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
        }
    }
}

async fn poll(
    app: AppHandle,
    owner_user_id: String,
    generation: Arc<AtomicU64>,
    expected_generation: u64,
    wake: Arc<Notify>,
) {
    recover_team_todo_drains(&app, &owner_user_id).await;
    let mut idle_delay = Duration::from_millis(250);
    while generation.load(Ordering::SeqCst) == expected_generation {
        enqueue_due_heartbeats(&app, &owner_user_id).await;
        let claimed = claim_and_execute(&app, &owner_user_id)
            .await
            .unwrap_or(false);
        if claimed {
            idle_delay = Duration::from_millis(250);
            continue;
        }
        tokio::select! {
            _ = wake.notified() => { idle_delay = Duration::from_millis(250); }
            _ = sleep(idle_delay) => {
                idle_delay = (idle_delay * 2).min(Duration::from_secs(2));
            }
        }
    }
}

async fn enqueue_due_heartbeats(app: &AppHandle, owner_user_id: &str) {
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let rooms = tauri::async_runtime::spawn_blocking(move || {
        store.enqueue_due_agent_heartbeats(&owner, now_unix_ms(), 16)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    for room_id in rooms {
        schedule_agent_communication_drain(app.clone(), owner_user_id.to_owned(), room_id);
    }
}

async fn recover_team_todo_drains(app: &AppHandle, owner_user_id: &str) {
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let teams =
        tauri::async_runtime::spawn_blocking(move || store.teams_requiring_todo_scheduler(&owner))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
    for team_id in teams {
        schedule_team_todo_drain(app.clone(), owner_user_id.to_owned(), team_id);
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let rooms = tauri::async_runtime::spawn_blocking(move || {
        store.communication_rooms_requiring_scheduler(&owner)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    for room_id in rooms {
        schedule_agent_communication_drain(app.clone(), owner_user_id.to_owned(), room_id);
    }
}

async fn claim_and_execute(app: &AppHandle, owner_user_id: &str) -> Result<bool, String> {
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::ClaimNextTool(ClaimNextToolCommand {
            owner_user_id: owner_user_id.to_owned(),
            worker_id: WORKER_ID.into(),
            lease_duration_ms: LEASE_DURATION_MILLISECONDS,
            include_tool_names: Some(
                std::iter::once(ATTACHMENT_READ_TOOL_NAME)
                    .chain(TEAM_ASSET_TOOL_NAMES)
                    .chain(PROJECT_DASHBOARD_TOOL_NAMES)
                    .chain(TODO_EXECUTION_TOOL_NAMES)
                    .chain(CAPABILITY_BROKER_TOOL_NAMES)
                    .chain(TODO_PLANNING_READ_TOOL_NAMES)
                    .chain(TEAM_CHAT_READ_TOOL_NAMES)
                    .chain(TEAM_CHAT_WRITE_TOOL_NAMES)
                    .chain(TODO_PLANNING_WRITE_TOOL_NAMES)
                    .chain(PLUGIN_INSTALLER_TOOL_NAMES)
                    .map(str::to_owned)
                    .collect(),
            ),
            exclude_tool_names: vec![],
            execution_route: None,
        }))
        .await?;
    let HostResult::ToolClaim { claim } = result else {
        return Err("Local Agent Host returned an unexpected tool claim result".into());
    };
    let Some(claim) = claim else { return Ok(false) };
    let outcome = match claim.preflight_outcome.clone() {
        Some(outcome) => outcome,
        None => {
            let executed = if claim.invocation.tool_name == ATTACHMENT_READ_TOOL_NAME {
                execute_attachment_read(app, owner_user_id, &claim).await
            } else if CAPABILITY_BROKER_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_capability_broker_tool(app, owner_user_id, &claim).await
            } else if PROJECT_DASHBOARD_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_project_dashboard_tool(app, owner_user_id, &claim).await
            } else if TODO_EXECUTION_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_todo_execution_tool(app, owner_user_id, &claim).await
            } else if TODO_PLANNING_READ_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_todo_planning_read_tool(app, owner_user_id, &claim).await
            } else if TEAM_CHAT_READ_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_team_chat_read_tool(app, owner_user_id, &claim).await
            } else if TEAM_CHAT_WRITE_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_team_chat_write_tool(app, owner_user_id, &claim).await
            } else if TODO_PLANNING_WRITE_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str())
            {
                execute_todo_planning_write_tool(app, owner_user_id, &claim).await
            } else if PLUGIN_INSTALLER_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_plugin_installer_tool(app, owner_user_id, &claim).await
            } else {
                execute_team_asset_tool(app, owner_user_id, &claim).await
            };
            executed.unwrap_or_else(|error| LocalAgentToolOutcome::Failed {
                error: safe_error(error),
                detail: json!({
                    "tool_name": claim.invocation.tool_name,
                    "phase": "windows_local_read_execution"
                }),
            })
        }
    };
    let schedule_team_todos = matches!(
        claim.invocation.tool_name.as_str(),
        "todo_add" | "todo_update" | "todo_complete" | "todo_block"
    ) && matches!(&outcome, LocalAgentToolOutcome::Succeeded { .. });
    let schedule_team_messages = matches!(
        claim.invocation.tool_name.as_str(),
        "chat_send_message" | "chat_heartbeat_complete" | "agent_cycle_complete"
    ) && matches!(&outcome, LocalAgentToolOutcome::Succeeded { .. });
    let team_to_schedule = if schedule_team_todos {
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = claim.invocation.run_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            store.load_agent_asset_tool_records(&owner, &run_id)
        })
        .await
        .ok()
        .and_then(Result::ok)
        .map(|(binding, _)| binding.team_id)
    } else {
        None
    };
    let result = host
        .execute(HostCommand::CommitTool(CommitToolCommand {
            owner_user_id: owner_user_id.to_owned(),
            invocation_id: claim.invocation.invocation_id.clone(),
            claim_token: claim.claim_token,
            expected_version: claim.invocation.version,
            outcome,
        }))
        .await?;
    if !matches!(result, HostResult::ToolCommit { .. }) {
        return Err("Local Agent Host returned an unexpected tool commit result".into());
    }
    if let Some(team_id) = team_to_schedule {
        schedule_team_todo_drain(app.clone(), owner_user_id.to_owned(), team_id);
    }
    if schedule_team_messages {
        recover_team_todo_drains(app, owner_user_id).await;
    }
    Ok(true)
}

async fn execute_capability_broker_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Plugin capability arguments are invalid.".to_owned())?;
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Plugin capability Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id
        || run.run_id != claim.invocation.run_id
        || run.profile_key != "task_execution"
    {
        return Err("The Plugin capability Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let (binding, _) = tauri::async_runtime::spawn_blocking(move || {
        store.load_agent_asset_tool_records(&owner, &run_id)
    })
    .await
    .map_err(|_| "The Plugin capability binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &binding)?;
    if binding.todo_id.is_none() {
        return Err("Plugin capabilities are available only in a Todo executor Run.".into());
    }
    if matches!(
        claim.invocation.tool_name.as_str(),
        "agent_skill_activate" | "agent_skill_read_resource"
    ) {
        if let Some(outcome) = app
            .state::<AttachmentToolWorker>()
            .product_skills
            .execute_if_product(
                host.inner(),
                owner_user_id,
                &run.run_id,
                &claim.invocation.tool_name,
                arguments,
            )
            .await?
        {
            return Ok(outcome);
        }
    }
    let platform =
        resolve_platform_execution_context(app, host.inner(), owner_user_id, &run.run_id).await?;
    let plugin_manager = app.state::<PluginApplicationManager>();
    plugin_manager
        .agent_runtime
        .execute(
            plugin_manager.inner(),
            host.inner(),
            AgentPluginRunContext {
                owner_user_id: owner_user_id.to_owned(),
                run_id: run.run_id,
                execution_root: platform.execution_root,
                workspace_scope_id: platform.workspace_scope_id,
                input: run.input,
            },
            &claim.invocation.invocation_id,
            &claim.invocation.tool_name,
            arguments,
        )
        .await
}

fn invalid_todo_planning_arguments() -> String {
    "The Todo planning arguments are invalid.".into()
}

fn optional_planning_text(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
    allow_empty: bool,
) -> Result<Option<String>, String> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_str()
                .filter(|value| {
                    (allow_empty || !value.is_empty())
                        && value.trim() == *value
                        && value.chars().count() <= maximum
                        && !value.contains('\0')
                })
                .map(str::to_owned)
                .ok_or_else(invalid_todo_planning_arguments)
        })
        .transpose()
}

fn optional_planning_integer(
    arguments: &Map<String, Value>,
    field: &str,
    minimum: i64,
    maximum: i64,
) -> Result<Option<i64>, String> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_i64()
                .filter(|value| (minimum..=maximum).contains(value))
                .ok_or_else(invalid_todo_planning_arguments)
        })
        .transpose()
}

fn planning_string_array(
    arguments: &Map<String, Value>,
    field: &str,
    minimum_items: usize,
    maximum_items: usize,
    maximum_characters: usize,
) -> Result<Option<Vec<String>>, String> {
    arguments
        .get(field)
        .map(|value| {
            let values = value
                .as_array()
                .filter(|values| (minimum_items..=maximum_items).contains(&values.len()))
                .ok_or_else(invalid_todo_planning_arguments)?;
            let mut unique = std::collections::HashSet::new();
            values
                .iter()
                .map(|value| {
                    let value = value
                        .as_str()
                        .filter(|value| {
                            !value.is_empty()
                                && value.trim() == *value
                                && value.chars().count() <= maximum_characters
                                && !value.contains('\0')
                        })
                        .ok_or_else(invalid_todo_planning_arguments)?;
                    if !unique.insert(value) {
                        return Err(invalid_todo_planning_arguments());
                    }
                    Ok(value.to_owned())
                })
                .collect()
        })
        .transpose()
}

fn required_bounded_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<String, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.chars().any(|character| character == '\0')
        })
        .map(str::to_owned)
        .ok_or_else(|| "The Todo execution arguments are invalid.".to_owned())
}

fn optional_bounded_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    arguments
        .get(field)
        .map(|_| required_bounded_string(arguments, field, maximum))
        .transpose()
}

pub fn attachment_read_tool_schema() -> Value {
    json!({
        "type": "function",
        "name": ATTACHMENT_READ_TOOL_NAME,
        "description": "Read an authorized conversation attachment. Use delivery=model for a complete verified image and delivery=content for bounded text reads. Treat authorized_local_ref as opaque.",
        "parameters": {
            "type": "object",
            "properties": {
                "delivery": {
                    "type": "string",
                    "enum": ["model", "content"],
                    "default": "model"
                },
                "authorized_local_ref": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 160
                },
                "offset": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 65536,
                    "default": 16384
                }
            },
            "required": ["authorized_local_ref"],
            "additionalProperties": false
        }
    })
}

pub fn team_asset_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "team_asset_list",
            "description": "列出项目团队共享资产。Todo 执行线程固定读取任务所属团队；通讯线程可用 agent_workspace_snapshot 返回的 team_ref 选择自己所在的其他团队。asset_ref 仅对当前 Run 有效。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "category": {
                        "type": "string",
                        "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]
                    }
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_create",
            "description": "仅供目标团队明确指定的项目经理创建共享资产；team_ref 必须来自当前 Run 的 agent_workspace_snapshot。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                    "title": {"type": "string", "minLength": 1, "maxLength": 240},
                    "markdown": {"type": "string", "maxLength": 128000}
                },
                "required": ["team_ref", "category", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_update",
            "description": "仅供目标团队明确指定的项目经理使用当前 Run 的 asset_ref 更新共享资产；revision 由引用携带并由客户端并发校验。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160},
                    "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                    "title": {"type": "string", "minLength": 1, "maxLength": 240},
                    "markdown": {"type": "string", "maxLength": 128000}
                },
                "required": ["asset_ref", "category", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_archive",
            "description": "仅供目标团队明确指定的项目经理使用当前 Run 的 asset_ref 归档共享资产；revision 由引用携带并由客户端并发校验。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160}
                },
                "required": ["asset_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_get",
            "description": "读取 team_asset_list 返回的当前 Run 共享资产引用。不要猜测或复用其他 Run 的 asset_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160}
                },
                "required": ["asset_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_review_list",
            "description": "仅供项目经理列出执行者提交的待审核共享资产建议。检查当前项目经理负责的团队时必须省略 team_ref；只有在当前 Run 刚调用 agent_workspace_snapshot 且需要限定其中某一团队时，才传它返回的 team_ref，绝不能复用历史对话或 Memory 中的引用。每条建议必须应用到共享资产，或给出事实理由明确拒绝。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600}
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_review_resolve",
            "description": "仅供项目经理处理一条共享资产建议。applied 时必须先用 team_asset_create/update 写入，再传成功返回的当前 asset_ref；rejected 时必须传具体 reason。处理后不可撤回。",
            "parameters": {
                "type": "object",
                "properties": {
                    "review_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "outcome": {"type": "string", "enum": ["applied", "rejected"]},
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "reason": {"type": "string", "minLength": 1, "maxLength": 4000}
                },
                "required": ["review_ref", "outcome"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn project_dashboard_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "project_dashboard_get",
            "description": "读取项目总览、里程碑、项目经理登记的问题，以及团队 Todo 的实时状态。项目经理更新看板前必须先调用本工具。",
            "parameters": {
                "type": "object",
                "properties": {"team_ref": {"type": "string", "minLength": 1, "maxLength": 600}},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "project_dashboard_update",
            "description": "仅供目标团队明确指定的项目经理更新结构化项目总览。必须先读取当前总览；首次创建省略 expected_revision，后续更新必须提交当前 revision。Todo 只能使用同一 Run 返回的 todo_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "expected_revision": {"type": "integer", "minimum": 1},
                    "phase": {"type": "string", "minLength": 1, "maxLength": 240},
                    "health": {"type": "string", "enum": ["on_track", "at_risk", "blocked", "completed"]},
                    "summary": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "next_steps": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 2000}, "maxItems": 32},
                    "milestones": {
                        "type": "array", "maxItems": 64,
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "minLength": 1, "maxLength": 512},
                                "title": {"type": "string", "minLength": 1, "maxLength": 240},
                                "detail": {"type": "string", "maxLength": 8000},
                                "status": {"type": "string", "enum": ["pending", "in_progress", "blocked", "completed"]},
                                "progress_percent": {"type": "integer", "minimum": 0, "maximum": 100},
                                "acceptance_criteria": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 2000}, "maxItems": 32},
                                "todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "maxItems": 128, "uniqueItems": true},
                                "target_at_unix_ms": {"type": "integer", "minimum": 0}
                            },
                            "required": ["id", "title", "status", "progress_percent"],
                            "additionalProperties": false
                        }
                    },
                    "issues": {
                        "type": "array", "maxItems": 64,
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "minLength": 1, "maxLength": 512},
                                "title": {"type": "string", "minLength": 1, "maxLength": 240},
                                "detail": {"type": "string", "maxLength": 8000},
                                "requested_action": {"type": "string", "minLength": 1, "maxLength": 4000},
                                "severity": {"type": "string", "enum": ["info", "warning", "critical"]},
                                "owner": {"type": "string", "enum": ["human", "agent", "external"]},
                                "todo_ref": {"type": "string", "minLength": 1, "maxLength": 600}
                            },
                            "required": ["id", "title", "requested_action", "severity", "owner"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["team_ref", "phase", "health", "summary"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn todo_planning_read_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "todo_execution_options",
            "description": "仅供项目经理读取当前管理团队、可分配成员、基础能力和本机 Plugin 临时选项。真实团队、项目、Agent 和 Plugin ID 不会返回。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_dependency_options",
            "description": "仅供项目经理读取当前团队可作为前置任务的 Todo 临时引用。只能建立同团队依赖。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600}
                },
                "required": ["team_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_list",
            "description": "读取当前 Agent 所属项目团队的共享任务板，并标明负责人、依赖、执行能力和是否分配给自己。默认不返回已完成或已取消任务。",
            "parameters": {
                "type": "object",
                "properties": {
                    "include_terminal": {"type": "boolean", "default": false}
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_read_progress",
            "description": "通讯线程读取一个 Todo 执行线程持续写入的阶段、动作、结果或阻塞记录。使用 todo_list 返回的临时 todo_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "todo_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 100}
                },
                "required": ["todo_ref"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn team_chat_read_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "agent_workspace_snapshot",
            "description": "读取当前账户的活动 Agent 与项目团队，并返回只对当前 Run 有效的 agent_ref、team_ref 和 project_ref。主动私聊另一个 Agent 时把 agent_ref 作为 target_agent_ref，主动向另一个项目团队发送消息时把 team_ref 交给 chat_send_message；真实 ID 不会暴露。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_list_members",
            "description": "列出当前项目团队或 Agent 私聊的活动成员，并返回仅对当前 Run 有效的 agent_ref。真实 Agent ID 不会暴露。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_read_all_unread",
            "description": "读取当前 Agent 在全部项目团队和 Agent 私聊中的未读消息，并在同一事务推进已返回会话的游标。回复时只把本工具返回的 message_ref 传给 chat_send_message。",
            "parameters": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 200}
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_read_attachment",
            "description": "读取 chat_read_all_unread 返回的附件。message_ref 与 attachment_ref 必须来自同一条当前 Run 已签发的消息；文本按字符分页返回。当前触发消息中的二进制附件已作为多模态输入提供，历史二进制附件只返回元数据和重新附带提示。",
            "parameters": {
                "type": "object",
                "properties": {
                    "message_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "attachment_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "offset": {"type": "integer", "minimum": 0, "maximum": 2097152, "default": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 12000, "default": 12000}
                },
                "required": ["message_ref", "attachment_ref"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn team_chat_write_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "chat_document_create",
            "description": "创建当前 Run 内的 UTF-8 Markdown 文档草稿。客户端清洗文件名、计算大小和 SHA-256，只返回临时 document_ref；创建后必须在同一 Run 的下一条发送消息中通过 document_refs 附加。",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1, "maxLength": 240},
                    "title": {"type": "string", "minLength": 1, "maxLength": 512},
                    "markdown": {"type": "string", "minLength": 1, "maxLength": 2097152}
                },
                "required": ["name", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_propose_member",
            "description": "使用已授予的人员管理权限，向 Human 提交新 Agent 草案。模型配置由客户端继承当前 Agent；该工具只创建待审批提案，绝不会直接创建 Agent。",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1, "maxLength": 120},
                    "role": {"type": "string", "minLength": 1, "maxLength": 160},
                    "responsibility": {"type": "string", "maxLength": 8000},
                    "role_prompt": {"type": "string", "minLength": 1, "maxLength": 32000},
                    "thinking_level": {"type": "string", "enum": ["auto", "none", "minimal", "low", "medium", "high", "xhigh", "max"]},
                    "profession_key": {"type": "string", "enum": [
                        "project_manager", "product_manager", "technical_manager",
                        "solution_architect", "security_engineer", "qa_engineer",
                        "software_engineer", "fullstack_engineer", "frontend_engineer",
                        "backend_engineer", "mobile_engineer", "desktop_engineer",
                        "game_engineer", "embedded_iot_engineer", "database_engineer",
                        "devops_engineer", "data_engineer", "data_analyst",
                        "machine_learning_engineer", "research_specialist", "product_designer",
                        "ui_designer", "ux_designer", "game_designer", "technical_writer",
                        "business_analyst", "implementation_consultant", "erp_consultant",
                        "wms_consultant", "domain_expert", "operations_specialist",
                        "growth_marketing_specialist", "general_member"
                    ]},
                    "rationale": {"type": "string", "maxLength": 4000}
                },
                "required": ["name", "role", "role_prompt", "profession_key"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_propose_existing_member",
            "description": "使用人员管理权限，提交把账户中已有 Agent 加入指定项目团队的待审批提案。team_ref 与 target_agent_ref 必须来自当前 Run 的 agent_workspace_snapshot。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "target_agent_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "role": {"type": "string", "minLength": 1, "maxLength": 160},
                    "responsibility": {"type": "string", "maxLength": 8000}
                },
                "required": ["team_ref", "target_agent_ref", "role"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_propose_member_removal",
            "description": "使用人员管理权限，提交把一个 Agent 移出当前项目团队的待审批提案。不会删除可复用 Agent profile，也不会绕过 Human 确认。",
            "parameters": {
                "type": "object",
                "properties": {
                    "target_agent_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "reason": {"type": "string", "minLength": 1, "maxLength": 4000},
                    "handoff_plan": {"type": "string", "maxLength": 8000}
                },
                "required": ["target_agent_ref", "reason"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_send_message",
            "description": "向当前会话发送消息，使用 chat_read_all_unread 返回的 message_ref 回复指定消息，使用 agent_workspace_snapshot 返回的 target_agent_ref 主动私聊并唤醒另一个 Agent，或用 team_ref 主动发送到另一个项目团队。reply_to_message_ref、target_agent_ref 和 team_ref 最多提供一个；需要 @ 其他成员时，只能使用当前 Run 返回且属于目标会话的 agent_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "content": {"type": "string", "minLength": 1, "maxLength": 2000},
                    "reply_to_message_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "target_agent_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "document_refs": {
                        "type": "array",
                        "items": {"type": "string", "minLength": 1, "maxLength": 600},
                        "maxItems": 5,
                        "uniqueItems": true
                    },
                    "mention_agent_refs": {
                        "type": "array",
                        "items": {"type": "string", "minLength": 1, "maxLength": 600},
                        "maxItems": 64,
                        "uniqueItems": true
                    }
                },
                "required": ["content"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_heartbeat_complete",
            "description": "仅用于主动巡检：检查全部会话未读和任务后没有需要汇报或执行的事项时，安静完成本次巡检。",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_cycle_complete",
            "description": "结束本次 Agent 通讯周期。调用前必须完成必要回复和任务调整；不会向聊天记录写入内部状态消息。",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
    ]
}

pub fn todo_planning_write_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "todo_add",
            "description": "仅供项目经理在当前共享团队任务板创建 Todo。必须先调用 todo_execution_options，并明确目标、范围、交付物、验收条件、负责人、前置任务和可信执行能力。来源消息由客户端绑定为当前 Run 的触发消息，真实 ID 不会暴露。",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 500},
                    "detail": {"type": "string", "maxLength": 16000},
                    "objective": {"type": "string", "minLength": 1, "maxLength": 8000},
                    "scope": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "expected_outputs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "acceptance_criteria": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "constraints": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "maxItems": 64, "uniqueItems": true},
                    "priority": {"type": "integer", "minimum": 0, "maximum": 100, "default": 50},
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "assignee_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "depends_on_todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "maxItems": 64, "uniqueItems": true},
                    "requires_execution": {"type": "boolean", "default": true},
                    "builtin_capabilities": {"type": "array", "items": {"type": "string", "enum": ["project_read", "project_write", "terminal", "requirement_survey_read", "requirement_survey_write"]}, "maxItems": 5, "uniqueItems": true},
                    "plugin_hints": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "plugin_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                                "reason": {"type": "string", "maxLength": 1000}
                            },
                            "required": ["plugin_ref"],
                            "additionalProperties": false
                        },
                        "maxItems": 32
                    }
                },
                "required": ["title", "objective", "scope", "expected_outputs", "acceptance_criteria", "team_ref", "assignee_ref", "requires_execution", "builtin_capabilities"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_update",
            "description": "仅供项目经理更新待处理或普通阻塞 Todo 的内容、优先级、前置任务，或将阻塞任务重开、取消过时任务。completed 和 blocked 执行终态由 executor 工具写入；正在执行的 Todo 必须通过持久化 Run 控制停止后才能编辑。",
            "parameters": {
                "type": "object",
                "properties": {
                    "todo_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "title": {"type": "string", "minLength": 1, "maxLength": 500},
                    "detail": {"type": "string", "maxLength": 16000},
                    "objective": {"type": "string", "minLength": 1, "maxLength": 8000},
                    "scope": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "expected_outputs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "acceptance_criteria": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "constraints": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "maxItems": 64, "uniqueItems": true},
                    "priority": {"type": "integer", "minimum": 0, "maximum": 100},
                    "status": {"type": "string", "enum": ["pending", "cancelled"]},
                    "depends_on_todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "maxItems": 64, "uniqueItems": true}
                },
                "required": ["todo_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_reorder",
            "description": "仅供项目经理显式调整当前团队任务板中未完成 Todo 的优先顺序。数组中靠前的任务优先；未完成前置任务仍不会被调度。",
            "parameters": {
                "type": "object",
                "properties": {
                    "todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "minItems": 1, "maxItems": 500, "uniqueItems": true}
                },
                "required": ["todo_refs"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn todo_execution_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "todo_get_context",
            "description": "仅用于 Todo 执行线程：读取当前 delivery 绑定的任务、可信能力计划、前置任务、团队资产摘要和最近进度，不接受任何 ID 参数。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_progress_append",
            "description": "仅用于 Todo 执行线程：记录当前负责人的阶段、动作、观察结果和下一步。",
            "parameters": {
                "type": "object",
                "properties": {
                    "stage": {"type": "string", "maxLength": 240},
                    "detail": {"type": "string", "minLength": 1, "maxLength": 16000}
                },
                "required": ["detail"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_complete",
            "description": "仅用于 Todo 执行线程：保存当前负责人可核验的完成总结并结束任务。project_write 任务必须先完成事务提交和提交后读取。",
            "parameters": {
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "asset_update_suggestions": {
                        "type": "array",
                        "maxItems": 8,
                        "items": {
                            "type": "object",
                            "properties": {
                                "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                                "title": {"type": "string", "minLength": 1, "maxLength": 240},
                                "markdown": {"type": "string", "minLength": 1, "maxLength": 128000},
                                "rationale": {"type": "string", "minLength": 1, "maxLength": 4000}
                            },
                            "required": ["category", "title", "markdown", "rationale"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["summary"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_block",
            "description": "仅用于 Todo 执行线程：因缺少必要信息、权限、访问或安全恢复路径而无法继续时，保存精确原因并阻塞任务。",
            "parameters": {
                "type": "object",
                "properties": {
                    "reason": {"type": "string", "minLength": 1, "maxLength": 8000}
                },
                "required": ["reason"],
                "additionalProperties": false
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_schema_matches_the_macos_main_chat_delivery_boundary() {
        let schema = attachment_read_tool_schema();
        assert_eq!(schema["name"], ATTACHMENT_READ_TOOL_NAME);
        assert_eq!(
            schema["parameters"]["properties"]["delivery"]["enum"],
            json!(["model", "content"])
        );
    }

    #[test]
    fn team_asset_schema_never_exposes_raw_asset_ids() {
        let schemas = team_asset_tool_schemas();
        assert_eq!(schemas.len(), 7);
        assert!(schemas
            .iter()
            .all(|schema| !schema.to_string().contains("asset_id")));
        assert!(schemas
            .iter()
            .filter(|schema| {
                matches!(
                    schema["name"].as_str(),
                    Some(
                        "team_asset_get"
                            | "team_asset_update"
                            | "team_asset_archive"
                            | "team_asset_review_resolve"
                    )
                )
            })
            .all(|schema| schema.to_string().contains("asset_ref")));
        assert_eq!(schemas[5]["name"], "team_asset_review_list");
        assert_eq!(schemas[6]["name"], "team_asset_review_resolve");
        assert!(schemas[0]["parameters"]["properties"]
            .get("team_ref")
            .is_some());
        assert!(schemas[1]["parameters"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "team_ref"));
        assert!(schemas[2]["parameters"]["properties"]
            .get("expected_revision")
            .is_none());
        assert!(schemas[3]["parameters"]["properties"]
            .get("expected_revision")
            .is_none());
        assert!(schemas[5]["parameters"]["properties"]
            .get("team_ref")
            .is_some());
        assert!(schemas[5]["parameters"]["required"]
            .as_array()
            .is_some_and(Vec::is_empty));
        assert!(schemas[5]["description"]
            .as_str()
            .is_some_and(|description| description.contains("必须省略 team_ref")
                && description.contains("当前 Run")
                && description.contains("绝不能复用历史对话或 Memory")));
    }

    #[test]
    fn project_dashboard_catalog_uses_revisioned_run_scoped_references() {
        let schemas = project_dashboard_tool_schemas();
        assert_eq!(schemas.len(), 2);
        assert_eq!(schemas[0]["name"], "project_dashboard_get");
        assert_eq!(schemas[1]["name"], "project_dashboard_update");
        let encoded = schemas[1].to_string();
        assert!(encoded.contains("team_ref"));
        assert!(encoded.contains("expected_revision"));
        assert!(encoded.contains("todo_refs"));
        assert!(!encoded.contains("team_id"));
        assert!(!encoded.contains("todo_id"));
    }

    #[test]
    fn todo_execution_catalog_requires_program_owned_completion_tools() {
        let schemas = todo_execution_tool_schemas();
        let names = schemas
            .iter()
            .map(|schema| schema["name"].as_str().unwrap().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            std::collections::BTreeSet::from([
                "todo_block".to_owned(),
                "todo_complete".to_owned(),
                "todo_get_context".to_owned(),
                "todo_progress_append".to_owned(),
            ])
        );
        assert_eq!(
            schemas[2]["parameters"]["properties"]["asset_update_suggestions"]["maxItems"],
            8
        );
    }

    #[test]
    fn todo_planning_read_catalog_uses_only_run_scoped_references() {
        let schemas = todo_planning_read_tool_schemas();
        let names = schemas
            .iter()
            .map(|schema| schema["name"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            std::collections::BTreeSet::from([
                "todo_dependency_options",
                "todo_execution_options",
                "todo_list",
                "todo_read_progress",
            ])
        );
        assert!(schemas.iter().all(|schema| {
            let encoded = schema.to_string();
            !encoded.contains("todo_id")
                && !encoded.contains("team_id")
                && !encoded.contains("agent_id")
        }));
    }

    #[test]
    fn team_chat_read_catalog_exposes_only_the_member_directory() {
        let schemas = team_chat_read_tool_schemas();
        assert_eq!(schemas.len(), 4);
        assert_eq!(schemas[0]["name"], "agent_workspace_snapshot");
        assert_eq!(schemas[1]["name"], "chat_list_members");
        assert_eq!(schemas[2]["name"], "chat_read_all_unread");
        assert_eq!(schemas[3]["name"], "chat_read_attachment");
        assert!(schemas
            .iter()
            .all(|schema| !schema.to_string().contains("agent_id")));
    }

    #[test]
    fn team_chat_write_catalog_accepts_only_run_scoped_mentions() {
        let schemas = team_chat_write_tool_schemas();
        assert_eq!(schemas.len(), 7);
        assert_eq!(schemas[0]["name"], "chat_document_create");
        assert_eq!(schemas[1]["name"], "agent_propose_member");
        assert_eq!(schemas[2]["name"], "agent_propose_existing_member");
        assert_eq!(schemas[3]["name"], "agent_propose_member_removal");
        assert_eq!(schemas[4]["name"], "chat_send_message");
        assert_eq!(schemas[5]["name"], "chat_heartbeat_complete");
        assert_eq!(schemas[6]["name"], "agent_cycle_complete");
        let encoded = schemas[4].to_string();
        assert!(encoded.contains("document_refs"));
        assert!(encoded.contains("mention_agent_refs"));
        assert!(encoded.contains("reply_to_message_ref"));
        assert!(encoded.contains("target_agent_ref"));
        assert!(encoded.contains("team_ref"));
        assert!(!encoded.contains("agent_id"));
        assert!(!encoded.contains("team_id"));
        assert!(!encoded.contains("conversation_id"));
    }

    #[test]
    fn todo_planning_write_catalog_never_accepts_native_ids_or_fake_completion() {
        let schemas = todo_planning_write_tool_schemas();
        let names = schemas
            .iter()
            .map(|schema| schema["name"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            std::collections::BTreeSet::from(["todo_add", "todo_reorder", "todo_update"])
        );
        let encoded = serde_json::to_string(&schemas).unwrap();
        assert!(encoded.contains("plugin_hints"));
        assert!(encoded.contains("plugin_ref"));
        assert!(encoded.contains("requirement_survey_read"));
        assert!(encoded.contains("requirement_survey_write"));
        assert!(!encoded.contains("todo_id"));
        assert!(!encoded.contains("team_id"));
        assert!(!encoded.contains("agent_id"));
        assert!(!encoded.contains("completed\""));
        assert!(!encoded.contains("blocked\""));
    }

    #[test]
    fn todo_plugin_refs_are_run_scoped_and_hide_frozen_identity() {
        let option = TrustedAgentPluginOption {
            installation_id: "installation-1".into(),
            plugin_id: "document-plugin".into(),
            component_key: "document-mcp".into(),
            component_revision: "revision-1".into(),
            release_id: "release-1".into(),
            release_digest: format!("sha256:{}", "a".repeat(64)),
            manifest_version: "1.2.3".into(),
            display_name: "Document Tools".into(),
            description: "Create documents".into(),
        };
        let first =
            todo_planning_reference("owner-1", "run-1", "plugin", &option.frozen_identity());
        let second =
            todo_planning_reference("owner-1", "run-2", "plugin", &option.frozen_identity());

        assert_ne!(first, second);
        assert!(!first.contains(&option.plugin_id));
        assert!(!first.contains(&option.installation_id));
        assert!(!first.contains(&option.release_id));
    }

    #[test]
    fn team_asset_refs_are_run_scoped_and_context_is_lane_bound() {
        let record = AgentAssetToolRecord {
            team_id: "team-1".into(),
            asset_id: "real-asset-id".into(),
            category: "overview".into(),
            title: "概览".into(),
            markdown: "secret body".into(),
            revision: 3,
        };
        let first = asset_reference("owner-1", "run-1", &record);
        let second = asset_reference("owner-1", "run-2", &record);
        assert_ne!(first, second);
        assert!(!first.contains(&record.asset_id));
        let review = AgentTeamAssetReview {
            id: "real-review-id".into(),
            team_id: "team-1".into(),
            todo_id: "todo-1".into(),
            todo_title: "核验架构".into(),
            source_agent_id: "agent-2".into(),
            source_agent_name: "架构师".into(),
            suggestion: AgentTeamAssetUpdateSuggestion {
                category: "architecture".into(),
                title: "事件驱动".into(),
                markdown: "正文".into(),
                rationale: "已核验".into(),
            },
            status: "pending".into(),
            resolved_asset_id: None,
            resolved_asset_revision: None,
            reviewed_by_agent_id: None,
            review_reason: String::new(),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        };
        let first_review = team_asset_review_reference("owner-1", "run-1", &review);
        let second_review = team_asset_review_reference("owner-1", "run-2", &review);
        assert_ne!(first_review, second_review);
        assert!(!first_review.contains(&review.id));

        let manager = AgentAssetToolBinding {
            team_id: "team-1".into(),
            agent_id: "agent-1".into(),
            todo_id: None,
            is_project_manager: true,
        };
        let manager_input = json!({
            "agent_execution_context": {
                "schema_version": 2,
                "room_id": "team-1",
                "agent_id": "agent-1",
                "is_project_manager": true
            }
        });
        assert!(validate_asset_run_context("main_chat", &manager_input, &manager).is_ok());
        assert!(validate_asset_run_context("task_execution", &manager_input, &manager).is_err());
        assert!(require_asset_manager_lane(&manager).is_ok());

        let member_manager_lane = AgentAssetToolBinding {
            is_project_manager: false,
            ..manager.clone()
        };
        assert!(require_asset_manager_lane(&member_manager_lane).is_err());

        let executor = AgentAssetToolBinding {
            todo_id: Some("todo-1".into()),
            is_project_manager: false,
            ..manager
        };
        let executor_input = json!({
            "agent_execution_context": {
                "schema_version": 2,
                "room_id": "team-1",
                "agent_id": "agent-1",
                "is_project_manager": false
            },
            "team_executor_context": {
                "delivery_lane": "executor",
                "todo_id": "todo-1",
                "team_id": "team-1",
                "agent_id": "agent-1"
            }
        });
        assert!(validate_asset_run_context("task_execution", &executor_input, &executor).is_ok());
        assert!(require_asset_manager_lane(&executor).is_err());
    }

    #[test]
    fn communication_context_is_separate_from_asset_and_executor_permissions() {
        let binding = AgentCommunicationToolBinding {
            room_id: "team-1".into(),
            agent_id: "agent-1".into(),
            conversation_kind: "project_team".into(),
            is_project_manager: true,
        };
        let input = json!({
            "agent_execution_context": {
                "schema_version": 2,
                "room_id": "team-1",
                "agent_id": "agent-1",
                "is_project_manager": true
            }
        });
        assert!(validate_communication_run_context("main_chat", &input, &binding).is_ok());

        let executor_input = json!({
            "agent_execution_context": input["agent_execution_context"].clone(),
            "team_executor_context": {
                "delivery_lane": "executor",
                "todo_id": "todo-1",
                "team_id": "team-1",
                "agent_id": "agent-1"
            }
        });
        assert!(
            validate_communication_run_context("task_execution", &executor_input, &binding)
                .is_err()
        );

        let direct_binding = AgentCommunicationToolBinding {
            conversation_kind: "agent_agent_direct".into(),
            ..binding
        };
        assert!(validate_communication_run_context("main_chat", &input, &direct_binding).is_err());
        let direct_input = json!({
            "agent_execution_context": {
                "schema_version": 3,
                "conversation_kind": "agent_agent_direct",
                "room_id": "team-1",
                "agent_id": "agent-1"
            }
        });
        assert!(
            validate_communication_run_context("main_chat", &direct_input, &direct_binding).is_ok()
        );
        let heartbeat_binding = AgentCommunicationToolBinding {
            conversation_kind: "human_agent_direct".into(),
            ..direct_binding
        };
        let heartbeat_input = json!({
            "agent_execution_context": {
                "schema_version": 3,
                "conversation_kind": "human_agent_direct",
                "room_id": "team-1",
                "agent_id": "agent-1"
            }
        });
        assert!(validate_communication_run_context(
            "main_chat",
            &heartbeat_input,
            &heartbeat_binding
        )
        .is_ok());
    }

    #[test]
    fn content_delivery_is_bounded_and_reports_the_next_offset() {
        let record = chatos_local_agent_protocol::LocalConversationAttachmentRecord {
            attachment_id: "attachment-1".into(),
            conversation_id: "conversation-1".into(),
            turn_id: "turn-1".into(),
            message_id: "message-1".into(),
            ordinal: 1,
            display_name: "notes.txt".into(),
            media_type: "text/plain".into(),
            byte_size: 4_096,
            sha256: "a".repeat(64),
            authorized_local_ref: "local-attachment:token".into(),
            metadata: json!({}),
            created_at_unix_ms: 1,
        };
        let output = content_output(&record, &vec![b'x'; 4_096], 0, 65_536).unwrap();
        assert_eq!(
            output["content"].as_str().unwrap().len(),
            MAXIMUM_CONTENT_READ_BYTES
        );
        assert_eq!(output["next_offset"], MAXIMUM_CONTENT_READ_BYTES);
    }

    #[test]
    fn model_delivery_uses_the_verified_image_encoding_not_the_declared_mime() {
        let record = chatos_local_agent_protocol::LocalConversationAttachmentRecord {
            attachment_id: "attachment-1".into(),
            conversation_id: "conversation-1".into(),
            turn_id: "turn-1".into(),
            message_id: "message-1".into(),
            ordinal: 1,
            display_name: "renamed.png".into(),
            media_type: "image/png".into(),
            byte_size: 0,
            sha256: "a".repeat(64),
            authorized_local_ref: "local-attachment:token".into(),
            metadata: json!({}),
            created_at_unix_ms: 1,
        };
        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg)
            .encode_image(&DynamicImage::new_rgb8(2, 2))
            .unwrap();
        let output = model_image_output(&record, &jpeg).unwrap();
        assert!(output["_model_input"][0]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
    }
}
