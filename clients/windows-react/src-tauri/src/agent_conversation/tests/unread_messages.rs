use super::*;

#[test]
fn team_unread_messages_advance_cursors_and_route_replies_by_run_scoped_reference() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "sender", "active");
    insert_agent(&store, "alice", "target", "active");
    let first_team = store
        .create_project_team("alice", "project-1", "第一团队", "接收唤醒")
        .unwrap();
    let second_team = store
        .create_project_team("alice", "project-2", "第二团队", "接收跨团队回复")
        .unwrap();
    for team_id in [&first_team.id, &second_team.id] {
        store
            .invite_agent_to_project_team("alice", team_id, "sender")
            .unwrap();
        store
            .invite_agent_to_project_team("alice", team_id, "target")
            .unwrap();
    }
    let trigger = store
        .queue_project_team_message("alice", &first_team.id, "处理全部未读", &["sender".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &first_team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &delivery, "turn-unread", "run-unread", 10)
        .unwrap();
    let other = store
        .queue_project_team_message("alice", &second_team.id, "回复第二团队", &["sender".into()])
        .unwrap();
    let (_, workspace) = store
        .agent_workspace_snapshot("alice", "run-unread")
        .unwrap();
    assert!(!workspace.to_string().contains(&first_team.id));
    assert!(!workspace.to_string().contains(&second_team.id));
    assert!(!workspace.to_string().contains("\"sender\""));
    assert!(!workspace.to_string().contains("\"target\""));
    let target_reference = workspace["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["is_current_agent"] == false)
        .unwrap()["agent_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let second_team_reference = workspace["teams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|team| team["name"] == "第二团队")
        .unwrap()["team_ref"]
        .as_str()
        .unwrap()
        .to_owned();

    let (_, unread) = store
        .read_all_unread_team_chat("alice", "run-unread", "read-unread-1", 200, 11)
        .unwrap();
    assert_eq!(unread["message_count"], 2);
    assert_eq!(unread["marked_read"], true);
    assert!(!unread.to_string().contains(&trigger.message_id));
    assert!(!unread.to_string().contains(&other.message_id));
    assert!(!unread.to_string().contains(&first_team.id));
    assert!(!unread.to_string().contains(&second_team.id));
    let other_reference = unread["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|conversation| conversation["messages"].as_array().unwrap())
        .find(|message| message["content"] == "回复第二团队")
        .unwrap()["message_ref"]
        .as_str()
        .unwrap()
        .to_owned();

    let (_, sent) = store
        .send_team_chat_message(
            "alice",
            "run-unread",
            "invocation-reply-other-team",
            "第二团队已收到。",
            Some(&other_reference),
            None,
            None,
            &[],
            std::slice::from_ref(&target_reference),
            12,
        )
        .unwrap();
    assert_eq!(sent["spawned_delivery_count"], 1);
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-unread",
            "invocation-forged-reply",
            "不得写入。",
            Some("message_ref_forged"),
            None,
            None,
            &[],
            &[],
            13,
        )
        .is_err());
    let (_, proactive) = store
        .send_team_chat_message(
            "alice",
            "run-unread",
            "invocation-proactive-team",
            "主动同步第二团队。",
            None,
            None,
            Some(&second_team_reference),
            &[],
            std::slice::from_ref(&target_reference),
            13,
        )
        .unwrap();
    assert_eq!(proactive["spawned_delivery_count"], 1);
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-unread",
            "invocation-forged-team",
            "不得写入。",
            None,
            None,
            Some("team_ref_forged"),
            &[],
            &[],
            13,
        )
        .is_err());
    let connection = Connection::open(store.database()).unwrap();
    let routed: (String, String, String) = connection
        .query_row(
            "SELECT room_id, reply_to_message_id, root_message_id \
                 FROM project_agent_messages WHERE owner_user_id = 'alice' \
                   AND causation_id = 'invocation-reply-other-team'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        routed,
        (
            second_team.id.clone(),
            other.message_id.clone(),
            other.message_id.clone(),
        )
    );
    let proactive_routing: (String, Option<String>, String, String) = connection
        .query_row(
            "SELECT room_id, reply_to_message_id, root_message_id, id \
                 FROM project_agent_messages WHERE owner_user_id = 'alice' \
                   AND causation_id = 'invocation-proactive-team'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(proactive_routing.0, second_team.id);
    assert_eq!(proactive_routing.1, None);
    assert_eq!(proactive_routing.2, proactive_routing.3);
    let cursor_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_read_cursors \
                 WHERE owner_user_id = 'alice' AND agent_id = 'sender'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cursor_count, 2);
    drop(connection);

    let (_, replayed) = store
        .read_all_unread_team_chat("alice", "run-unread", "read-unread-1", 200, 14)
        .unwrap();
    assert_eq!(replayed, unread);
    assert!(store
        .read_all_unread_team_chat("alice", "run-unread", "read-unread-1", 1, 14)
        .is_err());
    let (_, after) = store
        .read_all_unread_team_chat("alice", "run-unread", "read-unread-2", 200, 14)
        .unwrap();
    assert_eq!(after["message_count"], 0);
    assert_eq!(after["conversations"], json!([]));
}
