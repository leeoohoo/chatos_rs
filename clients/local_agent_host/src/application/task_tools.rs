// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::task_tool_definitions::*;
use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CancelTaskCommand, CreateTaskGraphCommand, GetTaskGraphCommand, HostCommand,
    HostRequestEnvelope, HostResult, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, LocalTaskDependency, LocalTaskSpec, LocalTaskStatus,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct LocalTaskToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
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

    async fn parent_run(&self, run_id: &str) -> Result<LocalAgentRunRecord, String> {
        self.runtime
            .get_run_for_host_worker(run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("parent Run not found: {run_id}"))
    }

    async fn create_graph(
        &self,
        invocation_id: &str,
        command: CreateTaskGraphCommand,
    ) -> Result<Value, String> {
        match self
            .runtime
            .try_handle(envelope(
                format!("task-tool-create-{invocation_id}"),
                HostCommand::CreateTaskGraph(command),
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            HostResult::TaskGraph { graph } => {
                serde_json::to_value(graph).map_err(|error| error.to_string())
            }
            result => Err(format!("unexpected Task Graph response: {result:?}")),
        }
    }

    async fn task_graph(&self, graph_id: &str) -> Result<Value, String> {
        match self
            .runtime
            .try_handle(envelope(
                format!("task-tool-get-graph-{graph_id}"),
                HostCommand::GetTaskGraph(GetTaskGraphCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    graph_id: graph_id.to_string(),
                }),
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            HostResult::TaskGraph { graph } => {
                serde_json::to_value(graph).map_err(|error| error.to_string())
            }
            result => Err(format!("unexpected Task Graph response: {result:?}")),
        }
    }

    async fn cancel_task(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
        args: CancelTaskArgs,
    ) -> Result<Value, String> {
        match self
            .runtime
            .try_handle(envelope(
                format!("task-tool-cancel-{}", invocation.invocation_id),
                HostCommand::CancelTask(CancelTaskCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    task_id: args.task_id,
                    expected_version: args.expected_version,
                    reason: args.reason,
                }),
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            HostResult::TaskGraph { graph } => {
                serde_json::to_value(graph).map_err(|error| error.to_string())
            }
            result => Err(format!("unexpected Task Graph response: {result:?}")),
        }
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
                let tasks = self
                    .runtime
                    .list_tasks_for_conversation(
                        &parent.owner_user_id,
                        conversation_id,
                        args.status,
                        args.keyword.as_deref(),
                        args.limit.unwrap_or(50),
                        args.offset.unwrap_or(0),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_value(tasks).map_err(|error| error.to_string())?
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
                serde_json::to_value(task).map_err(|error| error.to_string())?
            }
            CREATE_TASK_TOOL => {
                let args: CreateTaskArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid create_task input: {error}"))?;
                let graph = create_single_graph(invocation, &parent, args)?;
                self.create_graph(&invocation.invocation_id, graph).await?
            }
            CREATE_TASKS_TOOL => {
                let args: CreateTasksArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| {
                        format!("invalid create_tasks_with_prerequisites input: {error}")
                    })?;
                let graph = create_batch_graph(invocation, &parent, args)?;
                self.create_graph(&invocation.invocation_id, graph).await?
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
                    .map_err(|error| error.to_string())?;
                if task.is_none() {
                    return Err(format!("task not found: {}", args.task_id));
                }
                self.cancel_task(invocation, args).await?
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
                self.task_graph(&task.graph_id).await?
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
    status: Option<LocalTaskStatus>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskIdArgs {
    task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelTaskArgs {
    task_id: String,
    reason: String,
    #[serde(default)]
    expected_version: Option<u64>,
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
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateTasksArgs {
    tasks: Vec<CreateTaskItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateTaskItem {
    client_ref: String,
    title: String,
    objective: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    input_payload: Value,
    #[serde(default)]
    default_model_config_id: Option<String>,
    #[serde(default)]
    prerequisite_refs: Vec<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

fn create_single_graph(
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    args: CreateTaskArgs,
) -> Result<CreateTaskGraphCommand, String> {
    ensure_no_external_prerequisites(&args.prerequisite_task_ids)?;
    ensure_parent_model(parent, args.default_model_config_id.as_deref())?;
    let prompt = task_prompt(&args.objective, &args.description, &args.input_payload)?;
    let graph_id = format!("local-task-graph-{}", invocation.invocation_id);
    let source_context = source_conversation_context(parent);
    Ok(CreateTaskGraphCommand {
        graph_id,
        owner_user_id: parent.owner_user_id.clone(),
        source_entity_type: parent.owner_entity_type.clone(),
        source_entity_id: parent.owner_entity_id.clone(),
        tasks: vec![LocalTaskSpec {
            task_id: format!("local-task-{}-1", invocation.invocation_id),
            title: args.title,
            profile_key: "task_execution".to_string(),
            model_config_ref: parent.model_config_ref.clone(),
            model_config_revision: parent.model_config_revision.clone(),
            capability_policy_revision: parent.capability_policy_revision.clone(),
            input: json!({
                "prompt": prompt,
                "objective": args.objective,
                "description": args.description,
                "input_payload": args.input_payload,
                "tool_options": args.extra,
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone()
            }),
            max_iterations: parent.max_iterations,
        }],
        dependencies: Vec::new(),
    })
}

fn create_batch_graph(
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    args: CreateTasksArgs,
) -> Result<CreateTaskGraphCommand, String> {
    if args.tasks.is_empty() || args.tasks.len() > 50 {
        return Err("tasks must contain 1..=50 items".to_string());
    }
    let mut ref_to_id = HashMap::new();
    for (index, item) in args.tasks.iter().enumerate() {
        let client_ref = item.client_ref.trim();
        if client_ref.is_empty() {
            return Err("client_ref cannot be empty".to_string());
        }
        let task_id = format!("local-task-{}-{}", invocation.invocation_id, index + 1);
        if ref_to_id.insert(client_ref.to_string(), task_id).is_some() {
            return Err(format!("client_ref is duplicated: {client_ref}"));
        }
    }
    let source_context = source_conversation_context(parent);
    let mut tasks = Vec::with_capacity(args.tasks.len());
    let mut dependencies = Vec::new();
    for item in args.tasks {
        ensure_no_external_prerequisites(&item.prerequisite_task_ids)?;
        ensure_parent_model(parent, item.default_model_config_id.as_deref())?;
        let prompt = task_prompt(&item.objective, &item.description, &item.input_payload)?;
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
            model_config_ref: parent.model_config_ref.clone(),
            model_config_revision: parent.model_config_revision.clone(),
            capability_policy_revision: parent.capability_policy_revision.clone(),
            input: json!({
                "prompt": prompt,
                "objective": item.objective,
                "description": item.description,
                "input_payload": item.input_payload,
                "client_ref": item.client_ref,
                "tool_options": item.extra,
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone()
            }),
            max_iterations: parent.max_iterations,
        });
    }
    Ok(CreateTaskGraphCommand {
        graph_id: format!("local-task-graph-{}", invocation.invocation_id),
        owner_user_id: parent.owner_user_id.clone(),
        source_entity_type: parent.owner_entity_type.clone(),
        source_entity_id: parent.owner_entity_id.clone(),
        tasks,
        dependencies,
    })
}

fn task_prompt(
    objective: &str,
    description: &str,
    input_payload: &Value,
) -> Result<String, String> {
    let objective = objective.trim();
    if objective.is_empty() {
        return Err("task objective cannot be empty".to_string());
    }
    let mut prompt = format!("Objective: {objective}");
    let description = description.trim();
    if !description.is_empty() {
        prompt.push_str("\n\nDescription: ");
        prompt.push_str(description);
    }
    if !input_payload.is_null() {
        prompt.push_str("\n\nStructured input: ");
        prompt.push_str(
            &serde_json::to_string(input_payload)
                .map_err(|error| format!("task input is not serializable: {error}"))?,
        );
    }
    Ok(prompt)
}

#[derive(Debug, Clone)]
struct SourceConversationContext {
    conversation_id: Option<String>,
    turn_id: Option<String>,
}

fn source_conversation_context(parent: &LocalAgentRunRecord) -> SourceConversationContext {
    SourceConversationContext {
        conversation_id: input_string(&parent.input, "conversation_id")
            .or_else(|| input_string(&parent.input, "source_conversation_id")),
        turn_id: input_string(&parent.input, "turn_id")
            .or_else(|| input_string(&parent.input, "source_turn_id")),
    }
}

fn input_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn ensure_parent_model(
    parent: &LocalAgentRunRecord,
    requested: Option<&str>,
) -> Result<(), String> {
    if requested.is_some_and(|value| value != parent.model_config_ref) {
        return Err(
            "local Task tools cannot switch model_config_id until its exact revision is resolved"
                .to_string(),
        );
    }
    Ok(())
}

fn ensure_no_external_prerequisites(prerequisites: &[String]) -> Result<(), String> {
    if prerequisites.is_empty() {
        Ok(())
    } else {
        Err(
            "local Task tools currently accept only prerequisite_refs from the same batch"
                .to_string(),
        )
    }
}

fn envelope(command_id: String, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id,
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateRunCommand, LocalAgentToolStatus, LocalTaskGraph, LocalTaskStatus,
    };

    async fn runtime_with_parent() -> Arc<LocalAgentRuntime> {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                "create-parent".to_string(),
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "parent-run".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: "conversation-1".to_string(),
                    profile_key: "main_chat".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({
                        "conversation_id": "conversation-1",
                        "turn_id": "turn-1",
                        "message": "plan this"
                    }),
                    max_iterations: 8,
                }),
            ))
            .await
            .expect("create parent");
        runtime
    }

    fn invocation(arguments: Value) -> LocalAgentToolInvocationRecord {
        LocalAgentToolInvocationRecord {
            invocation_id: "invocation-1".to_string(),
            run_id: "parent-run".to_string(),
            batch_id: "batch-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: CREATE_TASKS_TOOL.to_string(),
            arguments,
            side_effecting: true,
            requires_approval: false,
            approval_status: chatos_local_agent_protocol::LocalAgentToolApprovalStatus::NotRequired,
            approval_decided_by: None,
            approval_reason: None,
            approval_decided_at_unix_ms: None,
            status: LocalAgentToolStatus::Running,
            result: None,
            error: None,
            version: 2,
            claim_token: Some("claim-1".to_string()),
            claim_until_unix_ms: Some(i64::MAX),
            created_at_unix_ms: 1_000,
            updated_at_unix_ms: 1_000,
        }
    }

    #[tokio::test]
    async fn batch_tool_creates_an_idempotent_local_dag() {
        let executor =
            LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
        let invocation = invocation(json!({
            "tasks": [
                {
                    "client_ref": "research",
                    "title": "Research",
                    "objective": "Inspect the code",
                    "requires_execution": false,
                    "enabled_builtin_kinds": []
                },
                {
                    "client_ref": "implement",
                    "title": "Implement",
                    "objective": "Apply the change",
                    "prerequisite_refs": ["research"],
                    "requires_execution": true,
                    "enabled_builtin_kinds": ["filesystem"]
                }
            ]
        }));
        let first = executor
            .execute_tool(&invocation)
            .await
            .expect("create graph");
        let replay = executor
            .execute_tool(&invocation)
            .await
            .expect("replay graph");
        assert_eq!(first, replay);
        let LocalAgentToolOutcome::Succeeded { output } = first else {
            panic!("expected success")
        };
        let graph: LocalTaskGraph = serde_json::from_value(output).expect("task graph");
        assert_eq!(graph.graph_id, "local-task-graph-invocation-1");
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Ready);
        assert_eq!(graph.tasks[1].status, LocalTaskStatus::Pending);
        assert_eq!(graph.dependencies.len(), 1);
        assert_eq!(
            graph.tasks[1].input["tool_options"]["enabled_builtin_kinds"],
            json!(["filesystem"])
        );
        assert_eq!(
            graph.tasks[1].input["source_conversation_id"],
            "conversation-1"
        );
        assert_eq!(graph.tasks[1].input["source_turn_id"], "turn-1");
        assert_eq!(
            graph.tasks[0].input["prompt"],
            "Objective: Inspect the code"
        );
        assert_eq!(
            graph.tasks[1].input["prompt"],
            "Objective: Apply the change"
        );
    }

    #[test]
    fn task_model_definitions_are_host_owned_and_closed() {
        let tools = task_model_tools();
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect::<Vec<_>>();
        assert_eq!(names, TASK_TOOL_NAMES);
        assert!(tools
            .iter()
            .all(|tool| tool["parameters"]["additionalProperties"] == false));
        assert_eq!(
            tools[3]["parameters"]["properties"]["tasks"]["maxItems"],
            50
        );
        assert_eq!(
            tools[3]["parameters"]["properties"]["tasks"]["items"]["additionalProperties"],
            false
        );
        assert!(tools[2]["description"]
            .as_str()
            .expect("create task description")
            .contains("inspecting project files"));
        assert!(names.iter().all(|name| !name.starts_with("notepad_")));
    }

    #[tokio::test]
    async fn task_tool_rejects_unresolved_model_switch() {
        let executor =
            LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
        let mut invocation = invocation(json!({
            "title": "Task",
            "objective": "Do work",
            "default_model_config_id": "model-2"
        }));
        invocation.tool_name = CREATE_TASK_TOOL.to_string();
        let error = executor
            .execute_tool(&invocation)
            .await
            .expect_err("model revision must be resolved");
        assert!(error.contains("cannot switch model_config_id"));
    }

    #[tokio::test]
    async fn task_tool_rejects_an_empty_execution_prompt() {
        let executor =
            LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
        let mut invocation = invocation(json!({
            "title": "Task",
            "objective": "   "
        }));
        invocation.tool_name = CREATE_TASK_TOOL.to_string();
        let error = executor
            .execute_tool(&invocation)
            .await
            .expect_err("empty objective must not create an unexecutable task");
        assert!(error.contains("objective cannot be empty"));
    }

    #[tokio::test]
    async fn task_tool_rejects_a_non_main_chat_parent() {
        let runtime = runtime_with_parent().await;
        runtime
            .try_handle(envelope(
                "create-task-parent".to_string(),
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "task-parent-run".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "task".to_string(),
                    owner_entity_id: "task-1".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"prompt": "do the work"}),
                    max_iterations: 8,
                }),
            ))
            .await
            .expect("create task parent");
        let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
        let mut invocation = invocation(json!({
            "title": "Nested Task",
            "objective": "Create work from another Task"
        }));
        invocation.run_id = "task-parent-run".to_string();
        invocation.tool_name = CREATE_TASK_TOOL.to_string();

        let error = executor
            .execute_tool(&invocation)
            .await
            .expect_err("Task execution must not create nested Tasks");

        assert!(error.contains("active local Main Chat"));
    }
}
