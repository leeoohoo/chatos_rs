use super::*;

#[test]
fn running_team_todo_cancel_is_journaled_before_atomic_host_reconciliation() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    let todo = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "停止中的任务",
            "必须先停止 Host Run",
            80,
            &[],
            None,
            100,
        )
        .unwrap();
    assert_eq!(
        store
            .enqueue_ready_project_team_todos("alice", &team.id, 101)
            .unwrap(),
        1
    );
    let pending = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &pending, "turn-cancel", "run-cancel", 102)
        .unwrap();
    let running = store
        .list_project_team_todos("alice", &team.id, true)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == todo.id)
        .unwrap();
    let intent = store
        .record_project_team_todo_cancel_intent(
            "alice",
            &team.id,
            &todo.id,
            running.updated_at_unix_ms,
            103,
        )
        .unwrap()
        .unwrap();
    assert_eq!(intent.action, "cancel");
    let replay = store
        .record_project_team_todo_cancel_intent(
            "alice",
            &team.id,
            &todo.id,
            running.updated_at_unix_ms,
            104,
        )
        .unwrap()
        .unwrap();
    assert_eq!(replay.id, intent.id);
    assert_eq!(
        store.pending_team_run_control_intents("alice").unwrap(),
        vec![intent.clone()]
    );
    assert_eq!(
        store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == todo.id)
            .unwrap()
            .status,
        "in_progress"
    );

    store
        .reconcile_project_team_runs(
            "alice",
            &team.id,
            &[team_todo::TeamRunReconciliation {
                delivery_id: intent.delivery_id.clone(),
                turn_id: "turn-cancel".into(),
                run_id: "run-cancel".into(),
                status: LocalConversationTurnStatus::Cancelled,
                created_at_unix_ms: 102,
                updated_at_unix_ms: 104,
                project_write_evidence_verified: false,
            }],
        )
        .unwrap();
    assert_eq!(
        store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == todo.id)
            .unwrap()
            .status,
        "blocked"
    );
    assert_eq!(
        store.pending_team_run_control_intents("alice").unwrap(),
        vec![intent.clone()]
    );

    let cancelled = store
        .complete_team_run_cancel_intent(
            &intent,
            &LocalConversationTurnRecord {
                turn_id: "turn-cancel".into(),
                conversation_id: intent.conversation_id.clone(),
                user_message_id: pending.message_id,
                run_id: "run-cancel".into(),
                status: LocalConversationTurnStatus::Cancelled,
                created_at_unix_ms: 102,
                updated_at_unix_ms: 105,
            },
        )
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(store
        .pending_team_run_control_intents("alice")
        .unwrap()
        .is_empty());
    let connection = Connection::open(store.database()).unwrap();
    let states: (String, String, String) = connection
        .query_row(
            "SELECT delivery.status, run.status, journal.status \
                 FROM project_agent_deliveries delivery \
                 JOIN windows_agent_delivery_runs run \
                   ON run.owner_user_id = delivery.owner_user_id \
                  AND run.delivery_id = delivery.id \
                 JOIN windows_agent_run_control_journal journal \
                   ON journal.owner_user_id = delivery.owner_user_id \
                  AND journal.delivery_id = delivery.id \
                 WHERE delivery.owner_user_id = 'alice' AND delivery.id = ?1",
            params![intent.delivery_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        states,
        ("cancelled".into(), "cancelled".into(), "completed".into())
    );
}

#[test]
fn interrupted_team_todo_retry_is_journaled_and_reopens_only_after_host_acceptance() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    let todo = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "中断任务",
            "等待 Human 核验后重试",
            80,
            &[],
            None,
            100,
        )
        .unwrap();
    store
        .enqueue_ready_project_team_todos("alice", &team.id, 101)
        .unwrap();
    let pending = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &pending, "turn-retry", "run-retry", 102)
        .unwrap();
    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "UPDATE local_agent_todos SET status = 'blocked', \
                 blocked_reason = '写入结果需要 Human 核验' \
                 WHERE owner_user_id = 'alice' AND id = ?1",
            params![todo.id],
        )
        .unwrap();
    drop(connection);

    let intent = store
        .record_project_team_run_retry_intent("alice", &team.id, "run-retry", 103)
        .unwrap();
    assert_eq!(intent.action, "retry");
    assert_eq!(
        store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == todo.id)
            .unwrap()
            .status,
        "blocked"
    );
    let resumed = store
        .complete_team_run_retry_intent(
            &intent,
            &LocalConversationTurnRecord {
                turn_id: "turn-retry".into(),
                conversation_id: intent.conversation_id.clone(),
                user_message_id: pending.message_id,
                run_id: "run-retry".into(),
                status: LocalConversationTurnStatus::Running,
                created_at_unix_ms: 102,
                updated_at_unix_ms: 104,
            },
            LocalAgentRunStatus::ContinuationReady,
            104,
        )
        .unwrap();
    assert_eq!(resumed.status, "in_progress");
    assert!(resumed.blocked_reason.is_empty());
    assert!(store
        .pending_team_run_control_intents("alice")
        .unwrap()
        .is_empty());
    let connection = Connection::open(store.database()).unwrap();
    let journal: (String, String) = connection
        .query_row(
            "SELECT status, host_status FROM windows_agent_run_control_journal \
                 WHERE owner_user_id = 'alice' AND id = ?1",
            params![intent.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(journal, ("completed".into(), "continuation_ready".into()));
}
