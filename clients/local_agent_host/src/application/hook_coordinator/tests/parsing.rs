// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[tokio::test]
async fn async_hook_returns_immediately_and_delivers_context_on_the_next_model_iteration() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-async",
        vec![async_hook(
            "interrupt-async",
            "Interrupt",
            "long-delay:background result",
        )],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).expect("hooks");
    let event = HookEvent::new_for_owner_with_sequence(
        "user-1",
        "Interrupt",
        "run-async-source",
        8,
        "",
        json!({
            "session_id": "conversation-1",
            "hook_event_name": "Interrupt",
            "run_id": "run-async-source",
            "reason": "process interrupted",
            "cwd": "",
        }),
    )
    .expect("async event");
    let immediate = coordinator
        .execute_event(event)
        .await
        .expect("enqueue async Hook");
    assert!(!immediate.blocked);
    assert!(!immediate.failed);
    assert!(immediate.additional_context.is_none());

    tokio::time::timeout(Duration::from_secs(1), async {
        while runner.active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("async Hook starts");
    assert_eq!(runner.active.load(Ordering::SeqCst), 1);

    let mut delivery_run = run("run-async-delivery");
    let delivery_claim = claim(delivery_run.clone());
    let before_completion = coordinator
        .prefixed_context(&delivery_claim)
        .await
        .expect("context while async Hook runs");
    assert!(!combined_context(&before_completion).contains("background result"));

    let delivered = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let items = coordinator
                .prefixed_context(&delivery_claim)
                .await
                .expect("claim async context");
            if combined_context(&items).contains("background result") {
                break items;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("async context delivery");
    assert!(combined_context(&delivered).contains("trusted_async_plugin_hook_context"));
    let replayed = coordinator
        .prefixed_context(&delivery_claim)
        .await
        .expect("same model iteration replays claimed context");
    assert!(combined_context(&replayed).contains("background result"));

    delivery_run.iteration += 1;
    let next_iteration = coordinator
        .prefixed_context(&claim(delivery_run))
        .await
        .expect("next model iteration");
    assert!(!combined_context(&next_iteration).contains("background result"));
    assert_eq!(runner.events(), vec!["Interrupt"]);
}

#[test]
fn hook_updated_input_requires_an_exact_valid_model_visible_schema() {
    let definitions = tool_definitions();
    validate_tool_input("dangerous", &json!({"value": 1}), definitions.as_slice())
        .expect("flat tool schema");
    validate_tool_input(
        "read_file",
        &json!({"path": "README.md"}),
        definitions.as_slice(),
    )
    .expect("nested tool schema");

    let missing = validate_tool_input("unknown", &json!({}), definitions.as_slice())
        .expect_err("missing tool must fail closed");
    assert!(missing.contains("not present"), "{missing}");

    let invalid_schema = [json!({
        "name": "broken",
        "parameters": {"type": 7}
    })];
    let invalid = validate_tool_input("broken", &json!({}), &invalid_schema)
        .expect_err("invalid schema must fail closed");
    assert!(invalid.contains("schema is invalid"), "{invalid}");
}

#[tokio::test]
async fn post_tool_use_context_is_injected_without_replacing_the_real_result() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-post",
        vec![matched_hook(
            "post",
            "PostToolUse",
            "^read_file$",
            "post context",
        )],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner, None).unwrap();
    let mut continued = run("run-post");
    continued.continuation_input = Some(json!({
        "type": "tool_results",
        "batch_id": "batch-1",
        "invocations": [{
            "invocation_id": "invocation-1",
            "call_id": "call-1",
            "tool_name": "read_file",
            "arguments": {"path": "README.md"},
            "status": "succeeded",
            "result": {"content": "immutable real result"},
        }],
    }));

    let context = coordinator
        .prefixed_context(&claim(continued.clone()))
        .await
        .unwrap();

    assert!(combined_context(&context).contains("post context"));
    assert_eq!(
        continued.continuation_input.as_ref().unwrap()["invocations"][0]["result"],
        json!({"content": "immutable real result"})
    );
}

#[tokio::test]
async fn post_tool_use_redacts_large_inline_media_before_persisting_the_event() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-post-media",
        vec![matched_hook(
            "post-media",
            "PostToolUse",
            "^capability_invoke$",
            "post media context",
        )],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();
    let screenshot = "a".repeat(1_340_288);
    let mut continued = run("run-post-media");
    continued.continuation_input = Some(json!({
        "type": "tool_results",
        "batch_id": "batch-media",
        "invocations": [{
            "invocation_id": "invocation-media",
            "call_id": "call-media",
            "tool_name": "capability_invoke",
            "arguments": {},
            "status": "succeeded",
            "result": {
                "content": {
                    "_meta": {
                        "chatos/artifacts": [{
                            "artifact": {
                                "artifact_id": "artifact-screenshot",
                                "sha256": "artifact-sha256",
                                "size_bytes": 3_193_997
                            }
                        }]
                    },
                    "content": [
                        {"type": "text", "text": "screenshot captured"},
                        {"type": "image", "data": screenshot, "mimeType": "image/jpeg"}
                    ]
                },
                "is_error": false,
                "made_progress": true
            }
        }]
    }));

    let context = coordinator
        .prefixed_context(&claim(continued.clone()))
        .await
        .expect("large screenshot Hook event");

    assert!(combined_context(&context).contains("post media context"));
    let payload = runner
        .payloads()
        .into_iter()
        .find(|payload| payload["hook_event_name"] == "PostToolUse")
        .expect("PostToolUse payload");
    assert!(serde_json::to_vec(&payload).unwrap().len() < 1024 * 1024);
    assert_eq!(
        payload["tool_response"]["content"]["_meta"]["chatos/artifacts"][0]["artifact"]
            ["artifact_id"],
        "artifact-screenshot"
    );
    let image_data = payload["tool_response"]["content"]["content"][1]["data"]
        .as_str()
        .expect("redacted image data");
    assert!(image_data.contains("inline binary omitted"), "{image_data}");
    assert!(!image_data.contains(&"a".repeat(8_192)));
    assert_eq!(
        continued.continuation_input.as_ref().unwrap()["invocations"][0]["result"]["content"]
            ["content"][1]["data"]
            .as_str()
            .unwrap()
            .len(),
        1_340_288
    );
}

#[test]
fn oversized_non_binary_tool_response_uses_a_bounded_integrity_summary() {
    let response = json!({
        "status": "succeeded",
        "is_error": false,
        "content": "x".repeat(MAXIMUM_INLINE_HOOK_TOOL_RESPONSE_BYTES + 1),
    });

    let bounded = hook_tool_response(&response);

    assert_eq!(bounded["status"], "succeeded");
    assert_eq!(bounded["is_error"], false);
    assert_eq!(bounded["_chatos_hook_response"]["omitted"], true);
    assert_eq!(
        bounded["_chatos_hook_response"]["sha256"],
        sha256(&serde_json::to_vec(&response).unwrap())
    );
    assert!(serde_json::to_vec(&bounded).unwrap().len() < 1_024);
}

#[tokio::test]
async fn only_valid_delegated_tasks_emit_subagent_start_once() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-1",
        vec![hook("subagent", "SubagentStart", "task context")],
    )
    .await;
    let parent = run("parent-run");
    storage
        .create_run(&command("create-parent"), &parent, "parent-event")
        .await
        .expect("create parent");
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).unwrap();

    let ordinary = claim(run("ordinary-main"));
    assert!(coordinator
        .prefixed_context(&ordinary)
        .await
        .unwrap()
        .is_empty());
    let mut task = run("task-run");
    task.owner_entity_type = "task".to_string();
    task.owner_entity_id = "task-1".to_string();
    task.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    task.input = json!({
        "prompt": "work",
        "source_run_id": "parent-run",
        "source_conversation_id": "conversation-1",
    });
    let task_claim = claim(task);

    let first = coordinator.prefixed_context(&task_claim).await.unwrap();
    let second = coordinator.prefixed_context(&task_claim).await.unwrap();

    assert_eq!(runner.events(), vec!["SubagentStart"]);
    assert_eq!(first, second);
    assert!(first[0]["content"]
        .as_str()
        .unwrap()
        .contains("task context"));
}

#[test]
fn successful_silent_hook_output_is_a_valid_continue_decision() {
    let hook = hook("silent", "UserPromptSubmit", "silent");
    let result = LocalHookInvocationResult {
        exit_code: Some(0),
        stdout: b" \n\t".to_vec(),
        stderr: Vec::new(),
        stdout_overflow: false,
        stderr_overflow: false,
        timed_out: false,
        duration_ms: 1,
    };

    let completion = completion_from_result("UserPromptSubmit", &hook, &result);

    assert_eq!(completion.status, "succeeded");
    assert!(completion.additional_context.is_none());
}

#[test]
fn async_hook_delivers_only_context_fields_and_ignores_control_decisions() {
    let hook = async_hook("async-output", "PreToolUse", "async-output");
    let result = LocalHookInvocationResult {
            exit_code: Some(0),
            stdout: b"background log\n{\"continue\":false,\"systemMessage\":\"system result\",\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"additionalContext\":\"async result\",\"permissionDecision\":\"deny\"}}\n"
                .to_vec(),
            stderr: Vec::new(),
            stdout_overflow: false,
            stderr_overflow: false,
            timed_out: false,
            duration_ms: 1,
        };

    let completion = completion_from_result("PreToolUse", &hook, &result);

    assert_eq!(completion.status, "succeeded");
    let output = decode_invocation_output(completion.additional_context.as_deref());
    assert_eq!(output.additional_context.as_deref(), Some("async result"));
    assert_eq!(output.system_message.as_deref(), Some("system result"));
    assert!(output.permission_decision.is_none());
    assert!(output.updated_input.is_none());
}

#[test]
fn overflowed_hook_output_fails_with_a_safe_summary_and_redacted_spill() {
    let hook = hook("overflow", "UserPromptSubmit", "overflow");
    let result = LocalHookInvocationResult {
        exit_code: Some(0),
        stdout: vec![b'x'; 512 * 1_024],
        stderr: b"private stderr must not become the summary".to_vec(),
        stdout_overflow: true,
        stderr_overflow: false,
        timed_out: false,
        duration_ms: 5,
    };

    let completion = completion_from_result("UserPromptSubmit", &hook, &result);

    assert_eq!(completion.status, "failed");
    assert_eq!(
        completion.stderr_summary.as_deref(),
        Some("Hook stdout exceeded the 524288-byte capture limit")
    );
    assert!(completion.stdout_spill.is_some());
    assert!(!format!("{:?}", completion.stdout_spill).contains(&"x".repeat(64)));
    assert!(!completion
        .stderr_summary
        .as_deref()
        .unwrap()
        .contains("private stderr"));
}
