// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[tokio::test]
async fn main_context_is_durable_and_completed_hooks_do_not_run_again() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-1",
        vec![
            hook("session", "SessionStart", "session context"),
            hook("prompt", "UserPromptSubmit", "prompt context"),
        ],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();
    let claim = claim(run("run-main"));

    let first = coordinator.prefixed_context(&claim).await.unwrap();
    let second = coordinator.prefixed_context(&claim).await.unwrap();

    assert_eq!(runner.call_count(), 2);
    assert_eq!(runner.events(), vec!["SessionStart", "UserPromptSubmit"]);
    assert_eq!(first, second);
    let combined = first
        .iter()
        .filter_map(|item| item["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(combined.contains("session context"));
    assert!(combined.contains("prompt context"));
    assert!(!combined.contains("private failure"));
}

#[cfg(windows)]
#[tokio::test]
async fn windows_appcontainer_runner_executes_an_active_user_prompt_hook() {
    let root = tempfile::tempdir().expect("hook root");
    let installation = root.path().join("plugin");
    std::fs::create_dir_all(&installation).expect("installation");
    std::fs::write(installation.join("hook.js"), "fixture").expect("hook fixture");
    std::fs::write(
            installation.join("hook.cmd"),
            "@echo off\r\necho {\"hookSpecificOutput\":{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"windows-hook-ok\"}}\r\n",
        )
        .expect("hook command fixture");
    std::fs::write(
        installation.join("block.cmd"),
        "@echo off\r\necho {\"continue\":false,\"stopReason\":\"windows-hook-blocked\"}\r\n",
    )
    .expect("blocking command fixture");
    let installation_root = installation.canonicalize().expect("canonical installation");
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::with_clock(
        storage.clone(),
        Arc::new(|| Ok(10_000)),
    ));
    runtime.initialize("user-1").await.expect("initialize");
    let bundle = LocalPluginHookBundleSpec {
        bundle_id: "windows-hook-bundle".into(),
        owner_user_id: "user-1".into(),
        plugin_id: "plugin-windows-hook".into(),
        release_id: "release-1".into(),
        component_key: "lifecycle".into(),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: installation_root.to_string_lossy().into_owned(),
        permissions: vec!["process.spawn".into()],
        task_policy: None,
        hooks: vec![hook("prompt", "UserPromptSubmit", "hook.cmd")],
        enabled: true,
        trusted_at_unix_ms: 1,
    };
    storage
        .put_plugin_hook_bundle(&command("put-windows-hook"), &bundle, 10_000)
        .await
        .expect("put bundle");
    storage
        .set_active_plugin_hook_bundles(
            &command("activate-windows-hook"),
            "user-1",
            std::slice::from_ref(&bundle.bundle_id),
            10_001,
        )
        .await
        .expect("activate bundle");
    let runner = WindowsAppContainerHookRunner::new(root.path().join("hook-data")).expect("runner");
    let coordinator =
        LocalHookCoordinator::new(runtime.clone(), runner, None).expect("coordinator");
    let prompt = StartConversationTurnCommand {
        owner_user_id: "user-1".into(),
        conversation_id: "conversation-1".into(),
        expected_conversation_version: 1,
        conversation_if_missing: None,
        turn_id: "turn-1".into(),
        message_id: "message-1".into(),
        run_id: "run-1".into(),
        message: "hello".into(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        model_config_ref: "model-1".into(),
        model_config_revision: "revision-1".into(),
        capability_policy_revision: "policy-1".into(),
        max_iterations: 8,
    };
    assert!(matches!(
        coordinator
            .gate_user_prompt(&prompt)
            .await
            .expect("prompt gate"),
        LocalHookPromptDecision::Allow
    ));
    let context = coordinator
        .prefixed_context(&claim(run("run-windows-hook")))
        .await
        .expect("hook context");
    assert!(combined_context(&context).contains("windows-hook-ok"));

    let mut blocking_bundle = bundle.clone();
    blocking_bundle.bundle_id = "windows-blocking-hook-bundle".into();
    blocking_bundle.plugin_id = "plugin-windows-blocking-hook".into();
    blocking_bundle.hooks = vec![hook("block", "UserPromptSubmit", "block.cmd")];
    storage
        .put_plugin_hook_bundle(
            &command("put-windows-blocking-hook"),
            &blocking_bundle,
            10_002,
        )
        .await
        .expect("put blocking bundle");
    storage
        .set_active_plugin_hook_bundles(
            &command("activate-windows-blocking-hook"),
            "user-1",
            &[bundle.bundle_id.clone(), blocking_bundle.bundle_id.clone()],
            10_003,
        )
        .await
        .expect("activate blocking bundle");
    let blocking_prompt = StartConversationTurnCommand {
        turn_id: "turn-2".into(),
        run_id: "run-2".into(),
        message: "blocked".into(),
        ..prompt
    };
    match coordinator
        .gate_user_prompt(&blocking_prompt)
        .await
        .expect("blocking prompt gate")
    {
        LocalHookPromptDecision::Block { reason } => {
            assert_eq!(reason, "windows-hook-blocked");
        }
        LocalHookPromptDecision::Allow => panic!("blocking Hook was ignored"),
    }
}

#[tokio::test]
async fn attachment_only_prompt_is_valid_hook_context() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-1",
        vec![hook("prompt", "UserPromptSubmit", "prompt context")],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();
    let mut attachment_run = run("run-attachment-only");
    attachment_run.input["message"] = json!("");
    attachment_run.input["attachments"] = json!([{
        "attachment_id": "attachment-1",
        "authorized_local_ref": "local-attachment:1"
    }]);

    let context = coordinator
        .prefixed_context(&claim(attachment_run))
        .await
        .expect("attachment-only prompt context");

    assert_eq!(runner.events(), vec!["UserPromptSubmit"]);
    assert!(context.iter().any(|item| item["content"]
        .as_str()
        .is_some_and(|value| value.contains("prompt context"))));
}

#[tokio::test]
async fn session_context_rebuilds_for_process_resume_and_new_compaction() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-1",
        vec![
            hook("session", "SessionStart", "session context"),
            hook("post-compact", "PostCompact", "compact context"),
        ],
    )
    .await;
    let first = LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).unwrap();
    let initial_claim = claim(run("run-main"));
    first.prefixed_context(&initial_claim).await.unwrap();

    let resumed = LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).unwrap();
    resumed.prefixed_context(&initial_claim).await.unwrap();
    let mut compacted_run = run("run-main");
    compacted_run.checkpoint = json!({
        "response": {
            "request_input_items": [{"role": "user", "content": "hello"}],
            "response_output_items": [{
                "type": "compaction",
                "id": "cmp-1",
                "encrypted_content": "opaque",
            }]
        }
    });
    let compacted_claim = claim(compacted_run);
    resumed.prefixed_context(&compacted_claim).await.unwrap();
    resumed.prefixed_context(&compacted_claim).await.unwrap();
    let compact_marker = latest_compaction_marker(&compacted_claim.run)
        .unwrap()
        .expect("compact marker");
    let compact_sequence = stable_sequence(&["compact", "conversation-1", &compact_marker]);
    let post_compact_sequence =
        stable_sequence(&["post_compact", "conversation-1", &compact_marker]);
    let compact_events = resumed.events_for_claim(&compacted_claim).await.unwrap();
    assert_eq!(
        compact_events
            .iter()
            .find(|event| event.event_type == "SessionStart")
            .expect("compact SessionStart")
            .spec
            .sequence,
        compact_sequence
    );
    assert_eq!(
        compact_events
            .iter()
            .find(|event| event.event_type == "PostCompact")
            .expect("PostCompact")
            .spec
            .sequence,
        post_compact_sequence
    );
    assert_eq!(
        runtime
            .get_hook_event_for_host_worker(
                "user-1",
                "SessionStart",
                "conversation-1",
                compact_sequence,
            )
            .await
            .unwrap()
            .expect("compact event")
            .status,
        "completed"
    );
    assert_eq!(
        runtime
            .get_hook_event_for_host_worker(
                "user-1",
                "PostCompact",
                "conversation-1",
                post_compact_sequence,
            )
            .await
            .unwrap()
            .expect("PostCompact event")
            .status,
        "completed"
    );

    let after_restart = LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).unwrap();
    assert_ne!(
        resumed.process_session_sequence,
        after_restart.process_session_sequence
    );
    assert!(runtime
        .get_hook_event_for_host_worker(
            "user-1",
            "SessionStart",
            "conversation-1",
            after_restart.process_session_sequence,
        )
        .await
        .unwrap()
        .is_none());
    after_restart
        .prefixed_context(&compacted_claim)
        .await
        .unwrap();
    assert!(runtime
        .get_hook_event_for_host_worker(
            "user-1",
            "SessionStart",
            "conversation-1",
            after_restart.process_session_sequence,
        )
        .await
        .unwrap()
        .is_some());

    assert_eq!(
        runner.session_sources(),
        vec!["startup", "resume", "compact", "resume"]
    );
    assert_eq!(
        runner
            .events()
            .into_iter()
            .filter(|event| event == "PostCompact")
            .count(),
        1
    );
    let compact_payload = runner
        .payloads()
        .into_iter()
        .find(|payload| payload["hook_event_name"] == "PostCompact")
        .expect("PostCompact payload");
    assert_eq!(compact_payload["trigger"], "auto");
    assert!(compact_payload.get("pre_compact").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn matching_hooks_run_concurrently_but_merge_in_stable_bundle_order() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-z",
        vec![hook("prompt-z", "UserPromptSubmit", "delay:last")],
    )
    .await;
    activate_bundle(
        &storage,
        "bundle-a",
        vec![hook("prompt-a", "UserPromptSubmit", "delay:first")],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();
    let event = coordinator
        .events_for_claim(&claim(run("run-main")))
        .await
        .unwrap()
        .pop()
        .expect("prompt event");

    let outcome = coordinator.execute_event(event).await.unwrap();

    assert!(runner.maximum_active.load(Ordering::SeqCst) >= 2);
    let context = outcome.additional_context.expect("context");
    assert!(context.find("bundle-a").unwrap() < context.find("bundle-z").unwrap());
    assert!(context.find("first").unwrap() < context.find("last").unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn matching_hooks_respect_the_process_concurrency_limit() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-many",
        (0..12)
            .map(|index| {
                hook(
                    &format!("prompt-{index}"),
                    "UserPromptSubmit",
                    &format!("delay:{index}"),
                )
            })
            .collect(),
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();
    let event = coordinator
        .events_for_claim(&claim(run("run-limited")))
        .await
        .unwrap()
        .pop()
        .expect("prompt event");

    coordinator.execute_event(event).await.unwrap();

    assert_eq!(runner.maximum_active.load(Ordering::SeqCst), 8);
}

#[tokio::test]
async fn tool_hooks_deny_before_execution_update_input_and_preserve_host_approval() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-tools",
        vec![
            matched_hook("deny", "PreToolUse", "^dangerous$", "deny-tool"),
            matched_hook("update", "PreToolUse", "^read_file$", "update-tool"),
            matched_hook(
                "permission-deny",
                "PermissionRequest",
                "^write_file$",
                "permission-deny",
            ),
        ],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).unwrap();
    let persisted_run = run("run-tool-gate");
    storage
        .create_run(
            &command("create-run-tool-gate"),
            &persisted_run,
            "run-tool-gate-created",
        )
        .await
        .expect("create tool gate run");
    let run_claim = claim(persisted_run);
    let gated = coordinator
        .gate_tool_outcome(
            &run_claim,
            LocalAgentStepOutcome::WaitForTool {
                batch_id: "batch-1".to_string(),
                tool_calls: vec![
                    LocalAgentToolCall {
                        call_id: "call-denied".to_string(),
                        tool_name: "dangerous".to_string(),
                        arguments: json!({"value": 1}),
                        side_effecting: true,
                        requires_approval: true,
                    },
                    LocalAgentToolCall {
                        call_id: "call-updated".to_string(),
                        tool_name: "read_file".to_string(),
                        arguments: json!({"path": "original.txt"}),
                        side_effecting: false,
                        requires_approval: false,
                    },
                    LocalAgentToolCall {
                        call_id: "call-permission".to_string(),
                        tool_name: "write_file".to_string(),
                        arguments: json!({"path": "output.txt"}),
                        side_effecting: true,
                        requires_approval: true,
                    },
                ],
                checkpoint: json!({"response_id": "response-1"}),
            },
            &tool_definitions(),
        )
        .await
        .unwrap();
    let LocalAgentStepOutcome::WaitForTool { tool_calls, .. } = gated else {
        panic!("expected tool outcome")
    };
    assert!(!tool_calls[0].requires_approval);
    assert_eq!(tool_calls[1].arguments, json!({"path": "rewritten.txt"}));
    assert!(!tool_calls[2].requires_approval);

    let denied = coordinator
        .tool_preflight_outcome(&tool_invocation("run-tool-gate", &tool_calls[0]))
        .await
        .unwrap();
    assert!(matches!(denied, Some(LocalAgentToolOutcome::Failed { .. })));
    assert!(coordinator
        .tool_preflight_outcome(&tool_invocation("run-tool-gate", &tool_calls[1],))
        .await
        .unwrap()
        .is_none());
    let permission_denied = coordinator
        .tool_preflight_outcome(&tool_invocation("run-tool-gate", &tool_calls[2]))
        .await
        .unwrap();
    assert!(matches!(
        permission_denied,
        Some(LocalAgentToolOutcome::Failed { .. })
    ));
}
