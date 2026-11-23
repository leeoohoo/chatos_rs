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

mod arguments;
mod asset_context;
mod asset_schemas;
mod attachment_read;
mod capability_broker;
mod planning_schemas;
mod project_dashboard;
mod team_asset;
mod team_chat;
mod team_chat_schemas;
mod todo_execution;
mod todo_planning;

use arguments::{
    invalid_todo_planning_arguments, optional_bounded_string, optional_planning_integer,
    optional_planning_text, planning_string_array, required_bounded_string,
};
use asset_context::{
    asset_reference, asset_summary, invalid_asset_arguments, team_asset_review_output,
    team_asset_review_reference, validate_asset_run_context, validate_communication_run_context,
};
pub(crate) use asset_schemas::{
    attachment_read_tool_schema, project_dashboard_tool_schemas, team_asset_tool_schemas,
};
pub(crate) use attachment_read::normalize_verified_image;
#[cfg(test)]
use attachment_read::{content_output, model_image_output, MAXIMUM_CONTENT_READ_BYTES};
use attachment_read::{execute_attachment_read, safe_error};
use capability_broker::execute_capability_broker_tool;
pub(crate) use planning_schemas::{
    todo_execution_tool_schemas, todo_planning_read_tool_schemas, todo_planning_write_tool_schemas,
};
use project_dashboard::execute_project_dashboard_tool;
#[cfg(test)]
use team_asset::require_asset_manager_lane;
use team_asset::{execute_team_asset_tool, now_unix_ms};
use team_chat::{execute_team_chat_read_tool, execute_team_chat_write_tool};
pub(crate) use team_chat::{schedule_agent_communication_drain, schedule_team_todo_drain};
pub(crate) use team_chat_schemas::{team_chat_read_tool_schemas, team_chat_write_tool_schemas};
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
