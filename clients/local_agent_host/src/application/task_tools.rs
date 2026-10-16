// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_model_policy::resolve_task_model,
    task_tool_definitions::*,
    task_tool_source_context::{
        source_attachments, source_conversation_context, source_user_message,
    },
    task_tool_support::*,
};
use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, HostCommand, HostRequestEnvelope, LocalAgentRunRecord,
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalTaskDependency,
    LocalTaskGraphNotificationPolicy, LocalTaskRecord, LocalTaskSpec, LocalTaskStatus,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
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
                task_for_agent_tool(&task, &graph.dependencies)
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
                *side_effects_started = true;
                let graph = if let Some(existing) = existing {
                    if existing.graph_id == command.graph_id {
                        self.create_graph(&invocation.invocation_id, command)
                            .await?
                    } else {
                        existing
                    }
                } else {
                    self.create_graph(&invocation.invocation_id, command)
                        .await?
                };
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
                *side_effects_started = true;
                let (graph, reused) = if let Some(existing) = existing {
                    if existing.graph_id == plan.command.graph_id {
                        (
                            self.create_graph(&invocation.invocation_id, plan.command.clone())
                                .await?,
                            false,
                        )
                    } else {
                        (existing, true)
                    }
                } else {
                    (
                        self.create_graph(&invocation.invocation_id, plan.command.clone())
                            .await?,
                        false,
                    )
                };
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
                    "message": "The local task system accepted the arranged tasks for background execution.",
                    "message_zh": "本地任务系统已接收安排好的任务，并会在完成后回写当前会话。"
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
            tool_name => return Err(format!("unsupported local Task tool: {tool_name}")),
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequiredTaskPolicy {
    #[serde(default)]
    max_iterations: Option<u32>,
    #[serde(default)]
    enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    plugin_components: Vec<TaskPluginHint>,
    #[serde(default)]
    plugin_release_snapshots: Option<Vec<TaskPluginReleaseSnapshot>>,
    #[serde(default)]
    allowed_model_config_ids: Option<Vec<String>>,
    #[serde(skip)]
    explicit_plugin_selection: Option<(String, String)>,
}

impl RequiredTaskPolicy {
    fn max_iterations(&self, fallback: u32) -> u32 {
        self.max_iterations.unwrap_or(fallback).max(1)
    }

    fn apply_single(&self, args: &mut CreateTaskArgs) -> Result<(), String> {
        self.validate_explicit_plugin_hints(&args.plugin_hints)?;
        extend_unique(&mut args.enabled_builtin_kinds, &self.enabled_builtin_kinds);
        extend_unique(
            &mut args.external_mcp_config_ids,
            &self.external_mcp_config_ids,
        );
        extend_required_plugin_hints(&mut args.plugin_hints, &self.plugin_components);
        args.plugin_release_snapshots = selected_plugin_release_snapshots(
            &args.plugin_hints,
            self.plugin_release_snapshots.as_deref(),
        )?;
        Ok(())
    }

    fn apply_batch(&self, args: &mut CreateTasksArgs) -> Result<(), String> {
        for task in &mut args.tasks {
            self.validate_explicit_plugin_hints(&task.plugin_hints)?;
            extend_unique(&mut task.enabled_builtin_kinds, &self.enabled_builtin_kinds);
            extend_unique(
                &mut task.external_mcp_config_ids,
                &self.external_mcp_config_ids,
            );
            extend_required_plugin_hints(&mut task.plugin_hints, &self.plugin_components);
            task.plugin_release_snapshots = selected_plugin_release_snapshots(
                &task.plugin_hints,
                self.plugin_release_snapshots.as_deref(),
            )?;
        }
        Ok(())
    }

    pub(super) fn bind_explicit_selection(
        &mut self,
        parent: &LocalAgentRunRecord,
    ) -> Result<(), String> {
        let Some(selection) = parent.input.get("explicit_capability_selection") else {
            return Ok(());
        };
        match selection.get("kind").and_then(Value::as_str) {
            Some("plugin") => {
                let plugin_key = selection
                    .get("plugin_key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or("explicit Plugin selection is missing plugin_key")?
                    .to_string();
                let component_key = selection
                    .get("component_key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or("explicit Plugin selection is missing component_key")?
                    .to_string();
                let frozen = self
                    .plugin_release_snapshots
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|snapshot| snapshot.plugin_key == plugin_key && snapshot.component_key == component_key);
                if !frozen {
                    return Err(format!(
                        "the Plugin component selected in the composer is not available in the current frozen catalog: {plugin_key}/{component_key}"
                    ));
                }
                extend_required_plugin_hints(&mut self.plugin_components, &[TaskPluginHint {
                    plugin_key: plugin_key.clone(),
                    component_key: component_key.clone(),
                    reason: "selected explicitly in Main Chat".to_string(),
                }]);
                self.explicit_plugin_selection = Some((plugin_key, component_key));
                Ok(())
            }
            Some("requirement_survey") => Err(
                "Requirement Survey was selected in the composer; call requirement_survey_create directly instead of creating a Task"
                    .to_string(),
            ),
            Some(kind) => Err(format!(
                "unsupported explicit capability selection for Task creation: {kind}"
            )),
            None => Err("explicit capability selection is missing kind".to_string()),
        }
    }

    fn validate_explicit_plugin_hints(&self, hints: &[TaskPluginHint]) -> Result<(), String> {
        let Some((selected_plugin, selected_component)) = self.explicit_plugin_selection.as_ref() else {
            return Ok(());
        };
        let allowed = self
            .plugin_components
            .iter()
            .map(|hint| (hint.plugin_key.trim(), hint.component_key.trim()))
            .collect::<HashSet<_>>();
        if let Some(unexpected) = hints
            .iter()
            .map(|hint| (hint.plugin_key.trim(), hint.component_key.trim()))
            .find(|pair| !allowed.contains(pair))
        {
            return Err(format!(
                "Task Plugin {}/{} is outside the user's explicit composer selection {}/{}",
                unexpected.0, unexpected.1, selected_plugin, selected_component
            ));
        }
        Ok(())
    }
}

fn extend_unique(target: &mut Vec<String>, required: &[String]) {
    let mut seen = target
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<HashSet<_>>();
    for value in required {
        let value = value.trim();
        if !value.is_empty() && seen.insert(value.to_string()) {
            target.push(value.to_string());
        }
    }
}

fn extend_required_plugin_hints(target: &mut Vec<TaskPluginHint>, required: &[TaskPluginHint]) {
    let mut seen = target
        .iter()
        .map(|hint| (hint.plugin_key.trim().to_string(), hint.component_key.trim().to_string()))
        .collect::<HashSet<_>>();
    for required_hint in required {
        let key = required_hint.plugin_key.trim();
        let component = required_hint.component_key.trim();
        if !key.is_empty() && !component.is_empty()
            && seen.insert((key.to_string(), component.to_string())) {
            target.push(required_hint.clone());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateTaskItem {
    pub(super) client_ref: String,
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
    notification_milestone: bool,
    #[serde(default)]
    pub(super) prerequisite_refs: Vec<String>,
    #[serde(default)]
    pub(super) context_refs: Vec<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
    #[serde(default)]
    schedule: Option<TaskScheduleArgs>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskScheduleArgs {
    #[serde(default)]
    pub(super) mode: Option<String>,
    #[serde(default)]
    pub(super) run_at: Option<String>,
    #[serde(default)]
    pub(super) interval_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskPluginHint {
    pub(super) plugin_key: String,
    pub(super) component_key: String,
    #[serde(default)]
    pub(super) reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskPluginReleaseSnapshot {
    pub(super) plugin_key: String,
    pub(super) component_key: String,
    pub(super) plugin_id: String,
    pub(super) release_id: String,
    pub(super) version: String,
    pub(super) artifact_sha256: String,
}

fn selected_plugin_release_snapshots(
    hints: &[TaskPluginHint],
    catalog: Option<&[TaskPluginReleaseSnapshot]>,
) -> Result<Option<Vec<TaskPluginReleaseSnapshot>>, String> {
    if hints.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let catalog = catalog.ok_or("Task capability policy must include plugin_release_snapshots")?;
    let mut by_key = HashMap::new();
    for snapshot in catalog {
        for (field, value) in [
            ("plugin_key", snapshot.plugin_key.as_str()),
            ("component_key", snapshot.component_key.as_str()),
            ("plugin_id", snapshot.plugin_id.as_str()),
            ("release_id", snapshot.release_id.as_str()),
            ("version", snapshot.version.as_str()),
            ("artifact_sha256", snapshot.artifact_sha256.as_str()),
        ] {
            validate_nonempty_text(value, field)?;
        }
        if by_key
            .insert((snapshot.plugin_key.as_str(), snapshot.component_key.as_str()), snapshot)
            .is_some()
        {
            return Err(format!(
                "required Task policy contains duplicate Plugin component: {}/{}",
                snapshot.plugin_key, snapshot.component_key
            ));
        }
    }
    let mut selected = Vec::with_capacity(hints.len());
    for hint in hints {
        let key = hint.plugin_key.trim();
        let component = hint.component_key.trim();
        let snapshot = by_key.get(&(key, component)).ok_or_else(|| {
            format!("Plugin component is not frozen in the current capability policy: {key}/{component}")
        })?;
        selected.push((*snapshot).clone());
    }
    selected.sort_by(|left, right| (&left.plugin_key, &left.component_key).cmp(&(&right.plugin_key, &right.component_key)));
    Ok(Some(selected))
}

fn validate_optional_notepad_selection(
    enabled_builtin_kinds: &[String],
    request_quote: Option<&str>,
    source_user_message: Option<&str>,
) -> Result<(), String> {
    let selects_notepad = enabled_builtin_kinds
        .iter()
        .any(|kind| kind.trim() == "Notepad");
    let quote = request_quote
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if !selects_notepad {
        return if quote.is_none() {
            Ok(())
        } else {
            Err(
                "notepad_user_request_quote requires enabled_builtin_kinds to contain Notepad"
                    .to_string(),
            )
        };
    }
    let quote = quote.ok_or_else(|| {
        "Selecting optional Notepad requires notepad_user_request_quote copied exactly from the current user message".to_string()
    })?;
    if quote.chars().count() < 2 || quote.chars().count() > 500 {
        return Err("notepad_user_request_quote must contain 2..=500 characters".to_string());
    }
    let source = source_user_message.ok_or_else(|| {
        "Selecting optional Notepad requires an available current user message".to_string()
    })?;
    if !source.contains(quote) {
        return Err(
            "notepad_user_request_quote must be an exact substring of the current user message"
                .to_string(),
        );
    }
    Ok(())
}

fn create_single_graph(
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    args: CreateTaskArgs,
    model: (String, String),
    max_iterations: u32,
) -> Result<CreateTaskGraphCommand, String> {
    let enabled_builtin_kinds =
        validated_builtin_kinds(args.requires_execution, args.enabled_builtin_kinds)?;
    let external_mcp_config_ids = validate_external_mcp_ids(&args.external_mcp_config_ids)?;
    validate_plugin_hints(&args.plugin_hints)?;
    let prompt = task_prompt(
        &args.objective,
        &args.description,
        &args.expected_outputs,
        &args.acceptance_criteria,
        &args.input_payload,
    )?;
    let schedule = contact_async_schedule(args.schedule)?;
    let runtime_settings = task_runtime_settings(args.thinking_level.as_deref())?;
    let graph_id = format!("local-task-graph-{}", invocation.invocation_id);
    let source_context = source_conversation_context(parent);
    let source_user_message = source_user_message(parent);
    let source_attachments = source_attachments(parent)?;
    Ok(CreateTaskGraphCommand {
        graph_id,
        owner_user_id: parent.owner_user_id.clone(),
        source_entity_type: parent.owner_entity_type.clone(),
        source_entity_id: parent.owner_entity_id.clone(),
        tasks: vec![LocalTaskSpec {
            task_id: format!("local-task-{}-1", invocation.invocation_id),
            title: args.title,
            profile_key: "task_execution".to_string(),
            model_config_ref: model.0,
            model_config_revision: model.1,
            capability_policy_revision: parent.capability_policy_revision.clone(),
            input: json!({
                "prompt": prompt,
                "objective": args.objective,
                "expected_outputs": args.expected_outputs,
                "acceptance_criteria": args.acceptance_criteria,
                "acceptance_report_version": 1,
                "final_response_max_chars": args.final_response_max_chars,
                "description": args.description,
                "input_payload": args.input_payload,
                "priority": args.priority,
                "tags": args.tags,
                "owned_paths": args.owned_paths,
                "tool_options": {
                    "requires_execution": args.requires_execution,
                    "enabled_builtin_kinds": enabled_builtin_kinds,
                    "external_mcp_config_ids": external_mcp_config_ids,
                    "plugin_hints": args.plugin_hints,
                    "plugin_release_snapshots": args.plugin_release_snapshots,
                },
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone(),
                "source_user_message": source_user_message,
                "remote_connection_id": source_context.remote_connection_id.clone(),
                "source_run_id": parent.run_id,
                "runtime_settings": runtime_settings,
                "attachments": source_attachments,
                "prerequisite_task_ids": args.prerequisite_task_ids,
                "supersedes_task_ids": args.supersedes_task_ids,
                "followup_message_id": parent.input.pointer("/task_followup_review/review_message_id"),
                "schedule": schedule
            }),
            max_iterations,
        }],
        dependencies: Vec::new(),
    })
}

async fn create_batch_graph(
    runtime: &LocalAgentRuntime,
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    args: CreateTasksArgs,
    max_iterations: u32,
    allowed_model_config_ids: Option<&[String]>,
) -> Result<CreateBatchPlan, String> {
    if args.tasks.len() < 2 || args.tasks.len() > 50 {
        return Err("tasks must contain 2..=50 items".to_string());
    }
    validate_nonempty_text(&args.graph_goal, "graph_goal")?;
    validate_nonempty_text(&args.decomposition_rationale, "decomposition_rationale")?;
    let mut items = args.tasks;
    let mut ref_to_id = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        let client_ref = item.client_ref.trim();
        if client_ref.is_empty() {
            return Err("client_ref cannot be empty".to_string());
        }
        let task_id = format!("local-task-{}-{}", invocation.invocation_id, index + 1);
        if ref_to_id.insert(client_ref.to_string(), task_id).is_some() {
            return Err(format!("client_ref is duplicated: {client_ref}"));
        }
    }
    for item in &items {
        let client_ref = item.client_ref.trim();
        for context_ref in &item.context_refs {
            let context_ref = context_ref.trim();
            if !ref_to_id.contains_key(context_ref) {
                return Err(format!("unknown context_ref: {context_ref}"));
            }
            if context_ref == client_ref {
                return Err(format!("task cannot use itself as context: {context_ref}"));
            }
        }
    }
    if let Some(final_ref) = args.final_delivery_ref.as_deref() {
        if !ref_to_id.contains_key(final_ref.trim()) {
            return Err(format!("unknown final_delivery_ref: {final_ref}"));
        }
    }
    let diagnostics = reduce_client_ref_dependencies(&mut items)?;
    let bindings = items
        .iter()
        .map(|item| CreatedTaskBinding {
            client_ref: item.client_ref.trim().to_string(),
            task_id: ref_to_id
                .get(item.client_ref.trim())
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    let source_context = source_conversation_context(parent);
    let source_user_message = source_user_message(parent);
    let source_attachments = source_attachments(parent)?;
    let mut tasks = Vec::with_capacity(items.len());
    let mut dependencies = Vec::new();
    for item in items {
        let model = resolve_task_model(
            runtime,
            parent,
            item.default_model_config_id.as_deref(),
            allowed_model_config_ids,
        )
        .await?;
        let enabled_builtin_kinds =
            validated_builtin_kinds(item.requires_execution, item.enabled_builtin_kinds)?;
        let external_mcp_config_ids = validate_external_mcp_ids(&item.external_mcp_config_ids)?;
        validate_plugin_hints(&item.plugin_hints)?;
        let input_payload = attach_dependency_context_payload(
            item.input_payload,
            item.client_ref.trim(),
            &item.context_refs,
        );
        let prompt = task_prompt(
            &item.objective,
            &item.description,
            &item.expected_outputs,
            &item.acceptance_criteria,
            &input_payload,
        )?;
        let schedule = contact_async_schedule(item.schedule)?;
        let runtime_settings = task_runtime_settings(item.thinking_level.as_deref())?;
        let task_id = ref_to_id
            .get(item.client_ref.trim())
            .cloned()
            .ok_or_else(|| format!("unknown client_ref: {}", item.client_ref))?;
        for prerequisite_ref in &item.prerequisite_refs {
            let prerequisite_task_id = ref_to_id
                .get(prerequisite_ref.trim())
                .cloned()
                .ok_or_else(|| format!("unknown prerequisite_ref: {prerequisite_ref}"))?;
            dependencies.push(LocalTaskDependency {
                task_id: task_id.clone(),
                prerequisite_task_id,
            });
        }
        tasks.push(LocalTaskSpec {
            task_id,
            title: item.title,
            profile_key: "task_execution".to_string(),
            model_config_ref: model.0,
            model_config_revision: model.1,
            capability_policy_revision: parent.capability_policy_revision.clone(),
            input: json!({
                "prompt": prompt,
                "objective": item.objective,
                "expected_outputs": item.expected_outputs,
                "acceptance_criteria": item.acceptance_criteria,
                "acceptance_report_version": 1,
                "final_response_max_chars": item.final_response_max_chars,
                "graph_goal": args.graph_goal,
                "decomposition_rationale": args.decomposition_rationale,
                "graph_notification_policy": args.notification_policy.as_str(),
                "notification_milestone": item.notification_milestone,
                "final_delivery": args.final_delivery_ref.as_deref().is_some_and(|value| value.trim() == item.client_ref.trim()),
                "description": item.description,
                "input_payload": input_payload,
                "client_ref": item.client_ref,
                "priority": item.priority,
                "tags": item.tags,
                "owned_paths": item.owned_paths,
                "tool_options": {
                    "requires_execution": item.requires_execution,
                    "enabled_builtin_kinds": enabled_builtin_kinds,
                    "external_mcp_config_ids": external_mcp_config_ids,
                    "plugin_hints": item.plugin_hints,
                    "plugin_release_snapshots": item.plugin_release_snapshots,
                },
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone(),
                "source_user_message": source_user_message.clone(),
                "remote_connection_id": source_context.remote_connection_id.clone(),
                "source_run_id": parent.run_id,
                "runtime_settings": runtime_settings,
                "attachments": source_attachments.clone(),
                "prerequisite_task_ids": item.prerequisite_task_ids,
                "supersedes_task_ids": args.supersedes_task_ids,
                "followup_message_id": parent.input.pointer("/task_followup_review/review_message_id"),
                "schedule": schedule
            }),
            max_iterations,
        });
    }
    Ok(CreateBatchPlan {
        command: CreateTaskGraphCommand {
            graph_id: format!("local-task-graph-{}", invocation.invocation_id),
            owner_user_id: parent.owner_user_id.clone(),
            source_entity_type: parent.owner_entity_type.clone(),
            source_entity_id: parent.owner_entity_id.clone(),
            tasks,
            dependencies,
        },
        bindings,
        diagnostics,
    })
}

struct CreateBatchPlan {
    command: CreateTaskGraphCommand,
    bindings: Vec<CreatedTaskBinding>,
    diagnostics: DependencyReduction,
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
