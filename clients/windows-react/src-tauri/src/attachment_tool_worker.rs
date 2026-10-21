use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chatos_local_agent_protocol::{
    ClaimNextToolCommand, CommitToolCommand, GetConversationAttachmentCommand, GetRunCommand,
    HostCommand, HostResult, LocalAgentToolClaim, LocalAgentToolOutcome,
};
use image::metadata::Orientation;
use image::{
    codecs::jpeg::JpegEncoder, DynamicImage, GenericImageView, ImageDecoder, ImageFormat,
    ImageReader,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
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
        AgentConversationStore,
    },
    agent_plugin_runtime::{AgentPluginRunContext, CAPABILITY_BROKER_TOOL_NAMES},
    agent_skill_management::AgentSkillStore,
    authentication::AuthenticationManager,
    conversation_attachments::AttachmentManager,
    host_lifecycle::HostManager,
    platform_execution_context::resolve_platform_execution_context,
    plugin_application::{PluginApplicationManager, TrustedAgentPluginOption},
    product_skill_runtime::ProductSkillRuntime,
    terminal_tool_worker::TerminalToolWorker,
};

pub const ATTACHMENT_READ_TOOL_NAME: &str = "local_attachment_read";
const TEAM_ASSET_TOOL_NAMES: [&str; 5] = [
    "team_asset_list",
    "team_asset_get",
    "team_asset_create",
    "team_asset_update",
    "team_asset_archive",
];
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
const MAXIMUM_CONTENT_READ_BYTES: usize = 2 * 1_024;
const MAXIMUM_MODEL_IMAGE_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_SOURCE_PIXELS: u64 = 40_000_000;
static ACTIVE_TEAM_TODO_DRAINS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

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
                    .chain(TODO_EXECUTION_TOOL_NAMES)
                    .chain(CAPABILITY_BROKER_TOOL_NAMES)
                    .chain(TODO_PLANNING_READ_TOOL_NAMES)
                    .chain(TEAM_CHAT_READ_TOOL_NAMES)
                    .chain(TEAM_CHAT_WRITE_TOOL_NAMES)
                    .chain(TODO_PLANNING_WRITE_TOOL_NAMES)
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

async fn execute_team_chat_read_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The team communication arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The team communication Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The team communication Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let request_key = claim.invocation.invocation_id.clone();
    let tool_name = claim.invocation.tool_name.clone();
    let limit = match tool_name.as_str() {
        "agent_workspace_snapshot" | "chat_list_members" if arguments.is_empty() => None,
        "chat_read_all_unread" => {
            Some(optional_planning_integer(arguments, "limit", 1, 500)?.unwrap_or(200))
        }
        "chat_read_attachment" => {
            Some(optional_planning_integer(arguments, "limit", 1, 12_000)?.unwrap_or(12_000))
        }
        _ => return Err("The team communication arguments are invalid.".into()),
    };
    let message_reference = if tool_name == "chat_read_attachment" {
        Some(required_bounded_string(arguments, "message_ref", 600)?)
    } else {
        None
    };
    let attachment_reference = if tool_name == "chat_read_attachment" {
        Some(required_bounded_string(arguments, "attachment_ref", 600)?)
    } else {
        None
    };
    let offset = if tool_name == "chat_read_attachment" {
        optional_planning_integer(arguments, "offset", 0, i64::MAX)?.unwrap_or(0) as usize
    } else {
        0
    };
    let (binding, output) =
        tauri::async_runtime::spawn_blocking(move || match tool_name.as_str() {
            "agent_workspace_snapshot" => store.agent_workspace_snapshot(&owner, &run_id),
            "chat_list_members" => store.team_chat_member_snapshot(&owner, &run_id),
            "chat_read_all_unread" => store.read_all_unread_team_chat(
                &owner,
                &run_id,
                &request_key,
                limit.ok_or_else(|| "The unread limit is unavailable.".to_owned())? as usize,
                now_unix_ms(),
            ),
            "chat_read_attachment" => store.read_agent_chat_attachment(
                &owner,
                &run_id,
                message_reference
                    .as_deref()
                    .ok_or_else(|| "The message reference is unavailable.".to_owned())?,
                attachment_reference
                    .as_deref()
                    .ok_or_else(|| "The attachment reference is unavailable.".to_owned())?,
                offset,
                limit.ok_or_else(|| "The attachment read limit is unavailable.".to_owned())?
                    as usize,
            ),
            _ => Err("The team communication tool is unavailable.".into()),
        })
        .await
        .map_err(|_| "The team member lookup task exited unexpectedly.".to_owned())??;
    validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

async fn execute_team_chat_write_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The team communication arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The team communication Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The team communication Run context is invalid.".into());
    }
    if claim.invocation.tool_name == "chat_document_create" {
        let name = required_bounded_string(arguments, "name", 240)?;
        let title = required_bounded_string(arguments, "title", 512)?;
        let markdown = required_bounded_string(arguments, "markdown", 2 * 1_024 * 1_024)?;
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = run.run_id.clone();
        let request_key = claim.invocation.invocation_id.clone();
        let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
            store.create_agent_chat_document(
                &owner,
                &run_id,
                &request_key,
                &name,
                &title,
                &markdown,
                now_unix_ms(),
            )
        })
        .await
        .map_err(|_| "The Agent document task exited unexpectedly.".to_owned())??;
        validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
        return Ok(LocalAgentToolOutcome::Succeeded { output });
    }
    if matches!(
        claim.invocation.tool_name.as_str(),
        "agent_propose_member" | "agent_propose_existing_member" | "agent_propose_member_removal"
    ) {
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = run.run_id.clone();
        let request_key = claim.invocation.invocation_id.clone();
        let tool_name = claim.invocation.tool_name.clone();
        let arguments = arguments.clone();
        let (binding, output) =
            tauri::async_runtime::spawn_blocking(move || match tool_name.as_str() {
                "agent_propose_member" => store.create_agent_creation_proposal(
                    &owner,
                    &run_id,
                    &request_key,
                    &required_bounded_string(&arguments, "name", 120)?,
                    &required_bounded_string(&arguments, "role", 160)?,
                    optional_bounded_string(&arguments, "responsibility", 8_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    &required_bounded_string(&arguments, "role_prompt", 32_000)?,
                    optional_bounded_string(&arguments, "thinking_level", 16)?.as_deref(),
                    &required_bounded_string(&arguments, "profession_key", 120)?,
                    optional_bounded_string(&arguments, "rationale", 4_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    now_unix_ms(),
                ),
                "agent_propose_existing_member" => store.create_agent_membership_proposal(
                    &owner,
                    &run_id,
                    &request_key,
                    &required_bounded_string(&arguments, "team_ref", 600)?,
                    &required_bounded_string(&arguments, "target_agent_ref", 600)?,
                    &required_bounded_string(&arguments, "role", 160)?,
                    optional_bounded_string(&arguments, "responsibility", 8_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    now_unix_ms(),
                ),
                "agent_propose_member_removal" => store.create_agent_removal_proposal(
                    &owner,
                    &run_id,
                    &request_key,
                    &required_bounded_string(&arguments, "target_agent_ref", 600)?,
                    &required_bounded_string(&arguments, "reason", 4_000)?,
                    optional_bounded_string(&arguments, "handoff_plan", 8_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    now_unix_ms(),
                ),
                _ => Err("The Agent staffing proposal tool is unavailable.".into()),
            })
            .await
            .map_err(|_| "The Agent staffing proposal task exited unexpectedly.".to_owned())??;
        validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
        return Ok(LocalAgentToolOutcome::Succeeded { output });
    }
    if matches!(
        claim.invocation.tool_name.as_str(),
        "chat_heartbeat_complete" | "agent_cycle_complete"
    ) {
        if !arguments.is_empty() {
            return Err("The Agent cycle completion arguments are invalid.".into());
        }
        let heartbeat_only = claim.invocation.tool_name == "chat_heartbeat_complete";
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = run.run_id.clone();
        let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
            store.validate_agent_communication_cycle_completion(&owner, &run_id, heartbeat_only)
        })
        .await
        .map_err(|_| "The Agent cycle completion task exited unexpectedly.".to_owned())??;
        validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
        return Ok(LocalAgentToolOutcome::Succeeded { output });
    }
    if claim.invocation.tool_name != "chat_send_message" {
        return Err("The team communication mutation is unavailable.".into());
    }
    let content = required_bounded_string(arguments, "content", 2_000)?;
    let reply_reference = optional_bounded_string(arguments, "reply_to_message_ref", 600)?;
    let target_agent_reference = optional_bounded_string(arguments, "target_agent_ref", 600)?;
    let team_reference = optional_bounded_string(arguments, "team_ref", 600)?;
    if [
        reply_reference.is_some(),
        target_agent_reference.is_some(),
        team_reference.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count()
        > 1
    {
        return Err("The team communication target is ambiguous.".into());
    }
    let mention_references =
        planning_string_array(arguments, "mention_agent_refs", 0, 64, 600)?.unwrap_or_default();
    let document_references =
        planning_string_array(arguments, "document_refs", 0, 5, 600)?.unwrap_or_default();
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let request_key = claim.invocation.invocation_id.clone();
    let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
        store.send_team_chat_message(
            &owner,
            &run_id,
            &request_key,
            &content,
            reply_reference.as_deref(),
            target_agent_reference.as_deref(),
            team_reference.as_deref(),
            &document_references,
            &mention_references,
            now_unix_ms(),
        )
    })
    .await
    .map_err(|_| "The team message task exited unexpectedly.".to_owned())??;
    validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

pub(crate) fn schedule_team_todo_drain(app: AppHandle, owner_user_id: String, team_id: String) {
    tauri::async_runtime::spawn(async move {
        let key = format!("{owner_user_id}\0{team_id}");
        let active = ACTIVE_TEAM_TODO_DRAINS.get_or_init(|| Mutex::new(HashSet::new()));
        if !active.lock().await.insert(key.clone()) {
            return;
        }
        let mut consecutive_failures = 0_u8;
        loop {
            let reconciled = load_agent_team_runs(
                owner_user_id.clone(),
                team_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<HostManager>(),
                app.state::<TerminalToolWorker>(),
            )
            .await
            .is_ok();
            let dispatched = dispatch_next_agent_team_delivery(
                owner_user_id.clone(),
                team_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<AgentSkillStore>(),
                app.state::<HostManager>(),
                app.state::<AttachmentManager>(),
                app.state::<AttachmentToolWorker>(),
                app.state::<TerminalToolWorker>(),
            )
            .await;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.clone();
            let team = team_id.clone();
            let has_work = tauri::async_runtime::spawn_blocking(move || {
                store.team_todo_scheduler_has_work(&owner, &team)
            })
            .await
            .ok()
            .and_then(Result::ok);
            if has_work == Some(false) {
                break;
            }
            if reconciled && dispatched.is_ok() && has_work.is_some() {
                consecutive_failures = 0;
            } else {
                consecutive_failures = consecutive_failures.saturating_add(1);
                if consecutive_failures >= 10 {
                    break;
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
        active.lock().await.remove(&key);
    });
}

pub(crate) fn schedule_agent_communication_drain(
    app: AppHandle,
    owner_user_id: String,
    room_id: String,
) {
    tauri::async_runtime::spawn(async move {
        let key = format!("communication\0{owner_user_id}\0{room_id}");
        let active = ACTIVE_TEAM_TODO_DRAINS.get_or_init(|| Mutex::new(HashSet::new()));
        if !active.lock().await.insert(key.clone()) {
            return;
        }
        let mut consecutive_failures = 0_u8;
        loop {
            let reconciled = reconcile_agent_communication_runs(
                owner_user_id.clone(),
                room_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<HostManager>(),
            )
            .await
            .is_ok();
            let dispatched = dispatch_next_agent_communication_delivery(
                owner_user_id.clone(),
                room_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<AgentSkillStore>(),
                app.state::<HostManager>(),
                app.state::<AttachmentToolWorker>(),
                app.state::<TerminalToolWorker>(),
            )
            .await;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.clone();
            let room = room_id.clone();
            let has_work = tauri::async_runtime::spawn_blocking(move || {
                store.communication_scheduler_has_work(&owner, &room)
            })
            .await
            .ok()
            .and_then(Result::ok);
            if has_work == Some(false) {
                break;
            }
            if reconciled && dispatched.is_ok() && has_work.is_some() {
                consecutive_failures = 0;
            } else {
                consecutive_failures = consecutive_failures.saturating_add(1);
                if consecutive_failures >= 10 {
                    break;
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
        active.lock().await.remove(&key);
    });
}

async fn execute_todo_planning_read_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TODO_PLANNING_READ_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested Todo planning tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Todo planning Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The Todo planning Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let validation_store = store.clone();
    let validation_owner = owner.clone();
    let validation_run = run_id.clone();
    let (expected_binding, _) = tauri::async_runtime::spawn_blocking(move || {
        validation_store.load_agent_asset_tool_records(&validation_owner, &validation_run)
    })
    .await
    .map_err(|_| "The Todo planning binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &expected_binding)?;
    if expected_binding.todo_id.is_some() {
        return Err("Todo planning tools are unavailable in an executor Run.".into());
    }
    let output = if claim.invocation.tool_name == "todo_read_progress" {
        let todo_reference = required_bounded_string(arguments, "todo_ref", 600)?;
        let limit = arguments
            .get("limit")
            .map(|value| {
                value
                    .as_u64()
                    .filter(|value| (1..=500).contains(value))
                    .map(|value| value as usize)
                    .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())
            })
            .transpose()?
            .unwrap_or(100);
        let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
            store.todo_planning_progress(&owner, &run_id, &todo_reference, limit)
        })
        .await
        .map_err(|_| "The Todo progress lookup task exited unexpectedly.".to_owned())??;
        if binding != expected_binding {
            return Err("The Todo planning binding changed during the operation.".into());
        }
        output
    } else {
        let include_terminal = arguments
            .get("include_terminal")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())
            })
            .transpose()?
            .unwrap_or(claim.invocation.tool_name != "todo_list");
        let (binding, snapshot) = tauri::async_runtime::spawn_blocking(move || {
            store.todo_planning_tool_snapshot(&owner, &run_id, include_terminal)
        })
        .await
        .map_err(|_| "The Todo planning lookup task exited unexpectedly.".to_owned())??;
        if binding != expected_binding {
            return Err("The Todo planning binding changed during the operation.".into());
        }
        match claim.invocation.tool_name.as_str() {
            "todo_execution_options" => {
                require_todo_planning_manager(&binding)?;
                let plugin_manager = app.state::<PluginApplicationManager>();
                let host = app.state::<HostManager>();
                let plugins = plugin_manager
                    .list_agent_plugins(host.inner(), owner_user_id)
                    .await?
                    .into_iter()
                    .map(|option| {
                        json!({
                            "plugin_ref": todo_planning_reference(
                                owner_user_id,
                                &run.run_id,
                                "plugin",
                                &option.frozen_identity(),
                            ),
                            "name": option.display_name,
                            "description": option.description,
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "teams": [snapshot["team"].clone()],
                    "builtin_capabilities": snapshot["builtin_capabilities"].clone(),
                    "plugins": plugins,
                })
            }
            "todo_dependency_options" => {
                require_todo_planning_manager(&binding)?;
                let team_reference = required_bounded_string(arguments, "team_ref", 600)?;
                if snapshot["team"]["team_ref"].as_str() != Some(team_reference.as_str()) {
                    return Err("The team reference is unavailable for this Run.".into());
                }
                let todos = snapshot["todos"]
                    .as_array()
                    .ok_or_else(|| "The Todo planning snapshot is invalid.".to_owned())?
                    .iter()
                    .filter(|todo| todo["status"].as_str() != Some("cancelled"))
                    .cloned()
                    .collect::<Vec<_>>();
                json!({"todos": todos})
            }
            "todo_list" => json!({"todos": snapshot["todos"].clone()}),
            _ => return Err("The requested Todo planning tool is unavailable.".into()),
        }
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

fn require_todo_planning_manager(binding: &AgentAssetToolBinding) -> Result<(), String> {
    if binding.todo_id.is_none() && binding.is_project_manager {
        Ok(())
    } else {
        Err("Only the active project manager Agent may plan team Todos.".into())
    }
}

async fn execute_todo_planning_write_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TODO_PLANNING_WRITE_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested Todo planning mutation is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Todo planning Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The Todo planning Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let validation_store = store.clone();
    let validation_owner = owner.clone();
    let validation_run = run_id.clone();
    let (expected_binding, _) = tauri::async_runtime::spawn_blocking(move || {
        validation_store.load_agent_asset_tool_records(&validation_owner, &validation_run)
    })
    .await
    .map_err(|_| "The Todo planning binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &expected_binding)?;
    require_todo_planning_manager(&expected_binding)?;
    let now = now_unix_ms();
    let (binding, output) = match claim.invocation.tool_name.as_str() {
        "todo_add" => {
            let plugins =
                resolve_todo_plugin_hints(app, owner_user_id, &run.run_id, arguments).await?;
            let input = TodoPlanningCreateInput {
                request_key: claim.invocation.invocation_id.clone(),
                team_reference: required_bounded_string(arguments, "team_ref", 600)?,
                assignee_reference: required_bounded_string(arguments, "assignee_ref", 600)?,
                title: required_bounded_string(arguments, "title", 500)?,
                detail: optional_planning_text(arguments, "detail", 16_000, true)?
                    .unwrap_or_default(),
                priority: optional_planning_integer(arguments, "priority", 0, 100)?.unwrap_or(50),
                dependency_references: planning_string_array(
                    arguments,
                    "depends_on_todo_refs",
                    0,
                    64,
                    600,
                )?
                .unwrap_or_default(),
                execution_contract: AgentTodoExecutionContract {
                    objective: required_bounded_string(arguments, "objective", 8_000)?,
                    scope: required_bounded_string(arguments, "scope", 16_000)?,
                    expected_outputs: planning_string_array(
                        arguments,
                        "expected_outputs",
                        1,
                        64,
                        4_000,
                    )?
                    .ok_or_else(invalid_todo_planning_arguments)?,
                    acceptance_criteria: planning_string_array(
                        arguments,
                        "acceptance_criteria",
                        1,
                        64,
                        4_000,
                    )?
                    .ok_or_else(invalid_todo_planning_arguments)?,
                    constraints: planning_string_array(arguments, "constraints", 0, 64, 4_000)?
                        .unwrap_or_default(),
                },
                requires_execution: arguments
                    .get("requires_execution")
                    .and_then(Value::as_bool)
                    .ok_or_else(invalid_todo_planning_arguments)?,
                builtin_capabilities: planning_string_array(
                    arguments,
                    "builtin_capabilities",
                    0,
                    3,
                    64,
                )?
                .ok_or_else(invalid_todo_planning_arguments)?,
                plugins,
            };
            tauri::async_runtime::spawn_blocking(move || {
                store.create_todo_from_planning_tool(&owner, &run_id, input, now)
            })
            .await
            .map_err(|_| "The Todo creation task exited unexpectedly.".to_owned())??
        }
        "todo_update" => {
            let input = TodoPlanningUpdateInput {
                todo_reference: required_bounded_string(arguments, "todo_ref", 600)?,
                title: optional_planning_text(arguments, "title", 500, false)?,
                detail: optional_planning_text(arguments, "detail", 16_000, true)?,
                priority: optional_planning_integer(arguments, "priority", 0, 100)?,
                status: optional_planning_text(arguments, "status", 32, false)?,
                dependency_references: planning_string_array(
                    arguments,
                    "depends_on_todo_refs",
                    0,
                    64,
                    600,
                )?,
                objective: optional_planning_text(arguments, "objective", 8_000, false)?,
                scope: optional_planning_text(arguments, "scope", 16_000, false)?,
                expected_outputs: planning_string_array(
                    arguments,
                    "expected_outputs",
                    1,
                    64,
                    4_000,
                )?,
                acceptance_criteria: planning_string_array(
                    arguments,
                    "acceptance_criteria",
                    1,
                    64,
                    4_000,
                )?,
                constraints: planning_string_array(arguments, "constraints", 0, 64, 4_000)?,
            };
            tauri::async_runtime::spawn_blocking(move || {
                store.update_todo_from_planning_tool(&owner, &run_id, input, now)
            })
            .await
            .map_err(|_| "The Todo update task exited unexpectedly.".to_owned())??
        }
        "todo_reorder" => {
            let references = planning_string_array(arguments, "todo_refs", 1, 500, 600)?
                .ok_or_else(invalid_todo_planning_arguments)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.reorder_todos_from_planning_tool(&owner, &run_id, &references, now)
            })
            .await
            .map_err(|_| "The Todo reorder task exited unexpectedly.".to_owned())??
        }
        _ => return Err("The requested Todo planning mutation is unavailable.".into()),
    };
    if binding != expected_binding {
        return Err("The Todo planning binding changed during the operation.".into());
    }
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

async fn resolve_todo_plugin_hints(
    app: &AppHandle,
    owner_user_id: &str,
    run_id: &str,
    arguments: &Map<String, Value>,
) -> Result<Vec<AgentTodoPluginSelection>, String> {
    let Some(raw_hints) = arguments.get("plugin_hints") else {
        return Ok(Vec::new());
    };
    let hints = raw_hints
        .as_array()
        .filter(|hints| hints.len() <= 32)
        .ok_or_else(invalid_todo_planning_arguments)?;
    let plugin_manager = app.state::<PluginApplicationManager>();
    let host = app.state::<HostManager>();
    let options = plugin_manager
        .list_agent_plugins(host.inner(), owner_user_id)
        .await?;
    let by_reference = options
        .into_iter()
        .map(|option| {
            (
                todo_planning_reference(owner_user_id, run_id, "plugin", &option.frozen_identity()),
                option,
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut selected_components = HashSet::new();
    hints
        .iter()
        .map(|hint| {
            let hint = hint
                .as_object()
                .filter(|hint| {
                    hint.len() <= 2
                        && hint
                            .keys()
                            .all(|key| matches!(key.as_str(), "plugin_ref" | "reason"))
                })
                .ok_or_else(invalid_todo_planning_arguments)?;
            let plugin_reference = required_bounded_string(hint, "plugin_ref", 600)?;
            let reason = optional_planning_text(hint, "reason", 1_000, true)?.unwrap_or_default();
            let option = by_reference.get(&plugin_reference).ok_or_else(|| {
                "Plugin option is unavailable or expired; call todo_execution_options again."
                    .to_owned()
            })?;
            if !selected_components.insert(option.selection_id()) {
                return Err("The same Plugin component cannot be selected twice.".into());
            }
            Ok(todo_plugin_selection(option, reason))
        })
        .collect()
}

fn todo_plugin_selection(
    option: &TrustedAgentPluginOption,
    reason: String,
) -> AgentTodoPluginSelection {
    AgentTodoPluginSelection {
        installation_id: option.installation_id.clone(),
        plugin_id: option.plugin_id.clone(),
        component_key: option.component_key.clone(),
        component_revision: option.component_revision.clone(),
        release_id: option.release_id.clone(),
        release_digest: option.release_digest.clone(),
        manifest_version: option.manifest_version.clone(),
        display_name: option.display_name.clone(),
        reason,
    }
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

async fn execute_todo_execution_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TODO_EXECUTION_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested Todo execution tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Todo execution arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Todo execution Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The Todo execution Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let now = now_unix_ms();
    let validation_store = store.clone();
    let validation_owner = owner.clone();
    let validation_run = run_id.clone();
    let (expected_binding, _) = tauri::async_runtime::spawn_blocking(move || {
        validation_store.load_agent_asset_tool_records(&validation_owner, &validation_run)
    })
    .await
    .map_err(|_| "The Todo execution binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &expected_binding)?;
    if expected_binding.todo_id.is_none() {
        return Err("The Todo execution Run context is invalid.".into());
    }
    let (binding, output) = match claim.invocation.tool_name.as_str() {
        "todo_get_context" => tauri::async_runtime::spawn_blocking(move || {
            store.todo_execution_tool_context(&owner, &run_id)
        })
        .await
        .map_err(|_| "The Todo context task exited unexpectedly.".to_owned())??,
        "todo_progress_append" => {
            let stage = optional_bounded_string(arguments, "stage", 240)?.unwrap_or_default();
            let detail = required_bounded_string(arguments, "detail", 16_000)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.append_todo_execution_progress(&owner, &run_id, &stage, &detail, now)
            })
            .await
            .map_err(|_| "The Todo progress task exited unexpectedly.".to_owned())??
        }
        "todo_complete" => {
            let summary = required_bounded_string(arguments, "summary", 16_000)?;
            let verified = app
                .state::<TerminalToolWorker>()
                .project_write_completion_verified(owner_user_id, &run.run_id);
            tauri::async_runtime::spawn_blocking(move || {
                store.finish_todo_execution(&owner, &run_id, true, &summary, verified, now)
            })
            .await
            .map_err(|_| "The Todo completion task exited unexpectedly.".to_owned())??
        }
        "todo_block" => {
            let reason = required_bounded_string(arguments, "reason", 8_000)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.finish_todo_execution(&owner, &run_id, false, &reason, false, now)
            })
            .await
            .map_err(|_| "The Todo blocking task exited unexpectedly.".to_owned())??
        }
        _ => return Err("The requested Todo execution tool is unavailable.".into()),
    };
    if binding != expected_binding {
        return Err("The Todo execution binding changed during the operation.".into());
    }
    Ok(LocalAgentToolOutcome::Succeeded { output })
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

async fn execute_team_asset_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TEAM_ASSET_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested team asset tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The team asset arguments are invalid.".to_owned())?;
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The team asset Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The team asset Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let (binding, records) = tauri::async_runtime::spawn_blocking(move || {
        store.load_agent_asset_tool_records(&owner, &run_id)
    })
    .await
    .map_err(|_| "The team asset lookup task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &binding)?;
    let output = match claim.invocation.tool_name.as_str() {
        "team_asset_list" => {
            let category = arguments
                .get("category")
                .map(|value| value.as_str().ok_or_else(invalid_asset_arguments))
                .transpose()?
                .filter(|value| !value.is_empty());
            let assets = records
                .iter()
                .filter(|record| category.is_none_or(|value| record.category == value))
                .map(|record| asset_summary(owner_user_id, &run.run_id, record))
                .collect::<Vec<_>>();
            json!({"assets": assets, "snapshot": binding.todo_id.is_some()})
        }
        "team_asset_get" => {
            let asset_ref = arguments
                .get("asset_ref")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 160)
                .ok_or_else(invalid_asset_arguments)?;
            let record = records
                .iter()
                .find(|record| asset_reference(owner_user_id, &run.run_id, record) == asset_ref)
                .ok_or_else(|| {
                    "The team asset reference is unavailable for this Run.".to_owned()
                })?;
            json!({
                "asset_ref": asset_ref,
                "category": record.category,
                "title": record.title,
                "markdown": record.markdown,
                "revision": record.revision,
                "snapshot": binding.todo_id.is_some(),
            })
        }
        "team_asset_create" => {
            require_asset_manager(&binding)?;
            let category = required_asset_string(arguments, "category", 64)?.to_owned();
            let title = required_asset_string(arguments, "title", 240)?.to_owned();
            let markdown = required_asset_text(arguments, "markdown", 128_000)?.to_owned();
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = binding.team_id.clone();
            let agent_id = binding.agent_id.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || {
                store.save_project_team_asset_as_manager(
                    &owner,
                    &team_id,
                    &agent_id,
                    None,
                    &category,
                    &title,
                    &markdown,
                    None,
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset create task exited unexpectedly.".to_owned())??;
            saved_asset_output(owner_user_id, &run.run_id, saved)
        }
        "team_asset_update" => {
            require_asset_manager(&binding)?;
            let record = referenced_asset(arguments, owner_user_id, &run.run_id, &records)?;
            let expected_revision = required_asset_revision(arguments)?;
            if record.revision != expected_revision {
                return Err("The team asset revision changed; list the assets again.".into());
            }
            let category = required_asset_string(arguments, "category", 64)?.to_owned();
            let title = required_asset_string(arguments, "title", 240)?.to_owned();
            let markdown = required_asset_text(arguments, "markdown", 128_000)?.to_owned();
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = binding.team_id.clone();
            let agent_id = binding.agent_id.clone();
            let asset_id = record.asset_id.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || {
                store.save_project_team_asset_as_manager(
                    &owner,
                    &team_id,
                    &agent_id,
                    Some(&asset_id),
                    &category,
                    &title,
                    &markdown,
                    Some(expected_revision),
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset update task exited unexpectedly.".to_owned())??;
            saved_asset_output(owner_user_id, &run.run_id, saved)
        }
        "team_asset_archive" => {
            require_asset_manager(&binding)?;
            let record = referenced_asset(arguments, owner_user_id, &run.run_id, &records)?;
            let expected_revision = required_asset_revision(arguments)?;
            if record.revision != expected_revision {
                return Err("The team asset revision changed; list the assets again.".into());
            }
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = binding.team_id.clone();
            let agent_id = binding.agent_id.clone();
            let asset_id = record.asset_id.clone();
            let archived = tauri::async_runtime::spawn_blocking(move || {
                store.archive_project_team_asset_as_manager(
                    &owner,
                    &team_id,
                    &agent_id,
                    &asset_id,
                    expected_revision,
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset archive task exited unexpectedly.".to_owned())??;
            saved_asset_output(owner_user_id, &run.run_id, archived)
        }
        _ => return Err("The requested team asset tool is unavailable.".into()),
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

fn require_asset_manager(binding: &AgentAssetToolBinding) -> Result<(), String> {
    if binding.todo_id.is_some() || !binding.is_project_manager {
        Err("Only the active project manager Agent may change team assets.".into())
    } else {
        Ok(())
    }
}

fn referenced_asset<'a>(
    arguments: &Map<String, Value>,
    owner_user_id: &str,
    run_id: &str,
    records: &'a [AgentAssetToolRecord],
) -> Result<&'a AgentAssetToolRecord, String> {
    let asset_ref = required_asset_string(arguments, "asset_ref", 160)?;
    records
        .iter()
        .find(|record| asset_reference(owner_user_id, run_id, record) == asset_ref)
        .ok_or_else(|| "The team asset reference is unavailable for this Run.".to_owned())
}

fn required_asset_revision(arguments: &Map<String, Value>) -> Result<i64, String> {
    arguments
        .get("expected_revision")
        .and_then(Value::as_i64)
        .filter(|revision| *revision > 0)
        .ok_or_else(invalid_asset_arguments)
}

fn required_asset_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.contains('\0')
        })
        .ok_or_else(invalid_asset_arguments)
}

fn required_asset_text<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| value.chars().count() <= maximum && !value.contains('\0'))
        .ok_or_else(invalid_asset_arguments)
}

fn saved_asset_output(
    owner_user_id: &str,
    run_id: &str,
    asset: crate::agent_conversation::AgentTeamAsset,
) -> Value {
    let record = AgentAssetToolRecord {
        asset_id: asset.id,
        category: asset.category,
        title: asset.title,
        markdown: asset.markdown,
        revision: asset.revision,
    };
    json!({
        "asset_ref": asset_reference(owner_user_id, run_id, &record),
        "category": record.category,
        "title": record.title,
        "revision": record.revision,
    })
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn validate_asset_run_context(
    profile_key: &str,
    input: &Value,
    binding: &AgentAssetToolBinding,
) -> Result<(), String> {
    let identity = input
        .get("agent_execution_context")
        .and_then(Value::as_object)
        .ok_or_else(|| "The team asset Run context is invalid.".to_owned())?;
    if identity.get("schema_version").and_then(Value::as_u64) != Some(2)
        || identity.get("room_id").and_then(Value::as_str) != Some(&binding.team_id)
        || identity.get("agent_id").and_then(Value::as_str) != Some(&binding.agent_id)
        || identity.get("is_project_manager").and_then(Value::as_bool)
            != Some(binding.is_project_manager)
    {
        return Err("The team asset Run context is invalid.".into());
    }
    match binding.todo_id.as_deref() {
        Some(todo_id) => {
            let executor = input
                .get("team_executor_context")
                .and_then(Value::as_object)
                .ok_or_else(|| "The team asset executor context is invalid.".to_owned())?;
            if profile_key != "task_execution"
                || executor.get("delivery_lane").and_then(Value::as_str) != Some("executor")
                || executor.get("todo_id").and_then(Value::as_str) != Some(todo_id)
                || executor.get("team_id").and_then(Value::as_str) != Some(&binding.team_id)
                || executor.get("agent_id").and_then(Value::as_str) != Some(&binding.agent_id)
            {
                return Err("The team asset executor context is invalid.".into());
            }
        }
        None if profile_key == "main_chat" && input.get("team_executor_context").is_none() => {}
        None => return Err("The team asset manager context is invalid.".into()),
    }
    Ok(())
}

fn validate_communication_run_context(
    profile_key: &str,
    input: &Value,
    binding: &AgentCommunicationToolBinding,
) -> Result<(), String> {
    let identity = input
        .get("agent_execution_context")
        .and_then(Value::as_object)
        .ok_or_else(|| "The Agent communication Run context is invalid.".to_owned())?;
    let schema_matches = match binding.conversation_kind.as_str() {
        "project_team" => {
            identity.get("schema_version").and_then(Value::as_u64) == Some(2)
                && identity.get("is_project_manager").and_then(Value::as_bool)
                    == Some(binding.is_project_manager)
        }
        "agent_agent_direct" => {
            identity.get("schema_version").and_then(Value::as_u64) == Some(3)
                && identity.get("conversation_kind").and_then(Value::as_str)
                    == Some("agent_agent_direct")
        }
        "human_agent_direct" => {
            (identity.get("schema_version").and_then(Value::as_u64) == Some(1)
                && identity.get("conversation_kind").is_none())
                || (identity.get("schema_version").and_then(Value::as_u64) == Some(3)
                    && identity.get("conversation_kind").and_then(Value::as_str)
                        == Some("human_agent_direct"))
        }
        _ => false,
    };
    if profile_key != "main_chat"
        || input.get("team_executor_context").is_some()
        || !schema_matches
        || identity.get("room_id").and_then(Value::as_str) != Some(&binding.room_id)
        || identity.get("agent_id").and_then(Value::as_str) != Some(&binding.agent_id)
    {
        return Err("The Agent communication Run context is invalid.".into());
    }
    Ok(())
}

fn asset_summary(owner_user_id: &str, run_id: &str, record: &AgentAssetToolRecord) -> Value {
    json!({
        "asset_ref": asset_reference(owner_user_id, run_id, record),
        "category": record.category,
        "title": record.title,
        "revision": record.revision,
    })
}

fn asset_reference(owner_user_id: &str, run_id: &str, record: &AgentAssetToolRecord) -> String {
    let digest = Sha256::digest(
        format!(
            "chatos-team-asset-ref-v1\n{owner_user_id}\n{run_id}\n{}\n{}",
            record.asset_id, record.revision
        )
        .as_bytes(),
    );
    format!("asset_ref_{digest:x}")
}

fn invalid_asset_arguments() -> String {
    "The team asset arguments are invalid.".into()
}

async fn execute_attachment_read(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if claim.invocation.tool_name != ATTACHMENT_READ_TOOL_NAME {
        return Err("The requested local platform tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The local attachment arguments are invalid.".to_string())?;
    let authorized_local_ref = string_argument(arguments, "authorized_local_ref")?;
    let offset = integer_argument(arguments, "offset", 0)?;
    let limit = integer_argument(arguments, "limit", 16_384)?;
    if offset < 0 || !(1..=65_536).contains(&limit) {
        return Err("The local attachment arguments are invalid.".into());
    }
    let host = app.state::<HostManager>();
    let run = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = run else {
        return Err("The local attachment Run context is invalid.".into());
    };
    let conversation_id = run
        .input
        .as_object()
        .and_then(|input| {
            input
                .get("source_conversation_id")
                .or_else(|| input.get("conversation_id"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The local attachment Run context is invalid.".to_string())?;
    let delivery = arguments.get("delivery").and_then(Value::as_str).unwrap_or(
        if run.profile_key == "main_chat" {
            "model"
        } else {
            "content"
        },
    );
    if !matches!(delivery, "model" | "content") {
        return Err(
            "This Windows attachment worker supports model or content delivery only.".into(),
        );
    }
    let attachment = host
        .execute(HostCommand::GetConversationAttachment(
            GetConversationAttachmentCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                authorized_local_ref: authorized_local_ref.to_owned(),
            },
        ))
        .await?;
    let HostResult::ConversationAttachment { attachment } = attachment else {
        return Err("Local Agent Host returned an unexpected attachment result".into());
    };
    let attachment = attachment
        .ok_or_else(|| "The attachment is not authorized for this conversation.".to_string())?;
    let vault = app.state::<AttachmentManager>();
    let bytes = vault.verified_record_bytes(owner_user_id, conversation_id, &attachment)?;
    let output = if delivery == "model" {
        model_image_output(&attachment, &bytes)?
    } else {
        content_output(&attachment, &bytes, offset as usize, limit as usize)?
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

fn content_output(
    attachment: &chatos_local_agent_protocol::LocalConversationAttachmentRecord,
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    if offset > bytes.len() {
        return Err("The local attachment offset is invalid.".into());
    }
    let count = limit
        .min(MAXIMUM_CONTENT_READ_BYTES)
        .min(bytes.len() - offset);
    let end = offset + count;
    let slice = &bytes[offset..end];
    let (encoding, content) = match std::str::from_utf8(slice) {
        Ok(text) => ("utf-8", text.to_owned()),
        Err(_) => ("base64", BASE64.encode(slice)),
    };
    Ok(json!({
        "display_name": attachment.display_name,
        "media_type": attachment.media_type,
        "byte_size": attachment.byte_size,
        "sha256": attachment.sha256,
        "sha256_verified": true,
        "offset": offset,
        "next_offset": (end < bytes.len()).then_some(end),
        "encoding": encoding,
        "content": content,
        "delivery": "content"
    }))
}

fn model_image_output(
    attachment: &chatos_local_agent_protocol::LocalConversationAttachmentRecord,
    bytes: &[u8],
) -> Result<Value, String> {
    if !attachment
        .media_type
        .to_ascii_lowercase()
        .starts_with("image/")
    {
        return Err("This attachment is not an image. Use content delivery for text.".into());
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "The verified attachment could not be decoded as an image.".to_string())?;
    let format = reader.format();
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "The verified attachment could not be decoded as an image.".to_string())?;
    let orientation = decoder
        .orientation()
        .map_err(|_| "The verified attachment image metadata is invalid.".to_string())?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|_| "The verified attachment could not be decoded as an image.".to_string())?;
    let (width, height) = image.dimensions();
    if width == 0
        || height == 0
        || u64::from(width).saturating_mul(u64::from(height)) > MAXIMUM_SOURCE_PIXELS
    {
        return Err("The verified image exceeds the safe pixel limit.".into());
    }
    let original_mime = format.and_then(model_media_type);
    let preserve_original = orientation == Orientation::NoTransforms
        && bytes.len() <= MAXIMUM_MODEL_IMAGE_BYTES
        && width <= 4_096
        && height <= 4_096;
    let (model_bytes, model_mime) = if let Some(mime) = original_mime.filter(|_| preserve_original)
    {
        (bytes.to_vec(), mime.to_owned())
    } else {
        image.apply_orientation(orientation);
        normalize_model_image(&image)?
    };
    let fields = json!({
        "display_name": attachment.display_name,
        "media_type": attachment.media_type,
        "byte_size": attachment.byte_size,
        "sha256": attachment.sha256,
        "sha256_verified": true,
        "delivery": "model",
        "model_image_supplied": true
    });
    let mut output = fields.as_object().cloned().unwrap_or_default();
    output.insert("text".into(), Value::String(fields.to_string()));
    output.insert("_structured_result".into(), fields);
    output.insert(
        "_model_input".into(),
        json!([{
            "type": "input_image",
            "image_url": format!("data:{model_mime};base64,{}", BASE64.encode(model_bytes)),
            "detail": "high"
        }]),
    );
    Ok(Value::Object(output))
}

fn model_media_type(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

fn normalize_model_image(image: &image::DynamicImage) -> Result<(Vec<u8>, String), String> {
    for maximum in [4_096, 3_072, 2_048, 1_536] {
        let thumbnail = image.thumbnail(maximum, maximum).to_rgb8();
        for quality in [88, 72] {
            let mut encoded = Vec::new();
            JpegEncoder::new_with_quality(&mut encoded, quality)
                .encode_image(&thumbnail)
                .map_err(|_| "The verified image could not be normalized.".to_string())?;
            if encoded.len() <= MAXIMUM_MODEL_IMAGE_BYTES {
                return Ok((encoded, "image/jpeg".into()));
            }
        }
    }
    Err("The image could not fit the model transport budget.".into())
}

fn string_argument<'a>(arguments: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("The local attachment field is invalid: {field}."))
}

fn integer_argument(
    arguments: &Map<String, Value>,
    field: &str,
    default: i64,
) -> Result<i64, String> {
    match arguments.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_i64()
            .ok_or_else(|| format!("The local attachment field is invalid: {field}.")),
    }
}

fn safe_error(error: String) -> String {
    let lowered = error.to_ascii_lowercase();
    if [
        "authorization",
        "api_key",
        "apikey",
        "access_token",
        "password",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        "Local attachment access failed; sensitive details were hidden.".into()
    } else {
        error.chars().take(1_000).collect()
    }
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
            "description": "列出当前团队共享资产。asset_ref 仅对当前 Run 有效，必须原样传给 team_asset_get。",
            "parameters": {
                "type": "object",
                "properties": {
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
            "description": "仅供当前团队的项目经理 Agent 创建共享资产。",
            "parameters": {
                "type": "object",
                "properties": {
                    "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                    "title": {"type": "string", "minLength": 1, "maxLength": 240},
                    "markdown": {"type": "string", "maxLength": 128000}
                },
                "required": ["category", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_update",
            "description": "仅供当前团队的项目经理 Agent 使用当前 Run 的 asset_ref 和 revision 更新共享资产。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160},
                    "expected_revision": {"type": "integer", "minimum": 1},
                    "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                    "title": {"type": "string", "minLength": 1, "maxLength": 240},
                    "markdown": {"type": "string", "maxLength": 128000}
                },
                "required": ["asset_ref", "expected_revision", "category", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_archive",
            "description": "仅供当前团队的项目经理 Agent 使用当前 Run 的 asset_ref 和 revision 归档共享资产。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160},
                    "expected_revision": {"type": "integer", "minimum": 1}
                },
                "required": ["asset_ref", "expected_revision"],
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
                    "builtin_capabilities": {"type": "array", "items": {"type": "string", "enum": ["project_read", "project_write", "terminal"]}, "maxItems": 3, "uniqueItems": true},
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
                    "summary": {"type": "string", "minLength": 1, "maxLength": 16000}
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
        assert_eq!(schemas.len(), 5);
        assert!(schemas
            .iter()
            .all(|schema| !schema.to_string().contains("asset_id")));
        assert!(schemas
            .iter()
            .filter(|schema| schema["name"] != "team_asset_create")
            .all(|schema| schema.to_string().contains("asset_ref")));
    }

    #[test]
    fn todo_execution_catalog_requires_program_owned_completion_tools() {
        let names = todo_execution_tool_schemas()
            .into_iter()
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
