// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[tokio::test]
async fn invalid_hook_updated_input_fails_closed_before_tool_persistence() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-invalid-update",
        vec![matched_hook(
            "invalid-update",
            "PreToolUse",
            "^read_file$",
            "invalid-update-tool",
        )],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).unwrap();
    let error = coordinator
        .gate_tool_outcome(
            &claim(run("run-invalid-update")),
            LocalAgentStepOutcome::WaitForTool {
                batch_id: "batch-invalid".to_string(),
                tool_calls: vec![LocalAgentToolCall {
                    call_id: "call-invalid".to_string(),
                    tool_name: "read_file".to_string(),
                    arguments: json!({"path": "original.txt"}),
                    side_effecting: false,
                    requires_approval: false,
                }],
                checkpoint: json!({"response_id": "response-invalid"}),
            },
            &tool_definitions(),
        )
        .await
        .expect_err("schema-invalid updatedInput must fail closed");

    assert!(error.contains("schema validation failed"), "{error}");
    assert!(error.contains("read_file"), "{error}");
}

#[tokio::test]
async fn denied_builtin_tool_never_reaches_its_executor() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-builtin-deny",
        vec![matched_hook(
            "deny-builtin",
            "PreToolUse",
            "^dangerous$",
            "deny-tool",
        )],
    )
    .await;
    let hooks = Arc::new(LocalHookCoordinator::new(runtime.clone(), runner, None).unwrap());
    let run_claim = create_claimed_run(&runtime, "run-builtin-deny").await;
    let gated = hooks
        .gate_tool_outcome(
            &run_claim,
            LocalAgentStepOutcome::WaitForTool {
                batch_id: "batch-builtin-deny".to_string(),
                tool_calls: vec![LocalAgentToolCall {
                    call_id: "call-builtin-deny".to_string(),
                    tool_name: "dangerous".to_string(),
                    arguments: json!({}),
                    side_effecting: true,
                    requires_approval: false,
                }],
                checkpoint: json!({"response_id": "response-builtin-deny"}),
            },
            &[tool_definition("dangerous")],
        )
        .await
        .expect("gate tool");
    runtime
        .try_handle(envelope(
            "commit-builtin-deny",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: run_claim.run.run_id,
                claim_token: run_claim.claim_token,
                expected_version: run_claim.run.version,
                outcome: gated,
            }),
        ))
        .await
        .expect("persist gated tool");
    let executions = Arc::new(AtomicUsize::new(0));
    let mut tools = LocalToolRegistry::new();
    tools
        .register(
            "dangerous",
            CountingToolExecutor {
                executions: Arc::clone(&executions),
            },
        )
        .expect("register tool");
    let scheduler = LocalToolScheduler::new(runtime, tools, "user-1", "tool-worker")
        .expect("scheduler")
        .with_hook_coordinator(hooks);

    let ToolSchedulerTick::Committed(result) = scheduler.run_once().await.expect("denial commit")
    else {
        panic!("expected committed denial")
    };

    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(result.invocation.status, LocalAgentToolStatus::Failed);
    assert!(result
        .invocation
        .error
        .as_deref()
        .is_some_and(|error| error.contains("Hook denied")));
}

#[tokio::test]
async fn denied_external_mcp_claim_carries_preflight_failure_to_native_worker() {
    let (storage, runtime, runner) = fixture().await;
    let tool_name = "external_mcp__search__query";
    activate_bundle(
        &storage,
        "bundle-mcp-deny",
        vec![matched_hook(
            "deny-mcp",
            "PreToolUse",
            "^external_mcp__search__query$",
            "deny-tool",
        )],
    )
    .await;
    let hooks = Arc::new(LocalHookCoordinator::new(runtime.clone(), runner, None).unwrap());
    let run_claim = create_claimed_run(&runtime, "run-mcp-deny").await;
    let gated = hooks
        .gate_tool_outcome(
            &run_claim,
            LocalAgentStepOutcome::WaitForTool {
                batch_id: "batch-mcp-deny".to_string(),
                tool_calls: vec![LocalAgentToolCall {
                    call_id: "call-mcp-deny".to_string(),
                    tool_name: tool_name.to_string(),
                    arguments: json!({}),
                    side_effecting: true,
                    requires_approval: false,
                }],
                checkpoint: json!({"response_id": "response-mcp-deny"}),
            },
            &[tool_definition(tool_name)],
        )
        .await
        .expect("gate MCP tool");
    runtime
        .try_handle(envelope(
            "commit-mcp-deny",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: run_claim.run.run_id,
                claim_token: run_claim.claim_token,
                expected_version: run_claim.run.version,
                outcome: gated,
            }),
        ))
        .await
        .expect("persist gated MCP tool");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(MAIN_CHAT_PROFILE_KEY, NeverModelProfile)
        .expect("profile");
    let model_scheduler =
        LocalAgentScheduler::new(runtime.clone(), profiles, "user-1", "model-worker")
            .expect("model scheduler");
    let host = LocalAgentHostCoordinator::new(runtime, "user-1", Some(model_scheduler), None)
        .expect("host")
        .with_hook_coordinator(hooks);

    let response = host
        .handle_request(envelope(
            "claim-mcp-deny",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "native-worker".to_string(),
                lease_duration_ms: 10_000,
                include_tool_names: None,
                exclude_tool_names: Vec::new(),
                execution_route: None,
            }),
        ))
        .await;
    let Some(HostResult::ToolClaim { claim: Some(claim) }) = response.result else {
        panic!("expected external MCP claim: {response:?}")
    };

    assert_eq!(
        claim.invocation.execution_route,
        LocalAgentToolExecutionRoute::Platform
    );
    let Some(LocalAgentToolOutcome::Failed { error, .. }) = claim.preflight_outcome else {
        panic!("native worker must receive the persisted Hook denial")
    };
    assert!(error.contains("Hook denied"), "{error}");
}

#[tokio::test]
async fn post_tool_use_preserves_tool_failure_and_user_rejection_contracts() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-post-failures",
        vec![matched_hook(
            "post-failures",
            "PostToolUse",
            "^(read_file|write_file)$",
            "post failure context",
        )],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();
    let mut continued = run("run-post-failures");
    continued.continuation_input = Some(json!({
        "type": "tool_results",
        "batch_id": "batch-failures",
        "invocations": [
            {
                "invocation_id": "invocation-tool-failed",
                "call_id": "call-tool-failed",
                "tool_name": "read_file",
                "arguments": {"path": "missing.txt"},
                "status": "failed",
                "error": "file not found"
            },
            {
                "invocation_id": "invocation-user-rejected",
                "call_id": "call-user-rejected",
                "tool_name": "write_file",
                "arguments": {"path": "output.txt"},
                "status": "failed",
                "error": "tool approval rejected by user",
                "approval_status": "rejected"
            }
        ]
    }));
    let original = continued.continuation_input.clone();

    let context = coordinator
        .prefixed_context(&claim(continued.clone()))
        .await
        .expect("post failure context");

    let payloads = runner
        .payloads()
        .into_iter()
        .filter(|payload| payload["hook_event_name"] == "PostToolUse")
        .collect::<Vec<_>>();
    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[0]["tool_response"]["status"], "failed");
    assert_eq!(payloads[0]["tool_response"]["error"], "file not found");
    assert_eq!(payloads[1]["tool_response"]["status"], "failed");
    assert_eq!(
        payloads[1]["tool_response"]["error"],
        "tool approval rejected by user"
    );
    assert!(combined_context(&context).contains("post failure context"));
    assert_eq!(continued.continuation_input, original);
}

#[tokio::test]
async fn stop_hook_blocks_before_success_commit_and_repeated_reason_needs_review() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(&storage, "bundle-stop", vec![hook("stop", "Stop", "block")]).await;
    runtime
        .try_handle(envelope(
            "create-stop-run",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-stop".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation_turn".to_string(),
                owner_entity_id: "turn-stop".to_string(),
                profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({
                    "conversation_id": "conversation-stop",
                    "turn_id": "turn-stop",
                    "message": "finish safely",
                }),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("create stop run");
    let hooks =
        Arc::new(LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).expect("hooks"));
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(MAIN_CHAT_PROFILE_KEY, FinalModelProfile)
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(runtime, profiles, "user-1", "model-worker")
        .expect("scheduler")
        .with_hook_coordinator(hooks);

    let SchedulerTick::Committed(first) = scheduler.run_once().await.expect("first block") else {
        panic!("expected first block commit")
    };
    assert_eq!(first.status, LocalAgentRunStatus::ContinuationReady);
    assert!(first.terminal_outcome.is_none());
    assert_eq!(
        first.checkpoint["continuation"]["stop_hook_block"]["count"],
        1
    );

    let SchedulerTick::Committed(second) = scheduler.run_once().await.expect("repeat block") else {
        panic!("expected repeat block commit")
    };
    assert_eq!(second.status, LocalAgentRunStatus::NeedsReview);
    assert_eq!(
        second.terminal_outcome.as_ref().expect("review diagnostic")["error"],
        "trusted plugin Hook repeatedly blocked successful completion"
    );
    assert_eq!(runner.events(), vec!["Stop", "Stop"]);
}

#[tokio::test]
async fn plan_first_policy_rejects_unrouted_main_chat_completion() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle_with_policy(
        &storage,
        "bundle-plan-first",
        vec![hook("session", "SessionStart", "context")],
        Some(plan_first_policy()),
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).expect("hooks");
    let output = json!({"content": "reply here", "response_output_items": []});

    let gated = coordinator
        .gate_success_outcome(
            &claim(run("run-unrouted")),
            LocalAgentStepOutcome::Succeed { output },
        )
        .await
        .expect("workflow gate");

    let LocalAgentStepOutcome::Continue { checkpoint } = gated else {
        panic!("unrouted completion must continue")
    };
    assert_eq!(
        checkpoint["continuation"]["reason"],
        "task_route_decision_required"
    );
    assert!(checkpoint["continuation"]["input_items"][0]["content"]
        .as_str()
        .expect("gate guidance")
        .contains("decide_task_route"));
}

#[tokio::test]
async fn plan_first_policy_allows_an_audited_direct_response() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle_with_policy(
        &storage,
        "bundle-plan-direct",
        vec![hook("session", "SessionStart", "context")],
        Some(plan_first_policy()),
    )
    .await;
    let routed = run_with_successful_tool(
        &runtime,
        "run-direct-route",
        DECIDE_TASK_ROUTE_TOOL,
        json!({"route": "direct_response", "reason": "ordinary factual answer"}),
    )
    .await;
    let coordinator =
        LocalHookCoordinator::new(runtime, runner, None).expect("workflow coordinator");
    let output = json!({"content": "direct answer", "response_output_items": []});

    let gated = coordinator
        .gate_success_outcome(
            &claim(routed),
            LocalAgentStepOutcome::Succeed {
                output: output.clone(),
            },
        )
        .await
        .expect("direct route gate");

    assert_eq!(gated, LocalAgentStepOutcome::Succeed { output });
}

#[tokio::test]
async fn planning_task_cannot_replace_survey_or_plan_save_with_plain_text() {
    let (_storage, runtime, runner) = fixture().await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).expect("hooks");
    let mut planning = run("run-planning-plain-text");
    planning.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    planning.owner_entity_type = "task".to_string();
    planning.owner_entity_id = "task-planning".to_string();
    planning.input = json!({
        "task_kind": "planning",
        "source_conversation_id": "conversation-1",
    });

    let gated = coordinator
        .gate_success_outcome(
            &claim(planning),
            LocalAgentStepOutcome::Succeed {
                output: json!({"content": "please answer these questions"}),
            },
        )
        .await
        .expect("planning completion gate");

    let LocalAgentStepOutcome::Continue { checkpoint } = gated else {
        panic!("planning prose must not finish")
    };
    assert_eq!(
        checkpoint["continuation"]["reason"],
        "planning_task_requires_plan"
    );
    let guidance = checkpoint["continuation"]["input_items"][0]["content"]
        .as_str()
        .expect("planning guidance");
    assert!(guidance.contains("requirement_survey_create"));
    assert!(guidance.contains("requirement_plan_save"));
}

#[tokio::test]
async fn stop_hook_distinct_blocks_are_bounded_before_an_infinite_loop() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-changing-stop",
        vec![hook("stop", "Stop", "block-changing")],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).expect("hooks");
    let mut candidate = run("run-changing-stop");
    candidate.iteration = 1;
    let output = json!({
        "content": "candidate",
        "response_output_items": [],
        "request_input_items": []
    });

    for expected_count in 1..MAXIMUM_STOP_BLOCKS {
        let gated = coordinator
            .gate_success_outcome(
                &claim(candidate.clone()),
                LocalAgentStepOutcome::Succeed {
                    output: output.clone(),
                },
            )
            .await
            .expect("bounded stop gate");
        let LocalAgentStepOutcome::Continue { checkpoint } = gated else {
            panic!("block {expected_count} should continue")
        };
        assert_eq!(
            checkpoint["continuation"]["stop_hook_block"]["count"],
            expected_count
        );
        candidate.checkpoint = checkpoint;
        candidate.iteration += 1;
    }

    let third = coordinator
        .gate_success_outcome(&claim(candidate), LocalAgentStepOutcome::Succeed { output })
        .await
        .expect("maximum stop gate");
    let LocalAgentStepOutcome::NeedsReview { detail, .. } = third else {
        panic!("maximum stop block must require review")
    };
    assert_eq!(detail["stop_block_count"], MAXIMUM_STOP_BLOCKS);
    assert_eq!(detail["repeated_reason"], false);
}

#[tokio::test]
async fn delegated_success_uses_subagent_stop_and_non_success_skips_terminal_hooks() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-subagent-stop",
        vec![
            hook("stop", "Stop", "block"),
            hook("subagent-stop", "SubagentStop", "block"),
        ],
    )
    .await;
    let parent = run("parent-stop-run");
    storage
        .create_run(&command("create-parent-stop"), &parent, "parent-stop-event")
        .await
        .expect("create parent");
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).expect("hooks");
    let mut task = run("task-stop-run");
    task.owner_entity_type = "task".to_string();
    task.owner_entity_id = "task-stop".to_string();
    task.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    task.iteration = 1;
    task.input = json!({
        "prompt": "work",
        "source_run_id": "parent-stop-run",
        "source_conversation_id": "conversation-1",
    });

    let failed = LocalAgentStepOutcome::Fail {
        error: "model failed".to_string(),
        detail: Value::Null,
    };
    assert_eq!(
        coordinator
            .gate_success_outcome(&claim(task.clone()), failed.clone())
            .await
            .expect("non-success gate"),
        failed
    );
    assert!(runner.events().is_empty());

    let gated = coordinator
        .gate_success_outcome(
            &claim(task),
            LocalAgentStepOutcome::Succeed {
                output: json!({
                    "content": "done",
                    "response_output_items": [],
                    "request_input_items": []
                }),
            },
        )
        .await
        .expect("subagent stop gate");
    assert!(matches!(gated, LocalAgentStepOutcome::Continue { .. }));
    assert_eq!(runner.events(), vec!["SubagentStop"]);
    let payload = runner.payloads().pop().expect("SubagentStop payload");
    assert_eq!(payload["agent_id"], "task-stop-run");
    assert_eq!(payload["agent_type"], "task_execution");
}
