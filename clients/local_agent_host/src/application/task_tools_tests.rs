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
    initialize_parent(runtime).await
}

async fn initialize_parent(runtime: Arc<LocalAgentRuntime>) -> Arc<LocalAgentRuntime> {
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
        execution_route: chatos_local_agent_protocol::LocalAgentToolExecutionRoute::Shared,
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
async fn invalid_single_contracts_and_capabilities_fail_before_write_and_can_be_repaired() {
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let valid = json!({"title": "Inspect", "objective": "Read configuration", "expected_outputs": ["Configuration report"],
        "acceptance_criteria": ["Report cites source evidence"], "requires_execution": false, "enabled_builtin_kinds": ["CodeMaintainerRead"]});
    let mut missing = valid.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("acceptance_criteria");
    let mut empty = valid.clone();
    empty["expected_outputs"] = json!([]);
    let mut unknown = valid.clone();
    unknown["enabled_builtin_kinds"] = json!(["ImaginaryCapability"]);
    let mut unsafe_write = valid.clone();
    unsafe_write["enabled_builtin_kinds"] = json!(["CodeMaintainerWrite"]);
    let mut typo = valid.clone();
    typo["acceptence_criteria"] = json!(["typo"]);
    let mut forged = valid.clone();
    forged["plugin_release_snapshots"] = json!([]);
    let mut invalid_path = valid.clone();
    invalid_path["owned_paths"] = json!(["../outside"]);
    let mut invalid_limit = valid.clone();
    invalid_limit["final_response_max_chars"] = json!(0);
    for arguments in [
        missing,
        empty,
        unknown,
        unsafe_write,
        typo,
        forged,
        invalid_path,
        invalid_limit,
    ] {
        let mut call = invocation(arguments);
        call.tool_name = CREATE_TASK_TOOL.into();
        assert!(!preparation_error(executor.execute_tool(&call).await.unwrap()).is_empty());
        assert!(executor
            .runtime
            .task_graph_by_id("user-1", "local-task-graph-invocation-1")
            .await
            .unwrap()
            .is_none());
    }
    let mut call = invocation(valid);
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
}

#[tokio::test]
async fn optional_notepad_requires_exact_user_request_evidence() {
    let base = json!({
        "title": "Record note",
        "objective": "Record the requested note",
        "expected_outputs": ["Saved note"],
        "acceptance_criteria": ["The requested note is saved"],
        "requires_execution": false,
        "enabled_builtin_kinds": ["Notepad"]
    });
    for arguments in [base.clone(), {
        let mut value = base.clone();
        value["notepad_user_request_quote"] = json!("not in the user message");
        value
    }] {
        let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
        let mut call = invocation(arguments);
        call.tool_name = CREATE_TASK_TOOL.into();
        let error = preparation_error(executor.execute_tool(&call).await.unwrap());
        assert!(error.contains("notepad_user_request_quote"));
        assert!(executor
            .runtime
            .task_graph_by_id("user-1", "local-task-graph-invocation-1")
            .await
            .unwrap()
            .is_none());
    }

    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let mut authorized = base;
    authorized["notepad_user_request_quote"] = json!("plan this");
    let mut call = invocation(authorized);
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
}

#[tokio::test]
async fn single_task_freezes_the_exact_source_user_message() {
    let runtime = runtime_with_parent().await;
    let executor = LocalTaskToolExecutor::new(runtime.clone(), "user-1").unwrap();
    let exact_source = runtime
        .get_run_for_host_worker("parent-run")
        .await
        .unwrap()
        .unwrap()
        .input["message"]
        .as_str()
        .unwrap()
        .to_string();
    let mut call = invocation(json!({
        "title": "Create document",
        "objective": "Create a document containing the requested checklist",
        "description": "The checklist contains three items.",
        "expected_outputs": ["A Word document"],
        "acceptance_criteria": ["The three requested checklist items are present"],
        "requires_execution": true,
        "enabled_builtin_kinds": []
    }));
    call.tool_name = CREATE_TASK_TOOL.into();

    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
    let graph = runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        graph.tasks[0].input["source_user_message"],
        Value::String(exact_source)
    );
}

#[tokio::test]
async fn single_writer_ownership_is_persisted_and_uses_shared_runtime_scope() {
    let runtime = runtime_with_parent().await;
    let executor = LocalTaskToolExecutor::new(runtime.clone(), "user-1").unwrap();
    let mut call = invocation(
        json!({"title": "Comment", "objective": "Add a source-backed comment only", "expected_outputs": ["Comment"],
        "acceptance_criteria": ["Runtime behavior unchanged"], "final_response_max_chars":400, "requires_execution": true,
        "enabled_builtin_kinds": ["CodeMaintainerRead", "CodeMaintainerWrite"], "owned_paths": ["src/config.ts"]}),
    );
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
    let graph = runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        chatos_local_agent_protocol::local_task_runtime_write_scope(
            &graph.tasks[0].input,
            &graph.tasks[0].task_id
        )
        .unwrap(),
        Some(vec!["src/config.ts".into()])
    );
    let tools = task_model_tools();
    assert_eq!(graph.tasks[0].input["acceptance_report_version"], 1);
    assert_eq!(graph.tasks[0].input["final_response_max_chars"], 400);
    assert_eq!(
        tools[2]["parameters"]["properties"]["final_response_max_chars"],
        tools[3]["parameters"]["properties"]["tasks"]["items"]["properties"]
            ["final_response_max_chars"]
    );
    assert_eq!(
        tools[2]["parameters"]["properties"]["owned_paths"],
        tools[3]["parameters"]["properties"]["tasks"]["items"]["properties"]["owned_paths"]
    );
}

#[tokio::test]
async fn missing_required_policy_is_legacy_only_not_a_skill_enabled_run_fallback() {
    let runtime = runtime_with_parent().await;
    let executor = LocalTaskToolExecutor::new(runtime.clone(), "user-1").unwrap();
    let parent = executor.parent_run("parent-run").await.unwrap();
    assert!(executor.required_task_policy(&parent).await.is_ok());
    let bundle = json!({"schema_version": 1, "bundle_revision": "test-bundle",
        "documents": [{"skill_ref": "product-skill:test-policy", "name": "test-policy", "role": "policy",
            "description": "Minimal test policy", "instructions": "Use frozen policy",
            "instructions_sha256": "0".repeat(64), "resources": []}], "tool_skill_bindings": []});
    runtime
        .try_handle(envelope(
            "test-bundle".into(),
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".into(),
                    profile_key: "skill_bundle_internal".into(),
                    capability_policy_revision: "policy-1".into(),
                    instructions: Some(bundle.to_string()),
                    prefixed_input_items: vec![],
                    tools: vec![],
                },
            }),
        ))
        .await
        .unwrap();
    assert!(executor
        .required_task_policy(&parent)
        .await
        .unwrap_err()
        .contains("policy is missing"));
    let mut call = invocation(
        json!({"title": "Inspect", "objective": "Inspect configuration", "expected_outputs": ["Report"],
        "acceptance_criteria": ["Source-backed"], "requires_execution": false, "enabled_builtin_kinds": ["CodeMaintainerRead"]}),
    );
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(
        preparation_error(executor.execute_tool(&call).await.unwrap())
            .contains("policy is missing")
    );
    assert!(runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn cancel_preparation_rejections_preserve_old_task_until_valid_cancel() {
    let runtime = super::super::task_tool_scope_tests::runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(runtime.clone(), "user-1").unwrap();
    let mut create = invocation(
        json!({"title": "Inspect", "objective": "Inspect configuration", "expected_outputs": ["Report"],
        "acceptance_criteria": ["Source-backed"], "requires_execution": false, "enabled_builtin_kinds": ["CodeMaintainerRead"]}),
    );
    create.tool_name = CREATE_TASK_TOOL.into();
    let LocalAgentToolOutcome::Succeeded { output } = executor.execute_tool(&create).await.unwrap()
    else {
        panic!("created task")
    };
    let id = output["id"].as_str().unwrap();
    let original = runtime
        .get_task_for_conversation("user-1", "conversation-1", id)
        .await
        .unwrap()
        .unwrap();
    for (index, args) in [
        json!({"task_id": id, "reason": ""}),
        json!({"task_id": id, "reason": "stop", "replacement_task_ids": [id]}),
        json!({"task_id": id, "reason": "stop", "expected_version": 0}),
        json!({"task_id": "foreign", "reason": "stop"}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut call = invocation(args);
        call.tool_name = CANCEL_TASK_TOOL.into();
        call.invocation_id = format!("invalid-cancel-{index}");
        assert!(!preparation_error(executor.execute_tool(&call).await.unwrap()).is_empty());
        assert_eq!(
            runtime
                .get_task_for_conversation("user-1", "conversation-1", id)
                .await
                .unwrap()
                .unwrap(),
            original
        );
    }
    let mut call = invocation(json!({"task_id": id, "reason": "latest user explicitly cancels"}));
    call.tool_name = CANCEL_TASK_TOOL.into();
    call.invocation_id = "valid-cancel".into();
    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
    assert_eq!(
        runtime
            .get_task_for_conversation("user-1", "conversation-1", id)
            .await
            .unwrap()
            .unwrap()
            .status,
        LocalTaskStatus::Cancelled
    );
}

#[tokio::test]
async fn dispatch_error_after_committed_graph_remains_unknown_not_preparation_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let remaining = Arc::new(AtomicUsize::new(usize::MAX));
    let clock_budget = remaining.clone();
    let runtime = Arc::new(LocalAgentRuntime::with_clock(
        Arc::new(SqliteClientStorage::connect_memory().await.unwrap()),
        Arc::new(move || {
            if clock_budget.fetch_sub(1, Ordering::SeqCst) == 1 {
                Err(chatos_local_agent_runtime::LocalAgentRuntimeError::Clock(
                    "injected dispatch failure".into(),
                ))
            } else {
                Ok(1000)
            }
        }),
    ));
    let runtime = initialize_parent(runtime).await;
    let executor = LocalTaskToolExecutor::new(runtime.clone(), "user-1").unwrap();
    // The first clock read creates the graph; the next begins Task Run dispatch.
    remaining.store(2, Ordering::SeqCst);
    let mut call = invocation(
        json!({"title": "Inspect", "objective": "Inspect configuration", "expected_outputs": ["Report"],
        "acceptance_criteria": ["Source-backed"], "requires_execution": false, "enabled_builtin_kinds": ["CodeMaintainerRead"]}),
    );
    call.tool_name = CREATE_TASK_TOOL.into();
    let error = executor
        .execute_tool(&call)
        .await
        .expect_err("possibly committed work must not return known no-effects failure");
    assert!(error.contains("injected dispatch failure"));
    let graph = runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(graph.tasks.len(), 1);
    assert!(graph.tasks[0].active_run_id.is_none());
}

#[tokio::test]
async fn batch_tool_creates_an_idempotent_local_dag() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let invocation = invocation(json!({
        "graph_goal": "Inspect and implement the requested change",
        "decomposition_rationale": "Investigation must establish the implementation input before mutation begins.",
        "final_delivery_ref": "implement",
        "tasks": [
            {
                "client_ref": "research",
                "title": "Research",
                "objective": "Inspect the code",
                "expected_outputs": ["Code inspection findings"],
                "acceptance_criteria": ["Relevant source paths are identified"],
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            },
            {
                "client_ref": "implement",
                "title": "Implement",
                "objective": "Apply the change",
                "expected_outputs": ["Implemented code change"],
                "acceptance_criteria": ["The requested behavior is implemented and verified"],
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
    assert!(graph
        .tasks
        .iter()
        .all(|task| task.input["source_user_message"] == "plan this"));
    assert_eq!(graph.tasks[1].input["remote_connection_id"], "connection-1");
    assert!(graph.tasks.iter().all(|task| {
        task.input["graph_notification_policy"] == "progress_summary"
            && task.input["notification_milestone"] == false
    }));
    assert_eq!(graph.tasks[0].input["final_delivery"], false);
    assert_eq!(graph.tasks[1].input["final_delivery"], true);
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
                            "plugin_components": [{
                                "plugin_key": "required-plugin",
                                "component_key": "required-component",
                                "reason": "required by Local Agent capability policy"
                            }],
                            "plugin_release_snapshots": [{
                                "plugin_key": "required-plugin",
                                "component_key": "required-component",
                                "plugin_id": "plugin-id-1",
                                "release_id": "release-1",
                                "version": "1.2.3",
                                "artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            }]
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
        "expected_outputs": ["Capability result"],
        "acceptance_criteria": ["Required capabilities are frozen on the Task"],
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
            "component_key": "required-component",
            "reason": "required by Local Agent capability policy"
        }])
    );
    assert_eq!(
        options["plugin_release_snapshots"],
        json!([{
            "plugin_key": "required-plugin",
            "component_key": "required-component",
            "plugin_id": "plugin-id-1",
            "release_id": "release-1",
            "version": "1.2.3",
            "artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }])
    );
}

#[tokio::test]
async fn composer_selected_plugin_is_mandatory_and_excludes_unselected_optional_plugins() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(envelope(
            "create-explicit-plugin-parent".to_string(),
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
                    "message": "帮我整理成正式文档",
                    "explicit_capability_selection": {
                        "kind": "plugin",
                        "plugin_key": "document-plugin",
                        "component_key": "document-mcp",
                        "display_name": "Document"
                    }
                }),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("create parent");
    runtime
        .try_handle(envelope(
            "publish-explicit-plugin-policy".to_string(),
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".to_string(),
                    profile_key: "task_policy_internal".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    instructions: Some(
                        json!({
                            "plugin_release_snapshots": [
                                {
                                    "plugin_key": "document-plugin",
                                    "component_key": "document-mcp",
                                    "plugin_id": "document-id",
                                    "release_id": "document-release",
                                    "version": "1.0.0",
                                    "artifact_sha256": "a".repeat(64)
                                },
                                {
                                    "plugin_key": "other-plugin",
                                    "component_key": "other-mcp",
                                    "plugin_id": "other-id",
                                    "release_id": "other-release",
                                    "version": "1.0.0",
                                    "artifact_sha256": "b".repeat(64)
                                }
                            ]
                        })
                        .to_string(),
                    ),
                    prefixed_input_items: Vec::new(),
                    tools: Vec::new(),
                },
            }),
        ))
        .await
        .expect("publish Plugin catalog");
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let base = json!({
        "title": "Document Task",
        "objective": "Create the requested document",
        "expected_outputs": ["Document artifact"],
        "acceptance_criteria": ["The Document Plugin persisted the artifact"],
        "requires_execution": true,
        "enabled_builtin_kinds": []
    });
    let mut unexpected = base.clone();
    unexpected["plugin_hints"] = json!([{
        "plugin_key": "other-plugin",
        "component_key": "other-mcp",
        "reason": "model chose another optional Plugin"
    }]);
    let mut call = invocation(unexpected);
    call.tool_name = CREATE_TASK_TOOL.to_string();
    let error = preparation_error(executor.execute_tool(&call).await.unwrap());
    assert!(error.contains("outside the user's explicit composer selection"));

    let mut call = invocation(base);
    call.tool_name = CREATE_TASK_TOOL.to_string();
    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
    let graph = executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        graph.tasks[0].input["tool_options"]["plugin_hints"],
        json!([{
            "plugin_key": "document-plugin",
            "component_key": "document-mcp",
            "reason": "selected explicitly in Main Chat"
        }])
    );
    assert_eq!(
        graph.tasks[0].input["tool_options"]["plugin_release_snapshots"][0]["plugin_key"],
        "document-plugin"
    );
}

#[tokio::test]
async fn task_creation_rejects_plugin_outside_the_frozen_release_catalog() {
    let runtime = runtime_with_parent().await;
    runtime
        .try_handle(envelope(
            "publish-plugin-release-catalog".to_string(),
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".to_string(),
                    profile_key: "task_policy_internal".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    instructions: Some(
                        json!({
                            "plugin_release_snapshots": [{
                                "plugin_key": "allowed-plugin",
                                "component_key": "allowed-mcp",
                                "plugin_id": "plugin-id-1",
                                "release_id": "release-1",
                                "version": "1.0.0",
                                "artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            }]
                        })
                        .to_string(),
                    ),
                    prefixed_input_items: Vec::new(),
                    tools: Vec::new(),
                },
            }),
        ))
        .await
        .expect("publish Plugin Release catalog");
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Use an unadvertised Plugin",
        "expected_outputs": ["Plugin result"],
        "acceptance_criteria": ["The result exists"],
        "requires_execution": true,
        "enabled_builtin_kinds": [],
        "plugin_hints": [{"plugin_key": "other-plugin", "component_key": "other-mcp", "reason": "needed"}]
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();

    let error = executor
        .execute_tool(&invocation)
        .await
        .expect("unfrozen Plugin rejection is known before mutation");
    let error = preparation_error(error);
    assert!(error.contains("not frozen in the current capability policy"));
}

#[tokio::test]
async fn batch_tool_reuses_the_active_plan_for_the_same_source_turn() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let arguments = json!({
        "graph_goal": "Inspect two independent areas",
        "decomposition_rationale": "The two inspections have distinct outputs and can run independently.",
        "tasks": [
            {
                "client_ref": "research",
                "title": "Research",
                "objective": "Inspect the code",
                "expected_outputs": ["Code findings"],
                "acceptance_criteria": ["Relevant source is cited"],
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            },
            {
                "client_ref": "tests",
                "title": "Inspect tests",
                "objective": "Inspect the test coverage",
                "expected_outputs": ["Test coverage findings"],
                "acceptance_criteria": ["Relevant tests are cited"],
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            }
        ]
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
        "graph_goal": "Inspect, implement, and review the change",
        "decomposition_rationale": "Implementation consumes research and review independently verifies the implementation.",
        "tasks": [
            {
                "client_ref": "research",
                "title": "Research",
                "objective": "Inspect the code",
                "expected_outputs": ["Research findings"],
                "acceptance_criteria": ["Implementation constraints are identified"],
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            },
            {
                "client_ref": "implement",
                "title": "Implement",
                "objective": "Apply the change",
                "expected_outputs": ["Code change"],
                "acceptance_criteria": ["Change matches the research constraints"],
                "prerequisite_refs": ["research"],
                "requires_execution": true,
                "enabled_builtin_kinds": ["CodeMaintainerWrite"]
            },
            {
                "client_ref": "review",
                "title": "Review",
                "objective": "Review the implementation",
                "expected_outputs": ["Review report"],
                "acceptance_criteria": ["Implementation is independently checked"],
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
    assert_eq!(tools[3]["parameters"]["properties"]["tasks"]["minItems"], 2);
    assert_eq!(
        tools[3]["parameters"]["properties"]["tasks"]["items"]["additionalProperties"],
        false
    );
    assert_eq!(
        tools[3]["parameters"]["required"],
        json!(["graph_goal", "decomposition_rationale", "tasks"])
    );
    assert_eq!(
        tools[3]["parameters"]["properties"]["tasks"]["items"]["required"],
        json!([
            "client_ref",
            "title",
            "objective",
            "expected_outputs",
            "acceptance_criteria",
            "requires_execution",
            "enabled_builtin_kinds"
        ])
    );
    assert_eq!(
        tools[3]["parameters"]["properties"]["tasks"]["items"]["properties"]["context_refs"]
            ["uniqueItems"],
        true
    );
    assert_eq!(
        tools[3]["parameters"]["properties"]["notification_policy"]["default"],
        "progress_summary"
    );
    assert_eq!(
        tools[3]["parameters"]["properties"]["tasks"]["items"]["properties"]
            ["notification_milestone"]["type"],
        "boolean"
    );
    assert_eq!(
        tools[2]["parameters"]["properties"]["thinking_level"]["enum"],
        json!(["none", "auto", "minimal", "low", "medium", "high", "xhigh", "max"])
    );
    assert_eq!(
        tools[3]["parameters"]["properties"]["tasks"]["items"]["properties"]["thinking_level"]
            ["type"],
        "string"
    );
    assert!(tools[2]["description"]
        .as_str()
        .expect("create task description")
        .contains("splitting would not enable useful concurrency"));
    assert!(names.iter().all(|name| !name.starts_with("notepad_")));
    assert_eq!(
        tools[2]["parameters"]["required"],
        json!([
            "title",
            "objective",
            "expected_outputs",
            "acceptance_criteria",
            "requires_execution",
            "enabled_builtin_kinds"
        ])
    );
}

#[tokio::test]
async fn batch_tool_rejects_a_single_task_disguised_as_a_graph() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let invocation = invocation(json!({
        "graph_goal": "One output",
        "decomposition_rationale": "No real decomposition",
        "tasks": [{
            "client_ref": "only",
            "title": "Only Task",
            "objective": "Do one thing",
            "expected_outputs": ["One result"],
            "acceptance_criteria": ["The result exists"],
            "requires_execution": false,
            "enabled_builtin_kinds": []
        }]
    }));

    let error = executor
        .execute_tool(&invocation)
        .await
        .expect("graph size rejection is known before mutation");
    let error = preparation_error(error);
    assert!(error.contains("tasks must contain 2..=50 items"));
}

#[tokio::test]
async fn batch_tool_rejects_invalid_final_delivery_without_creating_tasks() {
    for final_ref in ["missing", "final"] {
        let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
        let call = invocation(json!({
            "graph_goal": "Inspect and synthesize", "decomposition_rationale": "Independent evidence then synthesis",
            "final_delivery_ref": final_ref,
            "tasks": [
                {"client_ref": "source", "title": "Inspect", "objective": "Inspect code", "expected_outputs": ["Evidence"], "acceptance_criteria": ["Source-backed"], "requires_execution": false, "enabled_builtin_kinds": ["CodeMaintainerRead"]},
                {"client_ref": "final", "title": "Synthesize", "objective": "Combine evidence", "expected_outputs": ["Report"], "acceptance_criteria": ["All evidence included"], "requires_execution": false, "enabled_builtin_kinds": []}
            ]
        }));
        let outcome = executor.execute_tool(&call).await.unwrap();
        let LocalAgentToolOutcome::Failed { error, detail } = outcome else {
            panic!("invalid delivery must fail before mutation")
        };
        assert!(error.contains(if final_ref == "missing" {
            "unknown final_delivery_ref"
        } else {
            "depend on every"
        }));
        assert_eq!(detail["side_effects_started"], false);
        assert!(executor
            .runtime
            .task_graph_by_id("user-1", "local-task-graph-invocation-1")
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn batch_tool_rejects_parallel_writers_with_overlapping_owned_paths() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let invocation = invocation(json!({
        "graph_goal": "Implement two platform changes",
        "decomposition_rationale": "The platform changes are intended to run independently.",
        "tasks": [
            {
                "client_ref": "shared",
                "title": "Edit shared clients code",
                "objective": "Change shared client code",
                "expected_outputs": ["Shared code changes"],
                "acceptance_criteria": ["Shared change is verified"],
                "requires_execution": true,
                "enabled_builtin_kinds": ["CodeMaintainerWrite"],
                "owned_paths": ["clients"]
            },
            {
                "client_ref": "mac",
                "title": "Edit macOS client",
                "objective": "Change macOS client code",
                "expected_outputs": ["macOS code changes"],
                "acceptance_criteria": ["macOS change is verified"],
                "requires_execution": true,
                "enabled_builtin_kinds": ["CodeMaintainerWrite"],
                "owned_paths": ["clients/macos"]
            }
        ]
    }));

    let outcome = executor
        .execute_tool(&invocation)
        .await
        .expect("pure validation rejection has a known result");
    let LocalAgentToolOutcome::Failed { error, detail } = outcome else {
        panic!("overlapping parallel writers must fail without requiring review");
    };
    assert!(error.contains("overlapping owned_paths"));
    assert!(error.contains("add a prerequisite"));
    assert_eq!(detail["side_effects_started"], false);
    assert!(executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .is_none());
}

#[test]
fn project_inspection_task_prompt_requires_code_and_validation_evidence() {
    let prompt = task_prompt(
        "检查当前项目",
        "给出项目概览",
        &["项目检查报告".to_string()],
        &["报告引用源码与测试证据".to_string()],
        &Value::Null,
    )
    .expect("task prompt");

    assert!(prompt.contains("项目检查最低证据契约"));
    assert!(prompt.contains("不得只依据 README 或架构文档结束"));
    assert!(prompt.contains("源码入口"));
    assert!(prompt.contains("测试或 CI 配置"));
    assert!(prompt.contains("预期输出"));
    assert!(prompt.contains("验收标准"));
}

#[tokio::test]
async fn task_tool_rejects_unknown_model_switch() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Do work",
        "expected_outputs": ["Completed work"],
        "acceptance_criteria": ["Work is complete"],
        "default_model_config_id": "model-2",
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();
    let error = executor
        .execute_tool(&invocation)
        .await
        .expect("model lookup failure precedes mutation");
    let error = preparation_error(error);
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
        "expected_outputs": ["Completed work"],
        "acceptance_criteria": ["Work is complete"],
        "default_model_config_id": "model-2",
        "thinking_level": "high",
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
    assert_eq!(
        task.input["runtime_settings"],
        json!({
            "selected_thinking_level": "high",
            "reasoning_enabled": true
        })
    );
}

#[tokio::test]
async fn task_tool_rejects_a_model_excluded_by_required_policy() {
    let runtime = runtime_with_parent().await;
    runtime
        .try_handle(envelope(
            "publish-required-task-model-policy".to_string(),
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".to_string(),
                    profile_key: "task_policy_internal".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    instructions: Some(
                        json!({
                            "allowed_model_config_ids": ["model-2"]
                        })
                        .to_string(),
                    ),
                    prefixed_input_items: Vec::new(),
                    tools: Vec::new(),
                },
            }),
        ))
        .await
        .expect("publish required Task model policy");
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Do work",
        "expected_outputs": ["Completed work"],
        "acceptance_criteria": ["Work is complete"],
        "default_model_config_id": "model-1",
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();

    let error = executor
        .execute_tool(&invocation)
        .await
        .expect("excluded model rejection precedes mutation");
    let error = preparation_error(error);
    assert!(error.contains("model configuration is not enabled for Tasks: model-1"));
}

#[tokio::test]
async fn task_tool_falls_back_when_main_chat_model_is_excluded_by_required_policy() {
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
        .expect("publish fallback model");
    runtime
        .try_handle(envelope(
            "publish-required-task-model-policy".to_string(),
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".to_string(),
                    profile_key: "task_policy_internal".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    instructions: Some(
                        json!({
                            "allowed_model_config_ids": ["model-2"]
                        })
                        .to_string(),
                    ),
                    prefixed_input_items: Vec::new(),
                    tools: Vec::new(),
                },
            }),
        ))
        .await
        .expect("publish required Task model policy");
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "Do work",
        "expected_outputs": ["Completed work"],
        "acceptance_criteria": ["Work is complete"],
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();

    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation)
        .await
        .expect("create Task with fallback model")
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

#[test]
fn task_thinking_override_normalizes_disabled_aliases_and_rejects_unknown_levels() {
    assert_eq!(
        task_runtime_settings(Some("disabled")).expect("disabled override"),
        json!({
            "selected_thinking_level": "none",
            "reasoning_enabled": false
        })
    );
    assert!(task_runtime_settings(Some("extreme")).is_err());
    assert_eq!(
        task_runtime_settings(None).expect("default thinking"),
        Value::Null
    );
}

#[tokio::test]
async fn task_tool_rejects_an_empty_execution_prompt() {
    let executor =
        LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").expect("executor");
    let mut invocation = invocation(json!({
        "title": "Task",
        "objective": "   ",
        "expected_outputs": ["Completed work"],
        "acceptance_criteria": ["Work is complete"],
        "requires_execution": false,
        "enabled_builtin_kinds": []
    }));
    invocation.tool_name = CREATE_TASK_TOOL.to_string();
    let error = executor
        .execute_tool(&invocation)
        .await
        .expect("empty objective rejection precedes mutation");
    let error = preparation_error(error);
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
