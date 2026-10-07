use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CreateRunCommand, LocalAgentToolStatus, LocalCapabilityPolicySnapshot,
    LocalModelConfigSnapshot, LocalTaskStatus, PutCapabilityPolicySnapshotCommand,
    PutModelConfigSnapshotCommand,
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
                    "message": "plan this",
                    "runtime_settings": {
                        "remote_connection_id": "connection-1",
                        "reasoning_enabled": false
                    },
                    "attachments": [{
                        "attachment_id": "attachment-1",
                        "display_name": "brief.txt",
                        "media_type": "text/plain",
                        "byte_size": 12,
                        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "authorized_local_ref": "local-attachment:brief-1",
                        "metadata": {}
                    }]
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
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            },
            {
                "client_ref": "implement",
                "title": "Implement",
                "objective": "Apply the change",
                "prerequisite_refs": ["research"],
                "requires_execution": true,
                "enabled_builtin_kinds": ["CodeMaintainerWrite"]
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
    let LocalAgentToolOutcome::Succeeded { output } = first else {
        panic!("expected success")
    };
    let LocalAgentToolOutcome::Succeeded {
        output: replay_output,
    } = replay
    else {
        panic!("expected replay success")
    };
    assert_eq!(output["created_tasks"].as_array().map(Vec::len), Some(2));
    assert_eq!(output["dependency_edges"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        output["auto_started_runs"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        output["auto_started_runs"][0]["task_id"],
        output["created_tasks"][0]["task_id"]
    );
    assert_eq!(replay_output["auto_started_runs"], json!([]));
    assert_eq!(replay_output["created_tasks"], output["created_tasks"]);
    assert_eq!(
        replay_output["dependency_edges"],
        output["dependency_edges"]
    );
    let graph = executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .expect("load task graph")
        .expect("task graph");
    assert_eq!(graph.graph_id, "local-task-graph-invocation-1");
    assert_eq!(graph.tasks[0].status, LocalTaskStatus::Ready);
    assert_eq!(graph.tasks[0].max_iterations, 8);
    assert!(graph.tasks[0].active_run_id.is_some());
    assert_eq!(graph.tasks[1].status, LocalTaskStatus::Pending);
    assert_eq!(graph.dependencies.len(), 1);
    assert_eq!(
        graph.tasks[1].input["tool_options"]["enabled_builtin_kinds"],
        json!(["CodeMaintainerWrite", "CodeMaintainerRead"])
    );
    assert_eq!(
        graph.tasks[1].input["source_conversation_id"],
        "conversation-1"
    );
    assert_eq!(graph.tasks[1].input["source_turn_id"], "turn-1");
    assert_eq!(graph.tasks[1].input["remote_connection_id"], "connection-1");
    assert_eq!(
        graph.tasks[1].input["attachments"][0]["authorized_local_ref"],
        "local-attachment:brief-1"
    );
    assert!(graph.tasks[0].input["prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.starts_with("Task Objective:\nInspect the code")));
    assert!(graph.tasks[0].input["prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.contains("Project Inspection Evidence Contract")));
}

#[tokio::test]
async fn task_creation_automatically_freezes_required_policy_capabilities() {
    let runtime = runtime_with_parent().await;
    runtime
        .try_handle(envelope(
            "publish-required-task-policy".to_string(),
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".to_string(),
                    profile_key: "task_policy_internal".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    instructions: Some(
                        json!({
                            "max_iterations": 600,
                            "enabled_builtin_kinds": ["Notepad"],
                            "external_mcp_config_ids": ["required-mcp"],
                            "plugin_keys": ["required-plugin"]
                        })
                        .to_string(),
                    ),
                    prefixed_input_items: Vec::new(),
                    tools: Vec::new(),
                },
            }),
        ))
        .await
        .expect("publish required Task policy");
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Use the required capabilities",
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();

    executor
        .execute_tool(&invocation)
        .await
        .expect("create Task with required policy");
    let graph = executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .expect("load graph")
        .expect("graph");
    let options = &graph.tasks[0].input["tool_options"];
    assert_eq!(graph.tasks[0].max_iterations, 600);
    assert_eq!(options["enabled_builtin_kinds"], json!(["Notepad"]));
    assert_eq!(options["external_mcp_config_ids"], json!(["required-mcp"]));
    assert_eq!(
        options["plugin_hints"],
        json!([{
            "plugin_key": "required-plugin",
            "reason": "required by Local Agent capability policy"
        }])
    );
}

#[tokio::test]
async fn batch_tool_reuses_the_active_plan_for_the_same_source_turn() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let arguments = json!({
        "tasks": [{
            "client_ref": "research",
            "title": "Research",
            "objective": "Inspect the code",
            "requires_execution": false,
            "enabled_builtin_kinds": ["CodeMaintainerRead"]
        }]
    });
    let first = executor
        .execute_tool(&invocation(arguments.clone()))
        .await
        .expect("create initial graph");
    let LocalAgentToolOutcome::Succeeded { output: first } = first else {
        panic!("expected success")
    };

    let mut retry = invocation(arguments);
    retry.invocation_id = "invocation-retried-by-model".to_string();
    retry.call_id = "call-retried-by-model".to_string();
    let LocalAgentToolOutcome::Succeeded { output: reused } = executor
        .execute_tool(&retry)
        .await
        .expect("reuse source-turn graph")
    else {
        panic!("expected success")
    };

    assert_eq!(
        reused["created_tasks"][0]["task_id"],
        first["created_tasks"][0]["task_id"]
    );
    assert_eq!(
        reused["created_tasks"][0]["title"],
        first["created_tasks"][0]["title"]
    );
    assert!(reused["created_tasks"][0].get("client_ref").is_none());
    assert_eq!(reused["dependency_edges"], json!([]));
    assert_eq!(reused["idempotent_reused"], true);
}

#[tokio::test]
async fn batch_tool_preserves_context_edges_and_reduces_transitive_hard_edges() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let invocation = invocation(json!({
        "tasks": [
            {
                "client_ref": "research",
                "title": "Research",
                "objective": "Inspect the code",
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            },
            {
                "client_ref": "implement",
                "title": "Implement",
                "objective": "Apply the change",
                "prerequisite_refs": ["research"],
                "requires_execution": true,
                "enabled_builtin_kinds": ["CodeMaintainerWrite"]
            },
            {
                "client_ref": "review",
                "title": "Review",
                "objective": "Review the implementation",
                "input_payload": "retain this input",
                "prerequisite_refs": ["research", "implement"],
                "context_refs": ["research"],
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            }
        ]
    }));

    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation)
        .await
        .expect("create graph")
    else {
        panic!("expected success")
    };
    assert_eq!(
        output["removed_redundant_edges"],
        json!([{"dependent_id": "review", "prerequisite_id": "research"}])
    );
    let graph = executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .expect("load task graph")
        .expect("task graph");
    assert_eq!(graph.dependencies.len(), 2);
    assert!(graph.dependencies.iter().any(|edge| {
        edge.prerequisite_task_id == graph.tasks[0].task_id
            && edge.task_id == graph.tasks[1].task_id
    }));
    assert!(graph.dependencies.iter().any(|edge| {
        edge.prerequisite_task_id == graph.tasks[1].task_id
            && edge.task_id == graph.tasks[2].task_id
    }));
    assert_eq!(
        graph.tasks[2].input["input_payload"],
        json!({
            "input": "retain this input",
            "execution_client_ref": "review",
            "dependency_context_refs": ["research"]
        })
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
    assert_eq!(
        tools[3]["parameters"]["properties"]["tasks"]["items"]["properties"]["context_refs"]
            ["uniqueItems"],
        true
    );
    assert!(tools[2]["description"]
        .as_str()
        .expect("create task description")
        .contains("inspecting project files"));
    assert!(names.iter().all(|name| !name.starts_with("notepad_")));
    assert_eq!(
        tools[2]["parameters"]["required"],
        json!([
            "title",
            "objective",
            "requires_execution",
            "enabled_builtin_kinds"
        ])
    );
}

#[test]
fn project_inspection_task_prompt_requires_code_and_validation_evidence() {
    let prompt = task_prompt("检查当前项目", "给出项目概览", &Value::Null).expect("task prompt");

    assert!(prompt.contains("项目检查最低证据契约"));
    assert!(prompt.contains("不得只依据 README 或架构文档结束"));
    assert!(prompt.contains("源码入口"));
    assert!(prompt.contains("测试或 CI 配置"));
}

#[tokio::test]
async fn task_tool_rejects_unknown_model_switch() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Do work",
        "default_model_config_id": "model-2",
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();
    let error = executor
        .execute_tool(&invocation)
        .await
        .expect_err("model revision must be resolved");
    assert!(error.contains("model config not found: model-2"));
}

#[tokio::test]
async fn task_tool_resolves_an_explicit_authorized_model_revision() {
    let runtime = runtime_with_parent().await;
    runtime
        .try_handle(envelope(
            "publish-model-2".to_string(),
            HostCommand::PutModelConfigSnapshot(PutModelConfigSnapshotCommand {
                snapshot: LocalModelConfigSnapshot {
                    owner_user_id: "user-1".to_string(),
                    model_config_ref: "model-2".to_string(),
                    model_config_revision: "revision-2".to_string(),
                    credential_ref: "keychain:model/model-2".to_string(),
                    base_url: "https://api.example.test/v1".to_string(),
                    model: "example-model".to_string(),
                    provider: "openai".to_string(),
                    supports_responses: true,
                    supports_images: Some(true),
                    instructions: None,
                    temperature: None,
                    max_output_tokens: None,
                    thinking_level: None,
                    include_prompt_cache_retention: false,
                    request_body_limit_bytes: None,
                    max_transient_retries: None,
                    output_format: None,
                },
            }),
        ))
        .await
        .expect("publish model snapshot");
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Do work",
        "default_model_config_id": "model-2",
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();

    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation)
        .await
        .expect("create task with explicit model")
    else {
        panic!("expected success")
    };
    let task_id = output["id"].as_str().expect("task id");
    let graph = executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .expect("load graph")
        .expect("graph");
    let task = graph
        .tasks
        .iter()
        .find(|task| task.task_id == task_id)
        .expect("task");
    assert_eq!(task.model_config_ref, "model-2");
    assert_eq!(task.model_config_revision, "revision-2");
}

#[tokio::test]
async fn task_tool_rejects_an_empty_execution_prompt() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "   ",
        "requires_execution": false,
        "enabled_builtin_kinds": []
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
        "objective": "Create work from another Task",
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.run_id = "task-parent-run".to_string();
    invocation.tool_name = CREATE_TASK_TOOL.to_string();

    let error = executor
        .execute_tool(&invocation)
        .await
        .expect_err("Task execution must not create nested Tasks");

    assert!(error.contains("active local Main Chat"));
}
