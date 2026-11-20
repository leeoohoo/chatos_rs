use super::*;

#[test]
fn due_heartbeats_create_one_durable_cycle_and_advance_the_schedule() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "heartbeat-agent", "active");
    Connection::open(store.database())
        .unwrap()
        .execute(
            "UPDATE local_agent_profiles SET heartbeat_enabled = 1, \
                 heartbeat_interval_seconds = 60, heartbeat_prompt = '检查全局未读', \
                 next_heartbeat_at_unix_ms = 10 \
                 WHERE owner_user_id = 'alice' AND id = 'heartbeat-agent'",
            [],
        )
        .unwrap();

    assert!(store
        .enqueue_due_agent_heartbeats("alice", 9, 16)
        .unwrap()
        .is_empty());
    let rooms = store.enqueue_due_agent_heartbeats("alice", 10, 16).unwrap();
    assert_eq!(rooms.len(), 1);
    assert_eq!(
        store
            .communication_rooms_requiring_scheduler("alice")
            .unwrap(),
        rooms
    );
    let schedule: (Option<i64>, Option<i64>) = Connection::open(store.database())
        .unwrap()
        .query_row(
            "SELECT last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms \
                 FROM local_agent_profiles WHERE owner_user_id = 'alice' \
                   AND id = 'heartbeat-agent'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(schedule, (Some(10), Some(60_010)));

    assert!(store
        .enqueue_due_agent_heartbeats("alice", 60_010, 16)
        .unwrap()
        .is_empty());
    let connection = Connection::open(store.database()).unwrap();
    let facts: (i64, i64, Option<i64>) = connection
        .query_row(
            "SELECT count(*), \
                        sum(CASE WHEN trigger_kind = 'heartbeat' THEN 1 ELSE 0 END), \
                        (SELECT next_heartbeat_at_unix_ms FROM local_agent_profiles \
                         WHERE owner_user_id = 'alice' AND id = 'heartbeat-agent') \
                 FROM project_agent_deliveries WHERE owner_user_id = 'alice'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(facts, (1, 1, Some(120_010)));
    drop(connection);

    let pending = store
        .next_agent_communication_delivery("alice", &rooms[0])
        .unwrap()
        .unwrap();
    assert_eq!(pending.trigger_kind, "heartbeat");
    assert_eq!(pending.conversation_kind, "human_agent_direct");
    assert_eq!(pending.content, "检查全局未读");
    store
        .claim_agent_communication_delivery(
            "alice",
            &pending,
            "turn-heartbeat",
            "run-heartbeat",
            60_011,
        )
        .unwrap();
    let (binding, outcome) = store
        .validate_agent_communication_cycle_completion("alice", "run-heartbeat", true)
        .unwrap();
    assert_eq!(binding.conversation_kind, "human_agent_direct");
    assert_eq!(outcome["status"], "completed");
    store
        .reconcile_agent_communication_runs(
            "alice",
            &rooms[0],
            &[AgentCommunicationRunReconciliation {
                delivery_id: pending.delivery_id,
                turn_id: "turn-heartbeat".into(),
                run_id: "run-heartbeat".into(),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms: 60_011,
                updated_at_unix_ms: 60_012,
            }],
        )
        .unwrap();
    assert!(!store
        .communication_scheduler_has_work("alice", &rooms[0])
        .unwrap());
}

#[test]
fn agent_direct_messages_create_two_member_rooms_and_routable_deliveries() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "sender", "active");
    insert_agent(&store, "alice", "target", "active");
    let team = store
        .create_project_team("alice", "project-1", "通讯团队", "启动私聊")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "sender")
        .unwrap();
    let trigger = store
        .queue_project_team_message("alice", &team.id, "请联系目标 Agent", &["sender".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    assert_eq!(delivery.message_id, trigger.message_id);
    store
        .claim_project_team_delivery("alice", &delivery, "turn-sender", "run-sender", 10)
        .unwrap();
    let (_, workspace) = store
        .agent_workspace_snapshot("alice", "run-sender")
        .unwrap();
    let target_reference = workspace["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["is_current_agent"] == false)
        .unwrap()["agent_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let sender_reference = workspace["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["is_current_agent"] == true)
        .unwrap()["agent_ref"]
        .as_str()
        .unwrap()
        .to_owned();

    let (_, sent) = store
        .send_team_chat_message(
            "alice",
            "run-sender",
            "invocation-direct-1",
            "这是一条真实私聊。",
            None,
            Some(&target_reference),
            None,
            &[],
            &[],
            11,
        )
        .unwrap();
    assert_eq!(sent["spawned_delivery_count"], 1);
    let (_, replayed) = store
        .send_team_chat_message(
            "alice",
            "run-sender",
            "invocation-direct-1",
            "这是一条真实私聊。",
            None,
            Some(&target_reference),
            None,
            &[],
            &[],
            12,
        )
        .unwrap();
    assert_eq!(replayed, sent);
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-sender",
            "invocation-direct-forged",
            "不得发送。",
            None,
            Some("agent_ref_forged"),
            None,
            &[],
            &[],
            12,
        )
        .is_err());
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-sender",
            "invocation-direct-self",
            "不得发送给自己。",
            None,
            Some(&sender_reference),
            None,
            &[],
            &[],
            12,
        )
        .is_err());
    let connection = Connection::open(store.database()).unwrap();
    let (direct_room_id, conversation_kind, direct_key): (String, String, String) = connection
        .query_row(
            "SELECT id, conversation_kind, direct_key FROM project_agent_rooms \
                 WHERE owner_user_id = 'alice' AND conversation_kind = 'agent_agent_direct'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(conversation_kind, "agent_agent_direct");
    assert_eq!(direct_key, "agent:sender|agent:target");
    let member_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = 'alice' AND room_id = ?1 AND status = 'active'",
            params![direct_room_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(member_count, 2);
    drop(connection);
    assert_eq!(
        store
            .communication_rooms_requiring_scheduler("alice")
            .unwrap(),
        vec![direct_room_id.clone()]
    );

    let pending = store
        .next_agent_communication_delivery("alice", &direct_room_id)
        .unwrap()
        .unwrap();
    assert_eq!(pending.binding.agent_id, "target");
    assert_eq!(pending.trigger_kind, "default_agent");
    store
        .claim_agent_communication_delivery("alice", &pending, "turn-target", "run-target", 12)
        .unwrap();
    assert!(store
        .validate_agent_communication_cycle_completion("alice", "run-target", true)
        .is_err());
    assert_eq!(
        store
            .validate_agent_communication_cycle_completion("alice", "run-target", false)
            .unwrap()
            .1["status"],
        "completed"
    );
    let (binding, unread) = store
        .read_all_unread_team_chat("alice", "run-target", "read-direct", 20, 13)
        .unwrap();
    assert_eq!(binding.conversation_kind, "agent_agent_direct");
    assert_eq!(unread["message_count"], 1);
    assert_eq!(unread["conversations"][0]["kind"], "agent_agent_direct");
    let (_, direct_members) = store
        .team_chat_member_snapshot("alice", "run-target")
        .unwrap();
    assert_eq!(direct_members.as_array().unwrap().len(), 2);
    assert_eq!(
        store
            .active_agent_communication_run_bindings("alice", &direct_room_id)
            .unwrap()[0]
            .conversation_id,
        agent_communication_conversation_id(&direct_room_id, "target")
    );
    let (_, reply) = store
        .send_team_chat_message(
            "alice",
            "run-target",
            "invocation-direct-reply",
            "已收到并回复。",
            None,
            None,
            None,
            &[],
            &[],
            14,
        )
        .unwrap();
    assert_eq!(reply["spawned_delivery_count"], 1);
    store
        .reconcile_agent_communication_runs(
            "alice",
            &direct_room_id,
            &[AgentCommunicationRunReconciliation {
                delivery_id: pending.delivery_id,
                turn_id: "turn-target".into(),
                run_id: "run-target".into(),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms: 12,
                updated_at_unix_ms: 15,
            }],
        )
        .unwrap();
    store
        .reconcile_project_team_runs(
            "alice",
            &team.id,
            &[team_todo::TeamRunReconciliation {
                delivery_id: delivery.delivery_id,
                turn_id: "turn-sender".into(),
                run_id: "run-sender".into(),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms: 10,
                updated_at_unix_ms: 15,
                project_write_evidence_verified: false,
            }],
        )
        .unwrap();
    let reply_pending = store
        .next_agent_communication_delivery("alice", &direct_room_id)
        .unwrap()
        .unwrap();
    assert_eq!(reply_pending.binding.agent_id, "sender");
    assert!(store
        .communication_scheduler_has_work("alice", &direct_room_id)
        .unwrap());
}
