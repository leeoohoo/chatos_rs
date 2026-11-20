use super::*;

#[test]
fn team_creation_requires_an_active_account_scoped_project() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("Projects.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(crate::workspace_index::PROJECT_REGISTRY_SCHEMA)
        .unwrap();
    connection
        .execute(
            "INSERT INTO local_project_records VALUES \
                 ('alice', 'project-1', 'Alpha', '', 'workspace-1', '', \
                  'software_development', 1, 'active', 1, 1)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO local_project_records VALUES \
                 ('alice', 'removed', 'Removed', '', 'workspace-1', '', \
                  'software_development', 1, 'removed', 1, 1)",
            [],
        )
        .unwrap();
    drop(connection);

    assert!(require_active_project(&database, "alice", "project-1").is_ok());
    assert!(require_active_project(&database, "bob", "project-1").is_err());
    assert!(require_active_project(&database, "alice", "removed").is_err());
}

#[test]
fn direct_delivery_persists_and_reconciles_from_the_bound_host_turn() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let room = store.open_human_agent_direct("alice", "agent-1").unwrap();
    let binding = store.load_execution_binding("alice", &room.id).unwrap();
    store
        .record_direct_delivery(
            "alice",
            &binding,
            "delivery-1",
            "message-1",
            "turn-1",
            "run-1",
            "请完成任务",
            100,
        )
        .unwrap();
    let tool_binding = load_agent_communication_tool_binding(
        &Connection::open(store.database()).unwrap(),
        "alice",
        "run-1",
    )
    .unwrap();
    assert_eq!(tool_binding.conversation_kind, "human_agent_direct");
    assert!(store
        .communication_rooms_requiring_scheduler("alice")
        .unwrap()
        .is_empty());
    assert!(store
        .next_agent_communication_delivery("alice", &room.id)
        .unwrap()
        .is_none());

    let pending = store.list_direct_activity("alice", &binding).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].delivery_status, "pending");
    assert_eq!(pending[0].run_status, "ready");

    let detail = LocalConversationDetail {
        conversation: chatos_local_agent_protocol::LocalConversationRecord {
            conversation_id: agent_direct_conversation_id(&room.id),
            owner_user_id: "alice".into(),
            title: "测试 Agent".into(),
            resource: Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id: room.agent_id.clone(),
            }),
            version: 2,
            created_at_unix_ms: 100,
            updated_at_unix_ms: 220,
        },
        turns: vec![LocalConversationTurnRecord {
            turn_id: "turn-1".into(),
            conversation_id: agent_direct_conversation_id(&room.id),
            user_message_id: "message-1".into(),
            run_id: "run-1".into(),
            status: LocalConversationTurnStatus::Succeeded,
            created_at_unix_ms: 110,
            updated_at_unix_ms: 220,
        }],
        messages: vec![
            chatos_local_agent_protocol::LocalConversationMessageRecord {
                message_id: "answer-1".into(),
                conversation_id: agent_direct_conversation_id(&room.id),
                turn_id: "turn-1".into(),
                ordinal: 2,
                role: LocalConversationMessageRole::Assistant,
                content: json!({"text": "已完成"}),
                metadata: json!({}),
                created_at_unix_ms: 210,
            },
        ],
        attachments: vec![],
    };
    let completed = store
        .reconcile_direct_activity("alice", &binding, &detail)
        .unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].delivery_status, "completed");
    assert_eq!(completed[0].run_status, "succeeded");
    assert_eq!(
        completed[0].response_message_id.as_deref(),
        Some("answer-1")
    );
    assert_eq!(completed[0].updated_at_unix_ms, 220);

    let connection = Connection::open(store.database()).unwrap();
    let response: (String, String, String) = connection
        .query_row(
            "SELECT sender_kind, sender_id, content FROM project_agent_messages \
                 WHERE owner_user_id = 'alice' AND id = 'answer-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        response,
        ("agent".into(), "agent-1".into(), "已完成".into())
    );
    assert_eq!(
        store.activity_room_ids("alice", "agent-1").unwrap(),
        vec![room.id]
    );
    insert_agent(&store, "alice", "agent-2", "active");
    assert!(store
        .activity_room_ids("alice", "agent-2")
        .unwrap()
        .is_empty());
    assert!(store.activity_room_ids("bob", "agent-1").is_err());
}
