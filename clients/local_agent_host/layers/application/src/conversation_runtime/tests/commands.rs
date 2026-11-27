#[tokio::test]
async fn host_start_turn_creates_missing_conversation_in_the_same_command() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    let mut turn = start_turn(1);
    turn.conversation_if_missing = Some(create_conversation());

    let started = runtime
        .try_handle(request(
            "start-missing-conversation-turn",
            HostCommand::StartConversationTurn(turn),
        ))
        .await
        .expect("create conversation and start Turn");
    let HostResult::ConversationTurnStarted { result } = started else {
        panic!("unexpected start result")
    };
    assert_eq!(result.conversation.conversation_id, "conversation-1");
    assert_eq!(result.conversation.version, 2);
    assert_eq!(result.turn.status, LocalConversationTurnStatus::Running);
}

#[tokio::test]
async fn host_routes_local_conversation_and_atomic_turn_start() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");

    let created = runtime
        .try_handle(request(
            "create-conversation-1",
            HostCommand::CreateConversation(create_conversation()),
        ))
        .await
        .expect("create conversation");
    let HostResult::Conversation { conversation } = created else {
        panic!("unexpected create result")
    };
    assert_eq!(conversation.conversation.version, 1);

    let started = runtime
        .try_handle(request(
            "start-turn-1",
            HostCommand::StartConversationTurn(start_turn(1)),
        ))
        .await
        .expect("start Turn");
    let HostResult::ConversationTurnStarted { result } = started else {
        panic!("unexpected start result")
    };
    assert_eq!(result.conversation.version, 2);
    assert_eq!(result.turn.status, LocalConversationTurnStatus::Running);
    assert_eq!(result.message.role, LocalConversationMessageRole::User);
    assert_eq!(result.attachments.len(), 1);
    assert_eq!(result.attachments[0].attachment_id, "attachment-1");
    assert_eq!(result.run.owner_entity_type, "conversation_turn");
    assert_eq!(result.run.owner_entity_id, "turn-1");
    assert_eq!(result.run.profile_key, "main_chat");
    assert_eq!(result.run.input["agent_id"], "jiguli");
    assert_eq!(result.run.input["agent_name"], "叽咕狸");
    assert_eq!(result.run.input["conversation_id"], "conversation-1");
    assert_eq!(result.run.input["turn_id"], "turn-1");
    assert_eq!(result.run.input["message_id"], "message-1");
    assert_eq!(
        result.run.input["explicit_capability_selection"],
        json!({
            "kind": "plugin",
            "plugin_key": "document-plugin",
            "component_key": "document-mcp",
            "display_name": "Document"
        })
    );
    assert_eq!(
        result.run.input["attachments"][0]["attachment_id"],
        "attachment-1"
    );

    let loaded = runtime
        .try_handle(request(
            "get-conversation-1",
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
            }),
        ))
        .await
        .expect("get conversation");
    assert!(matches!(
        loaded,
        HostResult::Conversation { conversation }
            if conversation.turns.len() == 1
                && conversation.messages.len() == 1
                && conversation.attachments.len() == 1
    ));

    let listed = runtime
        .try_handle(request(
            "list-conversation-1",
            HostCommand::ListConversations(ListConversationsCommand {
                owner_user_id: "user-1".to_string(),
                before_updated_at_unix_ms: None,
                before_conversation_id: None,
                limit: 10,
            }),
        ))
        .await
        .expect("list conversations");
    assert!(matches!(
        listed,
        HostResult::Conversations { page } if page.conversations.len() == 1
    ));

    let history_error = runtime
        .try_handle(request(
            "get-conversation-history-1",
            HostCommand::GetConversationHistory(GetConversationHistoryCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                before_cursor: None,
                limit: 10,
            }),
        ))
        .await
        .expect_err("conversation history must not fall back to local SQLite");
    assert!(history_error
        .to_string()
        .contains("Memory transcript source is required"));
}

#[tokio::test]
async fn host_routes_conversation_resume_and_stop_with_version_checks() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(request(
            "create-conversation-control",
            HostCommand::CreateConversation(create_conversation()),
        ))
        .await
        .expect("create conversation");
    runtime
        .try_handle(request(
            "start-turn-control",
            HostCommand::StartConversationTurn(start_turn(1)),
        ))
        .await
        .expect("start Turn");
    let claim = runtime
        .try_handle(request(
            "claim-turn-control",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim Run");
    let HostResult::Claim { claim: Some(claim) } = claim else {
        panic!("unexpected claim result")
    };
    let waiting = runtime
        .try_handle(request(
            "wait-turn-control",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForUser {
                    prompt: json!({"question": "continue?"}),
                    checkpoint: json!({"response_id": "response-1"}),
                },
            }),
        ))
        .await
        .expect("wait for user");
    let HostResult::Run { run: waiting } = waiting else {
        panic!("unexpected wait result")
    };

    let resumed = runtime
        .try_handle(request(
            "resume-turn-control",
            HostCommand::ResumeConversationTurn(ResumeConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                expected_conversation_version: 2,
                turn_id: "turn-1".to_string(),
                expected_run_version: waiting.version,
                expected_run_status: LocalAgentRunStatus::WaitingUser,
                message_id: "message-2".to_string(),
                message: "continue".to_string(),
                message_metadata: json!({"source": "ask_user"}),
                attachments: Vec::new(),
                reason: "user replied".to_string(),
            }),
        ))
        .await
        .expect("resume Turn");
    let HostResult::ConversationTurnUpdated { result: resumed } = resumed else {
        panic!("unexpected resume result")
    };
    assert_eq!(resumed.conversation.version, 3);
    assert_eq!(resumed.run.status, LocalAgentRunStatus::ContinuationReady);
    assert_eq!(
        resumed
            .run
            .continuation_input
            .as_ref()
            .expect("continuation")["input"]["message"],
        "continue"
    );

    let cancelled = runtime
        .try_handle(request(
            "cancel-turn-control",
            HostCommand::CancelConversationTurn(CancelConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                expected_conversation_version: resumed.conversation.version,
                turn_id: "turn-1".to_string(),
                expected_run_version: Some(resumed.run.version),
                reason: "user stopped".to_string(),
            }),
        ))
        .await
        .expect("cancel Turn");
    let HostResult::ConversationTurnUpdated { result: cancelled } = cancelled else {
        panic!("unexpected cancel result")
    };
    assert_eq!(cancelled.conversation.version, 4);
    assert_eq!(cancelled.run.status, LocalAgentRunStatus::Cancelled);
    assert_eq!(
        cancelled.turn.status,
        LocalConversationTurnStatus::Cancelled
    );
    assert!(cancelled.message.is_none());
}

#[tokio::test]
async fn profile_failure_reaches_conversation_history_with_the_specific_error() {
    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.unwrap();
    runtime
        .try_handle(request(
            "create-error-conversation",
            HostCommand::CreateConversation(create_conversation()),
        ))
        .await
        .unwrap();
    runtime
        .try_handle(request(
            "start-error-turn",
            HostCommand::StartConversationTurn(start_turn(1)),
        ))
        .await
        .unwrap();
    let claimed = runtime
        .try_handle(request(
            "claim-error-turn",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".into(),
                worker_id: "worker".into(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .unwrap();
    let HostResult::Claim { claim: Some(claim) } = claimed else {
        panic!("missing claim")
    };
    let error = "complete model request exceeds frozen context budget: estimated_input_tokens=621861, output_reserve_tokens=30000, context_window_tokens=200000";
    let committed = runtime
        .try_handle(request(
            "commit-error-turn",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".into(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::NeedsReview {
                    reason: "Local Agent profile step failed".into(),
                    detail: json!({"error":error}),
                },
            }),
        ))
        .await
        .unwrap();
    let HostResult::Run { run } = committed else {
        panic!("missing Run")
    };
    assert_eq!(run.terminal_outcome.as_ref().unwrap()["error"], error);
    let result = runtime
        .try_handle(request(
            "read-error-history",
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
            }),
        ))
        .await
        .unwrap();
    let HostResult::Conversation {
        conversation: detail,
    } = result
    else {
        panic!("missing conversation")
    };
    assert_eq!(detail.turns[0].status, LocalConversationTurnStatus::Failed);
    let last = detail.messages.last().unwrap();
    assert_eq!(last.role, LocalConversationMessageRole::Assistant);
    assert_eq!(last.content["error"], error);
    assert_eq!(last.content["status"], "needs_review");
}

#[tokio::test]
async fn host_routes_guidance_into_the_next_local_model_claim() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    storage
        .configure_memory_projection("user-1", "chatos", 9_000)
        .await
        .expect("configure Memory projection");
    let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(request(
            "create-conversation-guidance",
            HostCommand::CreateConversation(create_conversation()),
        ))
        .await
        .expect("create conversation");
    runtime
        .try_handle(request(
            "start-turn-guidance",
            HostCommand::StartConversationTurn(start_turn(1)),
        ))
        .await
        .expect("start Turn");
    let initial_projection = storage
        .claim_next_memory_record("user-1", "initial-memory-claim", 10_001, 20_000)
        .await
        .expect("claim initial projection")
        .expect("initial Memory record");
    assert_eq!(initial_projection.record_id, "message-1");
    storage
        .complete_memory_record(
            &initial_projection.source_id,
            &initial_projection.record_id,
            "initial-memory-claim",
            initial_projection.version,
            10_001,
        )
        .await
        .expect("complete initial projection");
    let guided = runtime
        .try_handle(request(
            "guide-turn",
            HostCommand::GuideConversationTurn(GuideConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                expected_conversation_version: 2,
                turn_id: "turn-1".to_string(),
                expected_run_version: Some(1),
                message_id: "message-guidance".to_string(),
                message: "include tests".to_string(),
                message_metadata: json!({}),
                attachments: vec![LocalConversationAttachmentSpec {
                    attachment_id: "attachment-guidance".to_string(),
                    display_name: "guidance.txt".to_string(),
                    media_type: "text/plain".to_string(),
                    byte_size: 14,
                    sha256: "b".repeat(64),
                    authorized_local_ref: "local-attachment:guidance.txt".to_string(),
                    metadata: json!({"purpose": "guidance"}),
                }],
            }),
        ))
        .await
        .expect("guide Turn");
    assert!(matches!(
        guided,
        HostResult::ConversationTurnUpdated { result }
            if result.conversation.version == 3 && result.message.is_some()
    ));
    let claim = runtime
        .try_handle(request(
            "claim-guided-turn",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim guided Run");
    assert!(matches!(
        claim,
        HostResult::Claim { claim: Some(claim) }
            if claim.run.continuation_input.as_ref()
                .and_then(|input| input.get("guidance"))
                .and_then(serde_json::Value::as_array)
                .is_some_and(|guidance| guidance.len() == 1)
    ));
    let projected = storage
        .claim_next_memory_record("user-1", "memory-claim", 10_001, 20_000)
        .await
        .expect("claim projected guidance")
        .expect("guidance Memory record");
    assert_eq!(projected.record_id, "message-guidance");
    assert_eq!(projected.thread_id, "client-agent:agent:jiguli");
    assert_eq!(projected.payload["content"], "include tests");
    assert_eq!(
        projected.payload["metadata"]["ui_attachments"][0]["attachment_id"],
        "attachment-guidance"
    );
}
