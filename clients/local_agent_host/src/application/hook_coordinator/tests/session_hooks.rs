// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[tokio::test]
async fn invalid_stop_output_is_diagnostic_and_does_not_fabricate_a_block() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-invalid-stop",
        vec![hook("stop", "Stop", "invalid")],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).expect("hooks");
    let output = json!({
        "content": "done",
        "response_output_items": [],
        "request_input_items": []
    });

    let gated = coordinator
        .gate_success_outcome(
            &claim(run("run-invalid-stop")),
            LocalAgentStepOutcome::Succeed {
                output: output.clone(),
            },
        )
        .await
        .expect("invalid Stop output is a recorded diagnostic");

    assert_eq!(gated, LocalAgentStepOutcome::Succeed { output });
    assert_eq!(runner.events(), vec!["Stop"]);
}

#[tokio::test]
async fn interrupt_is_enqueued_only_after_cancellation_and_cannot_restore_the_run() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-interrupt",
        vec![hook("interrupt", "Interrupt", "block")],
    )
    .await;
    runtime
        .try_handle(envelope(
            "create-interrupt-run",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-interrupt".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation_turn".to_string(),
                owner_entity_id: "turn-interrupt".to_string(),
                profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({
                    "conversation_id": "conversation-interrupt",
                    "turn_id": "turn-interrupt",
                    "message": "work",
                }),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("create interrupt run");
    let hooks =
        Arc::new(LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).expect("hooks"));
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(MAIN_CHAT_PROFILE_KEY, NeverModelProfile)
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(runtime.clone(), profiles, "user-1", "model-worker")
        .expect("scheduler");
    let host = LocalAgentHostCoordinator::new(runtime.clone(), "user-1", Some(scheduler), None)
        .expect("host")
        .with_hook_coordinator(hooks);

    let response = host
        .handle_request(envelope(
            "cancel-interrupt-run",
            HostCommand::CancelRun(CancelRunCommand {
                owner_user_id: "user-1".to_string(),
                run_id: "run-interrupt".to_string(),
                expected_version: Some(1),
                reason: "cancelled by user".to_string(),
            }),
        ))
        .await;
    let Some(HostResult::Run { run: cancelled }) = response.result.as_ref() else {
        panic!("expected cancelled run: {response:?}")
    };
    assert_eq!(cancelled.status, LocalAgentRunStatus::Cancelled);

    tokio::time::timeout(Duration::from_secs(1), async {
        while !runner.events().iter().any(|event| event == "Interrupt") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Interrupt Hook completion");
    let persisted = runtime
        .get_run_for_host_worker("run-interrupt")
        .await
        .expect("get cancelled run")
        .expect("cancelled run");
    assert_eq!(persisted.status, LocalAgentRunStatus::Cancelled);
    let interrupt = runner
        .payloads()
        .into_iter()
        .find(|payload| payload["hook_event_name"] == "Interrupt")
        .expect("Interrupt payload");
    assert_eq!(interrupt["reason"], "cancelled by user");
}

#[tokio::test]
async fn session_end_is_durable_before_background_delivery_and_cannot_block_shutdown() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-session-end",
        vec![hook("session-end", "SessionEnd", "block")],
    )
    .await;
    let coordinator =
        Arc::new(LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).expect("hooks"));
    coordinator
        .prefixed_context(&claim(run("run-session-end")))
        .await
        .expect("establish session");

    assert_eq!(
        coordinator
            .enqueue_session_end("user-1", "shutdown")
            .await
            .expect("enqueue SessionEnd"),
        1
    );
    let sequence = coordinator.process_session_sequence;
    let pending_or_completed = runtime
        .get_hook_event_for_host_worker("user-1", "SessionEnd", "conversation-1", sequence)
        .await
        .expect("read durable SessionEnd")
        .expect("SessionEnd persisted before enqueue returns");
    assert!(matches!(
        pending_or_completed.status.as_str(),
        "pending" | "completed"
    ));

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let event = runtime
                .get_hook_event_for_host_worker("user-1", "SessionEnd", "conversation-1", sequence)
                .await
                .expect("read SessionEnd")
                .expect("SessionEnd");
            if event.status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("SessionEnd delivery");
    assert_eq!(
        runner
            .events()
            .into_iter()
            .filter(|event| event == "SessionEnd")
            .count(),
        1
    );
    let payload = runner
        .payloads()
        .into_iter()
        .find(|payload| payload["hook_event_name"] == "SessionEnd")
        .expect("SessionEnd payload");
    assert_eq!(payload["reason"], "shutdown");
    assert_eq!(
        coordinator
            .enqueue_session_end("user-1", "shutdown")
            .await
            .expect("completed SessionEnd replay"),
        0
    );
}

#[tokio::test]
async fn idle_session_end_waits_for_zero_clients_and_reopens_with_a_new_generation() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-idle-session",
        vec![
            hook("session-start", "SessionStart", "session context"),
            hook("session-end", "SessionEnd", "session cleanup"),
        ],
    )
    .await;
    let coordinator =
        Arc::new(LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).expect("hooks"));
    let session_claim = claim(run("run-idle-session"));
    coordinator
        .prefixed_context(&session_claim)
        .await
        .expect("establish session");
    let first_sequence = coordinator.process_session_sequence;
    coordinator
        .active_sessions
        .lock()
        .expect("active sessions")
        .get_mut("conversation-1")
        .expect("session")
        .last_active_at = Instant::now() - Duration::from_secs(31 * 60);

    coordinator.client_connected();
    assert_eq!(
        coordinator
            .expire_idle_sessions("user-1", Instant::now(), Duration::from_secs(30 * 60),)
            .await
            .expect("connected session remains active"),
        0
    );
    coordinator.client_disconnected();
    assert_eq!(
        coordinator
            .expire_idle_sessions("user-1", Instant::now(), Duration::from_secs(30 * 60),)
            .await
            .expect("expire idle session"),
        1
    );
    let persisted = runtime
        .get_hook_event_for_host_worker("user-1", "SessionEnd", "conversation-1", first_sequence)
        .await
        .expect("read idle SessionEnd")
        .expect("idle SessionEnd is durable");
    assert!(matches!(persisted.status.as_str(), "pending" | "completed"));

    tokio::time::timeout(Duration::from_secs(1), async {
        while !runner.events().iter().any(|event| event == "SessionEnd") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle SessionEnd delivery");
    let payload = runner
        .payloads()
        .into_iter()
        .find(|payload| payload["hook_event_name"] == "SessionEnd")
        .expect("idle SessionEnd payload");
    assert_eq!(payload["reason"], "idle_timeout");

    let reopened = coordinator
        .events_for_claim(&session_claim)
        .await
        .expect("reopen session");
    let reopened_sequence = reopened
        .iter()
        .find(|event| event.event_type == "SessionStart")
        .expect("reopened SessionStart")
        .spec
        .sequence;
    assert_ne!(reopened_sequence, first_sequence);
    coordinator
        .prefixed_context(&session_claim)
        .await
        .expect("execute reopened session");
    assert_eq!(
        runner
            .events()
            .into_iter()
            .filter(|event| event == "SessionStart")
            .count(),
        2
    );
    assert_eq!(runner.session_sources(), vec!["startup", "resume"]);
}

#[tokio::test]
async fn pending_event_recovers_with_its_event_time_bundle_snapshot() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-original",
        vec![hook("interrupt-original", "Interrupt", "original context")],
    )
    .await;
    let first =
        Arc::new(LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).expect("hooks"));
    let mut event = HookEvent::new_for_owner_with_sequence(
        "user-1",
        "Interrupt",
        "run-recovery",
        2,
        "",
        json!({
            "session_id": "conversation-recovery",
            "hook_event_name": "Interrupt",
            "run_id": "run-recovery",
            "reason": "process interrupted",
            "cwd": "",
        }),
    )
    .expect("recovery event");
    let prepared = first
        .persist_event_snapshot(&mut event)
        .await
        .expect("persist pending event");
    assert_eq!(prepared.status, "pending");
    assert_eq!(
        prepared.spec.active_bundle_ids,
        vec!["bundle-original".to_string()]
    );
    activate_bundle(
        &storage,
        "bundle-new",
        vec![hook("interrupt-new", "Interrupt", "new context")],
    )
    .await;
    storage
        .set_active_plugin_hook_bundles(
            &command("activate-only-new"),
            "user-1",
            &["bundle-new".to_string()],
            10_100,
        )
        .await
        .expect("replace active bundles");

    let recovered = Arc::new(
        LocalHookCoordinator::new(runtime.clone(), runner.clone(), None).expect("recovered"),
    );
    assert_eq!(
        recovered
            .recover_pending_events("user-1")
            .await
            .expect("recover pending events"),
        1
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let record = runtime
                .get_hook_event_for_host_worker("user-1", "Interrupt", "run-recovery", 2)
                .await
                .expect("read recovery event")
                .expect("recovery event");
            if record.status == "completed" {
                let outcome = decode_persisted_outcome("Interrupt", record.additional_context)
                    .expect("decode recovery outcome");
                assert!(outcome
                    .additional_context
                    .as_deref()
                    .is_some_and(|context| context.contains("original context")));
                assert!(!outcome
                    .additional_context
                    .as_deref()
                    .is_some_and(|context| context.contains("new context")));
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("pending event recovery");
    assert_eq!(runner.events(), vec!["Interrupt"]);
}

#[tokio::test]
async fn concurrent_event_replay_executes_each_hook_only_once_in_process() {
    let (storage, runtime, runner) = fixture().await;
    activate_bundle(
        &storage,
        "bundle-concurrent-replay",
        vec![hook(
            "interrupt-concurrent-replay",
            "Interrupt",
            "delay:replayed once",
        )],
    )
    .await;
    let coordinator = LocalHookCoordinator::new(runtime, runner.clone(), None).expect("hooks");
    let replay_event = || {
        HookEvent::new_for_owner_with_sequence(
            "user-1",
            "Interrupt",
            "run-concurrent-replay",
            7,
            "",
            json!({
                "session_id": "conversation-concurrent-replay",
                "hook_event_name": "Interrupt",
                "run_id": "run-concurrent-replay",
                "reason": "process interrupted",
                "cwd": "",
            }),
        )
        .expect("replay event")
    };

    let (first, second) = tokio::join!(
        coordinator.execute_event(replay_event()),
        coordinator.execute_event(replay_event())
    );
    for outcome in [first.expect("first replay"), second.expect("second replay")] {
        assert!(outcome
            .additional_context
            .as_deref()
            .is_some_and(|context| context.contains("replayed once")));
    }
    assert_eq!(runner.events(), vec!["Interrupt"]);
}
