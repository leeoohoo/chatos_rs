// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{task_tool_definitions::*, task_tool_support::*};
use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, HostCommand, HostRequestEnvelope, LocalAgentRunRecord,
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalTaskDependency, LocalTaskRecord,
    LocalTaskSpec, LocalTaskStatus, LOCAL_AGENT_PROTOCOL_VERSION,
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
        let output = match invocation.tool_name.as_str() {
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
                let task_policy = self.required_task_policy(&parent).await?;
                task_policy.apply_single(&mut args);
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
                )
                .await?;
                let command = create_single_graph(
                    invocation,
                    &parent,
                    args,
                    model,
                    task_policy.max_iterations(parent.max_iterations),
                )?;
                let graph = if let Some(existing) = self.reusable_source_graph(&parent).await? {
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
                let task_policy = self.required_task_policy(&parent).await?;
                task_policy.apply_batch(&mut args);
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
                )
                .await?;
                let (graph, reused) =
                    if let Some(existing) = self.reusable_source_graph(&parent).await? {
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
        Ok(LocalAgentToolOutcome::Succeeded { output })
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
struct CreateTaskArgs {
    title: String,
    objective: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    input_payload: Value,
    #[serde(default)]
    default_model_config_id: Option<String>,
    requires_execution: bool,
    enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    plugin_hints: Vec<TaskPluginHint>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
    #[serde(default)]
    schedule: Option<TaskScheduleArgs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateTasksArgs {
    tasks: Vec<CreateTaskItem>,
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
    plugin_keys: Vec<String>,
}

impl RequiredTaskPolicy {
    fn max_iterations(&self, fallback: u32) -> u32 {
        self.max_iterations.unwrap_or(fallback).max(1)
    }

    fn apply_single(&self, args: &mut CreateTaskArgs) {
        extend_unique(&mut args.enabled_builtin_kinds, &self.enabled_builtin_kinds);
        extend_unique(
            &mut args.external_mcp_config_ids,
            &self.external_mcp_config_ids,
        );
        extend_required_plugin_hints(&mut args.plugin_hints, &self.plugin_keys);
    }

    fn apply_batch(&self, args: &mut CreateTasksArgs) {
        for task in &mut args.tasks {
            extend_unique(&mut task.enabled_builtin_kinds, &self.enabled_builtin_kinds);
            extend_unique(
                &mut task.external_mcp_config_ids,
                &self.external_mcp_config_ids,
            );
            extend_required_plugin_hints(&mut task.plugin_hints, &self.plugin_keys);
        }
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

fn extend_required_plugin_hints(target: &mut Vec<TaskPluginHint>, required: &[String]) {
    let mut seen = target
        .iter()
        .map(|hint| hint.plugin_key.trim().to_string())
        .collect::<HashSet<_>>();
    for key in required {
        let key = key.trim();
        if !key.is_empty() && seen.insert(key.to_string()) {
            target.push(TaskPluginHint {
                plugin_key: key.to_string(),
                reason: "required by Local Agent capability policy".to_string(),
            });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CreateTaskItem {
    pub(super) client_ref: String,
    title: String,
    objective: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    input_payload: Value,
    #[serde(default)]
    default_model_config_id: Option<String>,
    requires_execution: bool,
    enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    plugin_hints: Vec<TaskPluginHint>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    owned_paths: Vec<String>,
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
    #[serde(default)]
    pub(super) reason: String,
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
    let prompt = task_prompt(&args.objective, &args.description, &args.input_payload)?;
    let schedule = contact_async_schedule(args.schedule)?;
    let graph_id = format!("local-task-graph-{}", invocation.invocation_id);
    let source_context = source_conversation_context(parent);
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
                "description": args.description,
                "input_payload": args.input_payload,
                "priority": args.priority,
                "tags": args.tags,
                "tool_options": {
                    "requires_execution": args.requires_execution,
                    "enabled_builtin_kinds": enabled_builtin_kinds,
                    "external_mcp_config_ids": external_mcp_config_ids,
                    "plugin_hints": args.plugin_hints,
                },
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone(),
                "remote_connection_id": source_context.remote_connection_id.clone(),
                "source_run_id": parent.run_id,
                "attachments": source_attachments,
                "prerequisite_task_ids": args.prerequisite_task_ids,
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
) -> Result<CreateBatchPlan, String> {
    if args.tasks.is_empty() || args.tasks.len() > 50 {
        return Err("tasks must contain 1..=50 items".to_string());
    }
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
    let source_attachments = source_attachments(parent)?;
    let mut tasks = Vec::with_capacity(items.len());
    let mut dependencies = Vec::new();
    for item in items {
        let model =
            resolve_task_model(runtime, parent, item.default_model_config_id.as_deref()).await?;
        let enabled_builtin_kinds =
            validated_builtin_kinds(item.requires_execution, item.enabled_builtin_kinds)?;
        let external_mcp_config_ids = validate_external_mcp_ids(&item.external_mcp_config_ids)?;
        validate_plugin_hints(&item.plugin_hints)?;
        let input_payload = attach_dependency_context_payload(
            item.input_payload,
            item.client_ref.trim(),
            &item.context_refs,
        );
        let prompt = task_prompt(&item.objective, &item.description, &input_payload)?;
        let schedule = contact_async_schedule(item.schedule)?;
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
                },
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone(),
                "remote_connection_id": source_context.remote_connection_id.clone(),
                "source_run_id": parent.run_id,
                "attachments": source_attachments.clone(),
                "prerequisite_task_ids": item.prerequisite_task_ids,
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

#[derive(Debug, Clone)]
struct SourceConversationContext {
    conversation_id: Option<String>,
    turn_id: Option<String>,
    remote_connection_id: Option<String>,
}

fn source_attachments(parent: &LocalAgentRunRecord) -> Result<Vec<Value>, String> {
    let Some(attachments) = parent.input.get("attachments") else {
        return Ok(Vec::new());
    };
    attachments
        .as_array()
        .cloned()
        .ok_or_else(|| "source conversation attachments must be an array".to_string())
}

fn source_conversation_context(parent: &LocalAgentRunRecord) -> SourceConversationContext {
    SourceConversationContext {
        conversation_id: input_string(&parent.input, "conversation_id")
            .or_else(|| input_string(&parent.input, "source_conversation_id")),
        turn_id: input_string(&parent.input, "turn_id")
            .or_else(|| input_string(&parent.input, "source_turn_id")),
        remote_connection_id: parent
            .input
            .pointer("/runtime_settings/remote_connection_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| input_string(&parent.input, "remote_connection_id")),
    }
}

fn input_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

async fn resolve_task_model(
    runtime: &LocalAgentRuntime,
    parent: &LocalAgentRunRecord,
    requested: Option<&str>,
) -> Result<(String, String), String> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    if requested.is_none() || requested == Some(parent.model_config_ref.as_str()) {
        return Ok((
            parent.model_config_ref.clone(),
            parent.model_config_revision.clone(),
        ));
    }
    let requested = requested.unwrap_or_default();
    let snapshot = runtime
        .latest_model_config_for_task(&parent.owner_user_id, requested)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("model config not found: {requested}"))?;
    Ok((snapshot.model_config_ref, snapshot.model_config_revision))
}

pub(super) fn envelope(command_id: String, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id,
        command,
    }
}

#[cfg(test)]
#[path = "task_tools_tests.rs"]
mod tests;
