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
    io::Cursor,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
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
        team_todo::{AgentTodoExecutionContract, TodoPlanningCreateInput, TodoPlanningUpdateInput},
        AgentAssetToolBinding, AgentAssetToolRecord, AgentConversationStore,
    },
    conversation_attachments::AttachmentManager,
    host_lifecycle::HostManager,
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
const TODO_PLANNING_WRITE_TOOL_NAMES: [&str; 3] = ["todo_add", "todo_update", "todo_reorder"];
const WORKER_ID: &str = "windows-attachment-tool-worker";
const LEASE_DURATION_MILLISECONDS: u64 = 30_000;
const MAXIMUM_CONTENT_READ_BYTES: usize = 2 * 1_024;
const MAXIMUM_MODEL_IMAGE_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_SOURCE_PIXELS: u64 = 40_000_000;

pub struct AttachmentToolWorker {
    generation: Arc<AtomicU64>,
    wake: Arc<Notify>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Default for AttachmentToolWorker {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            wake: Arc::new(Notify::new()),
            task: Mutex::new(None),
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
    let mut idle_delay = Duration::from_millis(250);
    while generation.load(Ordering::SeqCst) == expected_generation {
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
                    .chain(TODO_PLANNING_READ_TOOL_NAMES)
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
            } else if TODO_EXECUTION_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_todo_execution_tool(app, owner_user_id, &claim).await
            } else if TODO_PLANNING_READ_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
                execute_todo_planning_read_tool(app, owner_user_id, &claim).await
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
    let result = host
        .execute(HostCommand::CommitTool(CommitToolCommand {
            owner_user_id: owner_user_id.to_owned(),
            invocation_id: claim.invocation.invocation_id.clone(),
            claim_token: claim.claim_token,
            expected_version: claim.invocation.version,
            outcome,
        }))
        .await?;
    if matches!(result, HostResult::ToolCommit { .. }) {
        Ok(true)
    } else {
        Err("Local Agent Host returned an unexpected tool commit result".into())
    }
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
                json!({
                    "teams": [snapshot["team"].clone()],
                    "builtin_capabilities": snapshot["builtin_capabilities"].clone(),
                    "plugins": snapshot["plugins"].clone(),
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
                    "builtin_capabilities": {"type": "array", "items": {"type": "string", "enum": ["project_read", "project_write", "terminal"]}, "maxItems": 3, "uniqueItems": true}
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
        assert!(!encoded.contains("todo_id"));
        assert!(!encoded.contains("team_id"));
        assert!(!encoded.contains("agent_id"));
        assert!(!encoded.contains("completed\""));
        assert!(!encoded.contains("blocked\""));
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
