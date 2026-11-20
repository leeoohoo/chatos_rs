#[tokio::test]
async fn committed_replies_are_visible_before_sync_and_remote_takes_over_without_duplicates() {
    use chatos_ai_runtime::SaveRecordInput;

    for sync_during_read in [false, true] {
        let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
        storage
            .configure_memory_projection("user-1", "chatos", 9_000)
            .await
            .unwrap();
        let user = SaveRecordInput::user_message("client-agent:agent:jiguli", "list files")
            .with_message_id("message-2")
            .with_conversation_turn_id("turn-2")
            .with_metadata(json!({"conversation_id":"conversation-1"}))
            .with_created_at("2026-11-10T10:00:00Z");
        let reply = SaveRecordInput::assistant_message("client-agent:agent:jiguli", "task created")
            .with_message_id("run-2:assistant:1")
            .with_conversation_turn_id("turn-2")
            .with_metadata(json!({"conversation_id":"conversation-1"}))
            .with_created_at("2026-11-10T10:00:01Z");
        let callback = |id: &str, event: &str, at: i64, content: &str| {
            SaveRecordInput::assistant_message("client-agent:agent:jiguli", content)
                .with_message_id(id)
                .with_conversation_turn_id("turn-2")
                .with_metadata(json!({
                    "conversation_id":"conversation-1",
                    "ui_message_id":"task-callback-2",
                    "task_runner_async":{"event":event,"callback_at_unix_ms":at}
                }))
                .with_created_at("2026-11-10T10:00:02Z")
        };
        let running = callback("event-running", "task.run.started", 1_000, "running");
        let completed = callback("event-completed", "task.completed", 2_000, "done");
        let record = |input: &SaveRecordInput| AuthoritativeConversationRecord {
            record_id: input.message_id.clone().unwrap(),
            role: input.role.clone(),
            content: input.content.clone(),
            structured_payload: input.structured_payload.clone(),
            metadata: input.packed_metadata(),
            created_at: input.created_at.clone().unwrap(),
        };
        for (index, input) in [&user, &reply, &completed].into_iter().enumerate() {
            let mut payload = serde_json::to_value(input).unwrap();
            if index == 0 {
                payload.as_object_mut().unwrap().remove("created_at");
            }
            storage
                .enqueue_memory_record(
                    input.message_id.as_deref().unwrap(),
                    "user-1",
                    "chatos",
                    "client-agent:agent:jiguli",
                    &payload,
                    1_794_304_800_000 + index as i64 * 1_000,
                )
                .await
                .unwrap();
        }
        let source = Arc::new(LiveTranscriptFixture {
            storage: storage.clone(),
            records: std::sync::Mutex::new(vec![record(&running)]),
            sync_during_read: std::sync::atomic::AtomicBool::new(sync_during_read),
        });
        let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)))
            .with_conversation_transcript_source(source.clone());
        runtime.initialize("user-1").await.unwrap();
        runtime
            .try_handle(request(
                "create-live",
                HostCommand::CreateConversation(create_conversation()),
            ))
            .await
            .unwrap();
        let command = GetConversationHistoryCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            before_cursor: None,
            limit: 10,
        };
        let live = runtime
            .authoritative_conversation_history(command.clone())
            .await
            .unwrap();
        assert_eq!(live.turns.len(), 1);
        assert_eq!(live.turns[0].user_message_id, "message-2");
        assert_eq!(live.messages.len(), 3);
        assert_eq!(live.messages[1].message_id, "run-2:assistant:1");
        assert_eq!(live.messages[1].content, json!("task created"));
        assert_eq!(live.messages[2].message_id, "task-callback-2");
        assert_eq!(live.messages[2].content, json!("done"));
        assert_eq!(live.next_before_cursor.as_deref(), Some("older"));

        let older = runtime
            .authoritative_conversation_history(GetConversationHistoryCommand {
                before_cursor: Some("older".to_string()),
                ..command.clone()
            })
            .await
            .unwrap();
        assert_eq!(
            older.messages.len(),
            1,
            "older pages must not repeat live updates"
        );
        assert_eq!(older.messages[0].content, json!("running"));

        *source.records.lock().unwrap() = vec![
            record(&user),
            record(&reply),
            record(&running),
            record(&completed),
        ];
        source
            .sync_during_read
            .store(true, std::sync::atomic::Ordering::SeqCst);
        runtime
            .authoritative_conversation_history(command.clone())
            .await
            .unwrap();
        assert!(storage
            .list_pending_conversation_memory_records(
                "user-1",
                "client-agent:agent:jiguli",
                "conversation-1"
            )
            .await
            .unwrap()
            .is_empty());
        let synced = runtime
            .authoritative_conversation_history(command)
            .await
            .unwrap();
        assert_eq!(synced.messages, live.messages);
    }
}

#[test]
fn projection_history_keeps_latest_snapshot_regardless_of_record_order() {
    let snapshot =
        |id: &str, event: &str, at: i64, content: &str| AuthoritativeConversationRecord {
            record_id: id.to_string(),
            role: "assistant".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_turn_id":"turn-1",
                "ui_message_id":"callback-1",
                "memory_projection_event":true,
                "task_runner_async":{
                    "event":event,
                    "callback_at_unix_ms":at
                }
            })),
            created_at: "2026-11-10T10:00:00Z".to_string(),
        };
    let running = snapshot("projection-event:z", "task.run.started", 1_000, "running");
    for completed_at in [1_000, 2_000] {
        let completed = snapshot("projection-event:a", "task.completed", completed_at, "done");
        for records in [
            vec![running.clone(), completed.clone()],
            vec![completed.clone(), running.clone()],
        ] {
            let messages = super::authoritative_messages("conversation-1", &records).unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].message_id, "callback-1");
            assert_eq!(messages[0].content, json!("done"));
            assert_eq!(
                messages[0].metadata["task_runner_async"]["event"],
                "task.completed"
            );
        }
    }

    let mut open = running;
    open.metadata = Some(json!({
        "conversation_turn_id":"turn-1", "ui_message_id":"survey-1",
        "ui_structured_content":true, "memory_projection_event":true
    }));
    open.structured_payload = Some(json!({"type":"requirement_survey_status","status":"open"}));
    let mut resolved = open.clone();
    resolved.record_id = "projection-event:b".to_string();
    resolved.created_at = "2026-11-10T10:00:01Z".to_string();
    resolved.structured_payload =
        Some(json!({"type":"requirement_survey_status","status":"resolved"}));
    for records in [vec![open.clone(), resolved.clone()], vec![resolved, open]] {
        let messages = super::authoritative_messages("conversation-1", &records).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content["status"], "resolved");
        assert_eq!(messages[0].created_at_unix_ms, 1_794_304_800_000);
    }
}

#[tokio::test]
async fn recent_attachment_catalog_uses_local_index_without_memory_transcript() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(request(
            "create-local-attachment-conversation",
            HostCommand::CreateConversation(create_conversation()),
        ))
        .await
        .expect("create conversation");
    runtime
        .try_handle(request(
            "start-local-attachment-turn",
            HostCommand::StartConversationTurn(start_turn(1)),
        ))
        .await
        .expect("start turn");

    let attachments = runtime
        .recent_conversation_attachments("user-1", "conversation-1")
        .await
        .expect("read local attachment catalog");

    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].attachment_id, "attachment-1");
    assert_eq!(
        attachments[0].authorized_local_ref,
        "local-attachment:authority-1"
    );
}

#[tokio::test]
async fn configured_history_uses_memory_transcript_instead_of_local_messages() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)))
        .with_conversation_transcript_source(Arc::new(TranscriptFixture));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(request(
            "create-memory-authoritative-conversation",
            HostCommand::CreateConversation(create_conversation()),
        ))
        .await
        .expect("create conversation");
    let history = runtime
        .try_handle(request(
            "read-memory-authoritative-history",
            HostCommand::GetConversationHistory(GetConversationHistoryCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                before_cursor: None,
                limit: 10,
            }),
        ))
        .await
        .expect("read Memory transcript");
    let HostResult::ConversationHistory { page } = history else {
        panic!("unexpected result")
    };
    assert_eq!(page.messages.len(), 2);
    assert_eq!(page.messages[0].content, json!({"text": "remote hello"}));
    assert_eq!(page.messages[0].metadata["composer"], "main_chat");
    assert_eq!(page.messages[0].metadata["client_marker"], "authoritative");
    assert_eq!(page.messages[1].content, json!("remote answer"));
    assert_eq!(page.turns.len(), 1);
    assert_eq!(page.turns[0].turn_id, "turn-1");
    assert_eq!(page.turns[0].user_message_id, "message-1");
    assert_eq!(page.next_before_cursor.as_deref(), Some("cursor-2"));
    assert_eq!(page.attachments.len(), 1);
    assert_eq!(page.attachments[0].metadata, json!({"origin": "memory"}));
}
