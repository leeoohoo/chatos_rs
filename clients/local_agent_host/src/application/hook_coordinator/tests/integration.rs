// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(target_os = "macos")]
#[tokio::test]
async fn ponytail_fixture_runs_real_session_prompt_and_subagent_hooks() {
    let (storage, runtime, _) = fixture().await;
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/codex-hooks/ponytail-5.3.2")
        .canonicalize()
        .expect("Ponytail fixture root");
    let hooks = ponytail_fixture_hooks(&fixture_root);
    let sandbox = tempfile::tempdir().expect("sandbox root");
    let workspace = sandbox.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::write(workspace.join("visible.txt"), "visible").expect("workspace file");
    let data_root = sandbox.path().join("plugin-data");
    let bundle = LocalPluginHookBundleSpec {
        bundle_id: "bundle-ponytail-5-3-2".to_string(),
        owner_user_id: "user-1".to_string(),
        plugin_id: "ponytail".to_string(),
        release_id: "5-3-2".to_string(),
        component_key: "lifecycle-hooks".to_string(),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: fixture_root.display().to_string(),
        permissions: vec!["process.spawn".to_string(), "workspace.read".to_string()],
        task_policy: None,
        hooks,
        enabled: true,
        trusted_at_unix_ms: 9_000,
    };
    storage
        .put_plugin_hook_bundle(&command("put-ponytail"), &bundle, 10_000)
        .await
        .expect("put Ponytail bundle");
    storage
        .set_active_plugin_hook_bundles(
            &command("activate-ponytail"),
            "user-1",
            std::slice::from_ref(&bundle.bundle_id),
            10_001,
        )
        .await
        .expect("activate Ponytail bundle");
    let runner = MacOsSeatbeltHookRunner::new(data_root.clone()).expect("runner");
    let coordinator = LocalHookCoordinator::new(
        runtime.clone(),
        runner,
        Some(workspace.canonicalize().expect("canonical workspace")),
    )
    .expect("coordinator");

    let initial_prompt = StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version: 1,
        conversation_if_missing: None,
        turn_id: "turn-initial".to_string(),
        message_id: "message-initial".to_string(),
        run_id: "run-initial".to_string(),
        message: "inspect this project".to_string(),
        message_metadata: json!({"source": "composer"}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    };
    assert!(matches!(
        coordinator.gate_user_prompt(&initial_prompt).await.unwrap(),
        LocalHookPromptDecision::Allow
    ));
    let mut initial_run = run("run-initial");
    initial_run.input = json!({
        "conversation_id": "conversation-1",
        "turn_id": "turn-initial",
        "message": "inspect this project",
    });
    let initial_context = coordinator
        .prefixed_context(&claim(initial_run.clone()))
        .await
        .expect("initial context");
    assert!(combined_context(&initial_context).contains("PONYTAIL MODE ACTIVE — level: full"));

    let switch_prompt = StartConversationTurnCommand {
        turn_id: "turn-ultra".to_string(),
        message_id: "message-ultra".to_string(),
        run_id: "run-ultra".to_string(),
        message: "/ponytail ultra".to_string(),
        expected_conversation_version: 2,
        ..initial_prompt
    };
    assert!(matches!(
        coordinator.gate_user_prompt(&switch_prompt).await.unwrap(),
        LocalHookPromptDecision::Allow
    ));
    let mut ultra_run = run("run-ultra");
    ultra_run.input = json!({
        "conversation_id": "conversation-1",
        "turn_id": "turn-ultra",
        "message": "/ponytail ultra",
    });
    let ultra_context = coordinator
        .prefixed_context(&claim(ultra_run))
        .await
        .expect("ultra context");
    let ultra_context = combined_context(&ultra_context);
    assert!(
        ultra_context.contains("PONYTAIL MODE CHANGED — level: ultra"),
        "{ultra_context}"
    );

    storage
        .create_run(
            &command("create-parent-for-ponytail"),
            &initial_run,
            "parent-event",
        )
        .await
        .expect("create parent run");
    let mut task = run("task-ponytail");
    task.owner_entity_type = "task".to_string();
    task.owner_entity_id = "task-1".to_string();
    task.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    task.input = json!({
        "prompt": "perform delegated work",
        "source_run_id": "run-initial",
        "source_conversation_id": "conversation-1",
    });
    let task_context = coordinator
        .prefixed_context(&claim(task))
        .await
        .expect("task context");
    assert!(combined_context(&task_context).contains("PONYTAIL MODE ACTIVE — level: ultra"));

    let plugin_data = std::fs::read_dir(&data_root)
        .expect("plugin data root")
        .next()
        .expect("plugin data directory")
        .expect("plugin data entry")
        .path();
    assert_eq!(
        std::fs::read_to_string(plugin_data.join(".ponytail-active")).expect("Ponytail mode file"),
        "ultra"
    );
}

#[tokio::test]
async fn prompt_block_prevents_planner_context_and_invalid_output_stays_private() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-block",
        vec![hook("block", "UserPromptSubmit", "block")],
    )
    .await;
    activate_bundle(
        &storage,
        "bundle-invalid",
        vec![hook("invalid", "SessionStart", "invalid")],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).unwrap();

    let error = coordinator
        .prefixed_context(&claim(run("run-main")))
        .await
        .expect_err("prompt must be blocked before a model request is built");

    assert!(error.contains("blocked by fixture"));
    assert!(!error.contains("private failure"));
}

#[tokio::test]
async fn prompt_gate_blocks_before_the_user_message_and_run_are_persisted() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-block",
        vec![hook("block", "UserPromptSubmit", "block")],
    )
    .await;
    runtime
        .try_handle(envelope(
            "create-conversation",
            HostCommand::CreateConversation(CreateConversationCommand {
                conversation_id: "conversation-1".to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Hooks".to_string(),
                resource: None,
            }),
        ))
        .await
        .expect("create conversation");
    let hooks = Arc::new(LocalHookCoordinator::new(runtime.clone(), runner, None).unwrap());
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(MAIN_CHAT_PROFILE_KEY, NeverModelProfile)
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(runtime.clone(), profiles, "user-1", "model-worker")
        .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(runtime.clone(), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_hook_coordinator(hooks);

    let response = coordinator
        .handle_request(envelope(
            "start-blocked-turn",
            HostCommand::StartConversationTurn(StartConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                expected_conversation_version: 1,
                conversation_if_missing: None,
                turn_id: "turn-blocked".to_string(),
                message_id: "message-blocked".to_string(),
                run_id: "run-blocked".to_string(),
                message: "do not persist me".to_string(),
                message_metadata: json!({"source": "composer"}),
                attachments: Vec::new(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                max_iterations: 8,
            }),
        ))
        .await;

    assert_eq!(response.error.expect("blocked").code, "hook_blocked");
    assert!(runtime
        .get_run_for_host_worker("run-blocked")
        .await
        .expect("run lookup")
        .is_none());
    let history = runtime
        .try_handle(envelope(
            "history-after-block",
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
            }),
        ))
        .await
        .expect("history");
    let HostResult::Conversation { conversation: page } = history else {
        panic!("unexpected history result")
    };
    assert!(page.turns.is_empty());
    assert!(page.messages.is_empty());
}

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

#[cfg(target_os = "macos")]
fn ponytail_fixture_hooks(fixture_root: &std::path::Path) -> Vec<LocalPluginHookDefinition> {
    let document: Value = serde_json::from_slice(
        &std::fs::read(fixture_root.join("hooks/claude-codex-hooks.json"))
            .expect("Ponytail Hook manifest"),
    )
    .expect("Ponytail Hook JSON");
    ["SessionStart", "SubagentStart", "UserPromptSubmit"]
        .into_iter()
        .map(|event| {
            let group = document["hooks"][event]
                .as_array()
                .and_then(|groups| groups.first())
                .expect("Ponytail event group");
            let handler = group["hooks"]
                .as_array()
                .and_then(|handlers| handlers.first())
                .expect("Ponytail Hook handler");
            LocalPluginHookDefinition {
                hook_id: format!("ponytail-{}", event.to_ascii_lowercase()),
                event: event.to_string(),
                matcher: group["matcher"].as_str().map(ToString::to_string),
                command: handler["command"]
                    .as_str()
                    .expect("Ponytail Hook command")
                    .to_string(),
                is_async: false,
                timeout_ms: handler["timeout"].as_u64().expect("Ponytail timeout") * 1_000,
                additional_context_limit: 256 * 1_024,
            }
        })
        .collect()
}

fn combined_context(items: &[Value]) -> String {
    items
        .iter()
        .filter_map(|item| item["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_invocation(run_id: &str, call: &LocalAgentToolCall) -> LocalAgentToolInvocationRecord {
    LocalAgentToolInvocationRecord {
        invocation_id: format!("invocation-{}", call.call_id),
        run_id: run_id.to_string(),
        batch_id: "batch-1".to_string(),
        call_id: call.call_id.clone(),
        tool_name: call.tool_name.clone(),
        execution_route: LocalAgentToolExecutionRoute::Shared,
        arguments: call.arguments.clone(),
        side_effecting: call.side_effecting,
        requires_approval: call.requires_approval,
        approval_status: LocalAgentToolApprovalStatus::NotRequired,
        approval_decided_by: None,
        approval_reason: None,
        approval_decided_at_unix_ms: None,
        status: LocalAgentToolStatus::Running,
        result: None,
        error: None,
        version: 2,
        claim_token: Some("tool-token".to_string()),
        claim_until_unix_ms: Some(20_000),
        created_at_unix_ms: 10_000,
        updated_at_unix_ms: 10_000,
    }
}
