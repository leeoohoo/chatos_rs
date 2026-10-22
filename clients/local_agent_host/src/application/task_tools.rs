// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "task_creation.rs"]
mod task_creation;

use self::task_creation::{
    task_graph_builder::{create_batch_graph, create_single_graph},
    validate_optional_notepad_selection, TaskPluginReleaseSnapshot,
};
pub(super) use self::task_creation::{
    CreateTaskItem, RequiredTaskPolicy, TaskPluginHint, TaskScheduleArgs,
};
use super::{
    task_model_policy::resolve_task_model,
    task_tool_definitions::*,
    task_tool_source_context::{source_conversation_context, source_user_message},
    task_tool_support::*,
};
use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    HostCommand, HostRequestEnvelope, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, LocalTaskGraphNotificationPolicy, LocalTaskRecord, LocalTaskStatus,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

#[derive(Clone)]
pub struct LocalTaskToolExecutor {
    pub(super) runtime: Arc<LocalAgentRuntime>,
    pub(super) owner_user_id: String,
}

impl LocalTaskToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Task tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    async fn replacement_chain(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        requested: &LocalTaskRecord,
    ) -> Result<Vec<LocalTaskRecord>, String> {
        const MAX_REPLACEMENT_TASKS: usize = 64;
        let mut pending = VecDeque::from(requested.replacement_task_ids.clone());
        let mut visited = HashSet::from([requested.task_id.clone()]);
        let mut chain = Vec::new();
        while let Some(task_id) = pending.pop_front() {
            if !visited.insert(task_id.clone()) {
                continue;
            }
            if visited.len() > MAX_REPLACEMENT_TASKS {
                return Err("Task replacement chain exceeds the 64 Task safety limit".to_string());
            }
            let task = self
                .runtime
                .get_task_for_conversation(owner_user_id, conversation_id, &task_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("replacement Task not found: {task_id}"))?;
            pending.extend(task.replacement_task_ids.iter().cloned());
            chain.push(task);
        }
        Ok(chain)
    }
}

#[async_trait]
impl LocalToolExecutor for LocalTaskToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        let parent = self.parent_run(&invocation.run_id).await?;
        if parent.owner_user_id != self.owner_user_id || parent.profile_key != "main_chat" {
            return Err("Task tools are available only to the active local Main Chat".to_string());
        }
        let source_context = source_conversation_context(&parent);
        let conversation_id = source_context
            .conversation_id
            .as_deref()
            .ok_or_else(|| "Task tools require an active conversation context".to_string())?;
        // Only errors before the first mutation are known to have no effects. Never infer
        // this from error text: dispatch can fail after graph creation has committed.
        let mut side_effects_started = false;
        let mut phase = "task_preparation";
        let output = self
            .execute_plan(
                invocation,
                parent,
                conversation_id,
                &mut side_effects_started,
                &mut phase,
            )
            .await;
        match output {
            Ok(output) => Ok(LocalAgentToolOutcome::Succeeded { output }),
            Err(error) if !side_effects_started => Ok(LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": phase, "side_effects_started": false}),
            }),
            Err(error) => Err(error),
        }
    }
}

impl LocalTaskToolExecutor {
    async fn execute_plan(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
        parent: LocalAgentRunRecord,
        conversation_id: &str,
        side_effects_started: &mut bool,
        phase: &mut &'static str,
    ) -> Result<Value, String> {
        let output = match invocation.tool_name.as_str() {
            RESOLVE_TASK_FOLLOWUP_TOOL => {
                *phase = "task_followup_review";
                super::task_followup_tool::resolve(self, invocation, &parent, side_effects_started)
                    .await?
            }
            LIST_TASKS_TOOL => {
                let args: ListTasksArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid list_tasks input: {error}"))?;
                let limit = args.limit.unwrap_or(50);
                let offset = args.offset.unwrap_or(0);
                let (storage_status, post_filter) = match args.status {
                    None => (None, None),
                    Some(TaskListStatus::Ready) => (None, Some(TaskListStatus::Ready)),
                    Some(TaskListStatus::Running) => (Some(LocalTaskStatus::Running), None),
                    Some(TaskListStatus::Succeeded) => (Some(LocalTaskStatus::Succeeded), None),
                    Some(TaskListStatus::Failed) => (Some(LocalTaskStatus::Failed), None),
                    Some(TaskListStatus::Blocked) => (Some(LocalTaskStatus::Blocked), None),
                    Some(TaskListStatus::Cancelled) => (Some(LocalTaskStatus::Cancelled), None),
                    Some(
                        status @ (TaskListStatus::Draft
                        | TaskListStatus::Queued
                        | TaskListStatus::Archived),
                    ) => (None, Some(status)),
                };
                let query_limit = if post_filter.is_some() { 500 } else { limit };
                let query_offset = if post_filter.is_some() { 0 } else { offset };
                let mut tasks = self
                    .runtime
                    .list_tasks_for_conversation(
                        &parent.owner_user_id,
                        conversation_id,
                        storage_status,
                        args.keyword.as_deref(),
                        args.tag.as_deref(),
                        args.scheduled_only,
                        args.parent_task_id.as_deref(),
                        args.source_run_id.as_deref(),
                        query_limit,
                        query_offset,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                if let Some(status) = post_filter {
                    tasks.retain(|task| status.matches(task));
                    tasks = tasks
                        .into_iter()
                        .skip(offset as usize)
                        .take(limit as usize)
                        .collect();
                }
                tasks_for_agent_tool(&tasks)
            }
            GET_TASK_TOOL => {
                let args: TaskIdArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid get_task input: {error}"))?;
                let task = self
                    .runtime
                    .get_task_for_conversation(
                        &parent.owner_user_id,
                        conversation_id,
                        &args.task_id,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("task not found: {}", args.task_id))?;
                let graph = self
                    .runtime
                    .task_graph_by_id(&parent.owner_user_id, &task.graph_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("Task Graph not found: {}", task.graph_id))?;
                let requested_task = task_for_agent_tool(&task, &graph.dependencies);
                let replacement_chain = self
                    .replacement_chain(&parent.owner_user_id, conversation_id, &task)
                    .await?;
                let current_tasks = if replacement_chain.is_empty() {
                    vec![requested_task.clone()]
                } else {
                    replacement_chain
                        .iter()
                        .filter(|candidate| candidate.replacement_task_ids.is_empty())
                        .map(|candidate| task_for_agent_tool(candidate, &[]))
                        .collect::<Vec<_>>()
                };
                let chain_values = replacement_chain
                    .iter()
                    .map(|candidate| task_for_agent_tool(candidate, &[]))
                    .collect::<Vec<_>>();
                let mut output = requested_task.clone();
                if let Some(object) = output.as_object_mut() {
                    object.insert("requested_task".to_string(), requested_task);
                    object.insert(
                        "is_superseded".to_string(),
                        Value::Bool(!task.replacement_task_ids.is_empty()),
                    );
                    object.insert("replacement_chain".to_string(), Value::Array(chain_values));
                    object.insert("current_tasks".to_string(), Value::Array(current_tasks));
                    object.insert(
                        "current_status_guidance".to_string(),
                        Value::String(
                            "Use current_tasks, not the requested historical Task, as the current status."
                                .to_string(),
                        ),
                    );
                }
                output
            }
            CREATE_TASK_TOOL => {
                let mut args: CreateTaskArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid create_task input: {error}"))?;
                validate_optional_notepad_selection(
                    &args.enabled_builtin_kinds,
                    args.notepad_user_request_quote.as_deref(),
                    source_user_message(&parent).as_deref(),
                )?;
                let existing = self
                    .reusable_source_graph(&parent, conversation_id, &args.supersedes_task_ids)
                    .await?;
                let task_policy = self.required_task_policy(&parent).await?;
                task_policy.apply_single(&mut args)?;
                self.validate_existing_prerequisites(
                    &parent,
                    conversation_id,
                    &args.prerequisite_task_ids,
                )
                .await?;
                let model = resolve_task_model(
                    self.runtime.as_ref(),
                    &parent,
                    args.default_model_config_id.as_deref(),
                    task_policy.allowed_model_config_ids.as_deref(),
                )
                .await?;
                let command = create_single_graph(
                    invocation,
                    &parent,
                    args,
                    model,
                    task_policy.max_iterations(parent.max_iterations),
                )?;
                *phase = "task_graph_validation";
                command.validate()?;
                let graph = if let Some(existing) = existing {
                    if existing.graph_id == command.graph_id {
                        self.create_graph(&invocation.invocation_id, command, side_effects_started)
                            .await?
                    } else {
                        existing
                    }
                } else {
                    self.create_graph(&invocation.invocation_id, command, side_effects_started)
                        .await?
                };
                *side_effects_started = true;
                self.runtime
                    .start_ready_task_runs_for_graph(&parent.owner_user_id, &graph.graph_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let graph = self
                    .runtime
                    .task_graph_by_id(&parent.owner_user_id, &graph.graph_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        format!("Task Graph not found after dispatch: {}", graph.graph_id)
                    })?;
                let task = graph
                    .tasks
                    .first()
                    .ok_or_else(|| "created Task Graph contains no Tasks".to_string())?;
                task_for_agent_tool(task, &graph.dependencies)
            }
            CREATE_TASKS_TOOL => {
                let mut args: CreateTasksArgs =
                    serde_json::from_value(invocation.arguments.clone()).map_err(|error| {
                        format!("invalid create_tasks_with_prerequisites input: {error}")
                    })?;
                let source_message = source_user_message(&parent);
                for task in &args.tasks {
                    validate_optional_notepad_selection(
                        &task.enabled_builtin_kinds,
                        task.notepad_user_request_quote.as_deref(),
                        source_message.as_deref(),
                    )?;
                }
                let existing = self
                    .reusable_source_graph(&parent, conversation_id, &args.supersedes_task_ids)
                    .await?;
                let task_policy = self.required_task_policy(&parent).await?;
                task_policy.apply_batch(&mut args)?;
                for task in &args.tasks {
                    self.validate_existing_prerequisites(
                        &parent,
                        conversation_id,
                        &task.prerequisite_task_ids,
                    )
                    .await?;
                }
                let plan = create_batch_graph(
                    self.runtime.as_ref(),
                    invocation,
                    &parent,
                    args,
                    task_policy.max_iterations(parent.max_iterations),
                    task_policy.allowed_model_config_ids.as_deref(),
                )
                .await?;
                *phase = "task_graph_validation";
                plan.command.validate()?;
                let (graph, reused) = if let Some(existing) = existing {
                    if existing.graph_id == plan.command.graph_id {
                        (
                            self.create_graph(
                                &invocation.invocation_id,
                                plan.command.clone(),
                                side_effects_started,
                            )
                            .await?,
                            false,
                        )
                    } else {
                        (existing, true)
                    }
                } else {
                    (
                        self.create_graph(
                            &invocation.invocation_id,
                            plan.command.clone(),
                            side_effects_started,
                        )
                        .await?,
                        false,
                    )
                };
                *side_effects_started = true;
                let auto_started_runs = self
                    .runtime
                    .start_ready_task_runs_for_graph(&parent.owner_user_id, &graph.graph_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let graph = self
                    .runtime
                    .task_graph_by_id(&parent.owner_user_id, &graph.graph_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        format!("Task Graph not found after dispatch: {}", graph.graph_id)
                    })?;
                batch_creation_value(
                    &graph,
                    &plan.bindings,
                    (!reused).then_some(&plan.diagnostics),
                    reused,
                    &auto_started_runs,
                )
            }
            CANCEL_TASK_TOOL => {
                let args: CancelTaskArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid cancel_task input: {error}"))?;
                chatos_local_agent_protocol::CancelTaskCommand {
                    owner_user_id: parent.owner_user_id.clone(),
                    task_id: args.task_id.clone(),
                    expected_version: args.expected_version,
                    reason: args.reason.clone(),
                    replacement_task_ids: args.replacement_task_ids.clone(),
                }
                .validate()?;
                let task = self
                    .runtime
                    .get_task_for_conversation(
                        &parent.owner_user_id,
                        conversation_id,
                        &args.task_id,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("task not found: {}", args.task_id))?;
                for replacement_task_id in &args.replacement_task_ids {
                    if replacement_task_id == &args.task_id {
                        return Err("a cancelled task cannot replace itself".to_string());
                    }
                    let replacement = self
                        .runtime
                        .get_task_for_conversation(
                            &parent.owner_user_id,
                            conversation_id,
                            replacement_task_id,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    if replacement.is_none() {
                        return Err(format!("replacement task not found: {replacement_task_id}"));
                    }
                }
                let task_id = args.task_id.clone();
                let reason = args.reason.clone();
                *side_effects_started = true;
                let graph = self.cancel_task(invocation, args).await?;
                cancellation_value(&graph, &task_id, &reason, task.active_run_id.as_deref())?
            }
            WAIT_FOR_TASK_COMPLETION_TOOL => {
                let _: EmptyArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid wait_for_task_completion input: {error}"))?;
                json!({
                    "accepted": true,
                    "mode": "background",
                    "message": "The task system accepted the arranged tasks for background execution.",
                    "message_zh": "任务系统已接收安排好的任务，并会在完成后回写当前会话。"
                })
            }
            GET_TASK_DEPENDENCY_GRAPH_TOOL => {
                let args: TaskIdArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid get_task_dependency_graph input: {error}"))?;
                let task = self
                    .runtime
                    .get_task_for_conversation(
                        &parent.owner_user_id,
                        conversation_id,
                        &args.task_id,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("task not found: {}", args.task_id))?;
                self.dependency_graph_for_task(conversation_id, task)
                    .await?
            }
            tool_name => return Err(format!("unsupported Task tool: {tool_name}")),
        };
        Ok(output)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListTasksArgs {
    #[serde(default)]
    status: Option<TaskListStatus>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    scheduled_only: Option<bool>,
    #[serde(default)]
    parent_task_id: Option<String>,
    #[serde(default)]
    source_run_id: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TaskListStatus {
    Draft,
    Ready,
    Queued,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    Archived,
}

impl TaskListStatus {
    fn matches(self, task: &LocalTaskRecord) -> bool {
        match self {
            Self::Ready => {
                task.status == LocalTaskStatus::Pending
                    || (task.status == LocalTaskStatus::Ready && task.active_run_id.is_none())
            }
            Self::Queued => task.status == LocalTaskStatus::Ready && task.active_run_id.is_some(),
            Self::Running => task.status == LocalTaskStatus::Running,
            Self::Succeeded => task.status == LocalTaskStatus::Succeeded,
            Self::Failed => task.status == LocalTaskStatus::Failed,
            Self::Blocked => task.status == LocalTaskStatus::Blocked,
            Self::Cancelled => task.status == LocalTaskStatus::Cancelled,
            Self::Draft | Self::Archived => false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskIdArgs {
    task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CancelTaskArgs {
    pub(super) task_id: String,
    pub(super) reason: String,
    #[serde(default)]
    pub(super) expected_version: Option<u64>,
    #[serde(default)]
    pub(super) replacement_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTaskArgs {
    title: String,
    objective: String,
    expected_outputs: Vec<String>,
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    final_response_max_chars: Option<u64>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    input_payload: Value,
    #[serde(default)]
    default_model_config_id: Option<String>,
    #[serde(default)]
    thinking_level: Option<String>,
    requires_execution: bool,
    enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    notepad_user_request_quote: Option<String>,
    #[serde(default)]
    external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    plugin_hints: Vec<TaskPluginHint>,
    #[serde(skip)]
    plugin_release_snapshots: Option<Vec<TaskPluginReleaseSnapshot>>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    owned_paths: Vec<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
    #[serde(default)]
    schedule: Option<TaskScheduleArgs>,
    #[serde(default)]
    supersedes_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTasksArgs {
    graph_goal: String,
    decomposition_rationale: String,
    #[serde(default)]
    notification_policy: LocalTaskGraphNotificationPolicy,
    #[serde(default)]
    final_delivery_ref: Option<String>,
    tasks: Vec<CreateTaskItem>,
    #[serde(default)]
    supersedes_task_ids: Vec<String>,
}

pub(super) fn envelope(command_id: String, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id,
        command,
    }
}

#[cfg(test)]
pub(super) fn preparation_error(outcome: LocalAgentToolOutcome) -> String {
    let LocalAgentToolOutcome::Failed { error, detail } = outcome else {
        panic!("expected known failure before mutation, got {outcome:?}");
    };
    assert_eq!(detail["side_effects_started"], false);
    error
}

#[cfg(test)]
#[path = "task_tools_tests.rs"]
mod tests;
