use super::*;

#[test]
fn agent_documents_are_run_scoped_idempotent_and_limited() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "sender", "active");
    let team = store
        .create_project_team("alice", "project-1", "文档团队", "验证文档草稿")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "sender")
        .unwrap();
    store
        .queue_project_team_message("alice", &team.id, "创建文档", &["sender".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &delivery, "turn-doc", "run-doc", 10)
        .unwrap();

    let (_, first) = store
        .create_agent_chat_document(
            "alice",
            "run-doc",
            "create-doc-1",
            "../../交付:说明",
            "交付说明",
            "# 交付\n正文",
            11,
        )
        .unwrap();
    assert_eq!(first["name"], "交付-说明.md");
    assert_eq!(first["mime_type"], "text/markdown");
    let (_, replayed) = store
        .create_agent_chat_document(
            "alice",
            "run-doc",
            "create-doc-1",
            "../../交付:说明",
            "交付说明",
            "# 交付\n正文",
            12,
        )
        .unwrap();
    assert_eq!(replayed, first);
    assert!(store
        .create_agent_chat_document(
            "alice",
            "run-doc",
            "create-doc-1",
            "不同名称",
            "交付说明",
            "# 交付\n正文",
            12,
        )
        .is_err());

    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "DELETE FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-doc'",
            [],
        )
        .unwrap();
    drop(connection);
    let maximum_document = "x".repeat(2 * 1_024 * 1_024);
    for index in 1..=4 {
        store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                &format!("large-doc-{index}"),
                &format!("大文档-{index}"),
                &format!("大文档 {index}"),
                &maximum_document,
                40 + index,
            )
            .unwrap();
    }
    assert!(store
        .create_agent_chat_document(
            "alice",
            "run-doc",
            "run-bytes-overflow",
            "总量超限",
            "总量超限",
            "x",
            45,
        )
        .is_err());
    assert!(store
        .create_agent_chat_document(
            "alice",
            "run-doc",
            "too-large",
            "超限",
            "超限",
            &"x".repeat(2 * 1_024 * 1_024 + 1),
            12,
        )
        .is_err());

    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "DELETE FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-doc'",
            [],
        )
        .unwrap();
    drop(connection);
    for index in 1..=20 {
        store
            .create_agent_chat_document(
                "alice",
                "run-doc",
                &format!("create-doc-{index}"),
                &format!("文档-{index}"),
                &format!("文档 {index}"),
                "正文",
                10 + index,
            )
            .unwrap();
    }
    assert!(store
        .create_agent_chat_document(
            "alice",
            "run-doc",
            "create-doc-21",
            "文档-21",
            "文档 21",
            "正文",
            31,
        )
        .is_err());
}

#[test]
fn agent_documents_attach_to_team_messages_and_consume_once() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "sender", "active");
    let team = store
        .create_project_team("alice", "project-1", "文档团队", "接收文档")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "sender")
        .unwrap();
    store
        .queue_project_team_message("alice", &team.id, "请提交说明", &["sender".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &delivery, "turn-team-doc", "run-team-doc", 10)
        .unwrap();
    let (_, document) = store
        .create_agent_chat_document(
            "alice",
            "run-team-doc",
            "create-team-doc",
            "team-notes",
            "团队说明",
            "# 团队说明\n真实正文",
            11,
        )
        .unwrap();
    let document_reference = document["document_ref"].as_str().unwrap().to_owned();
    let connection = Connection::open(store.database()).unwrap();
    let draft_path: String = connection
        .query_row(
            "SELECT relative_path FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-team-doc'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    let absolute_draft_path = store
        .database()
        .parent()
        .unwrap()
        .join("AgentMessageAttachments")
        .join(draft_path);
    assert!(absolute_draft_path.is_file());
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-team-doc",
            "duplicate-team-doc",
            "不得重复引用。",
            None,
            None,
            None,
            &[document_reference.clone(), document_reference.clone()],
            &[],
            12,
        )
        .is_err());

    let (_, sent) = store
        .send_team_chat_message(
            "alice",
            "run-team-doc",
            "send-team-doc",
            "说明已完成。",
            None,
            None,
            None,
            std::slice::from_ref(&document_reference),
            &[],
            12,
        )
        .unwrap();
    assert!(!absolute_draft_path.exists());
    let (_, replayed) = store
        .send_team_chat_message(
            "alice",
            "run-team-doc",
            "send-team-doc",
            "说明已完成。",
            None,
            None,
            None,
            std::slice::from_ref(&document_reference),
            &[],
            13,
        )
        .unwrap();
    assert_eq!(replayed, sent);
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-team-doc",
            "reuse-consumed-doc",
            "不得再次消费。",
            None,
            None,
            None,
            std::slice::from_ref(&document_reference),
            &[],
            14,
        )
        .is_err());

    let connection = Connection::open(store.database()).unwrap();
    let attachment: (i64, String, String) = connection
        .query_row(
            "SELECT count(*), min(attachment.mime_type), min(room.conversation_kind) \
                 FROM project_agent_message_attachments attachment \
                 JOIN project_agent_messages message \
                   ON message.owner_user_id = attachment.owner_user_id \
                  AND message.id = attachment.message_id \
                 JOIN project_agent_rooms room ON room.owner_user_id = message.owner_user_id \
                  AND room.id = message.room_id \
                 WHERE message.causation_id = 'send-team-doc'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        attachment,
        (
            1,
            "text/markdown; charset=utf-8".into(),
            "project_team".into()
        )
    );
}

#[test]
fn direct_message_documents_issue_refs_page_text_and_reject_mismatches() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "sender", "active");
    insert_agent(&store, "alice", "target", "active");
    let team = store
        .create_project_team("alice", "project-1", "私聊文档团队", "发送私聊文档")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "sender")
        .unwrap();
    store
        .queue_project_team_message("alice", &team.id, "发送两份文档", &["sender".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &delivery, "turn-sender-doc", "run-sender-doc", 10)
        .unwrap();
    let (_, workspace) = store
        .agent_workspace_snapshot("alice", "run-sender-doc")
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
    for (index, markdown) in [(1, "abcdef甲乙"), (2, "第二份正文")] {
        let (_, document) = store
            .create_agent_chat_document(
                "alice",
                "run-sender-doc",
                &format!("create-direct-doc-{index}"),
                &format!("direct-{index}"),
                &format!("私聊文档 {index}"),
                markdown,
                10 + index,
            )
            .unwrap();
        let reference = document["document_ref"].as_str().unwrap().to_owned();
        store
            .send_team_chat_message(
                "alice",
                "run-sender-doc",
                &format!("send-direct-doc-{index}"),
                &format!("发送文档 {index}"),
                None,
                Some(&target_reference),
                None,
                std::slice::from_ref(&reference),
                &[],
                12 + index,
            )
            .unwrap();
    }
    let (_, unsent_document) = store
        .create_agent_chat_document(
            "alice",
            "run-sender-doc",
            "create-unsent-doc",
            "unsent",
            "未发送文档",
            "仅属于发送方 Run",
            15,
        )
        .unwrap();
    let unsent_document_reference = unsent_document["document_ref"].as_str().unwrap().to_owned();
    let connection = Connection::open(store.database()).unwrap();
    let direct_room_id: String = connection
        .query_row(
            "SELECT id FROM project_agent_rooms WHERE owner_user_id = 'alice' \
                 AND conversation_kind = 'agent_agent_direct'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    let incoming = store
        .next_agent_communication_delivery("alice", &direct_room_id)
        .unwrap()
        .unwrap();
    store
        .claim_agent_communication_delivery(
            "alice",
            &incoming,
            "turn-target-doc",
            "run-target-doc",
            20,
        )
        .unwrap();
    let (_, unread) = store
        .read_all_unread_team_chat("alice", "run-target-doc", "read-direct-docs", 20, 21)
        .unwrap();
    assert_eq!(unread["message_count"], 2);
    let messages = unread["conversations"][0]["messages"].as_array().unwrap();
    let first_message = messages
        .iter()
        .find(|message| message["content"] == "发送文档 1")
        .unwrap();
    let second_message = messages
        .iter()
        .find(|message| message["content"] == "发送文档 2")
        .unwrap();
    let first_message_reference = first_message["message_ref"].as_str().unwrap();
    let first_attachment_reference = first_message["attachments"][0]["attachment_ref"]
        .as_str()
        .unwrap();
    let second_attachment_reference = second_message["attachments"][0]["attachment_ref"]
        .as_str()
        .unwrap();
    let (_, first_page) = store
        .read_agent_chat_attachment(
            "alice",
            "run-target-doc",
            first_message_reference,
            first_attachment_reference,
            0,
            4,
        )
        .unwrap();
    assert_eq!(first_page["content"], "abcd");
    assert_eq!(first_page["next_offset"], 4);
    assert_eq!(first_page["has_more"], true);
    assert!(store
        .read_agent_chat_attachment(
            "alice",
            "run-target-doc",
            first_message_reference,
            second_attachment_reference,
            0,
            12_000,
        )
        .is_err());
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-target-doc",
            "cross-run-document",
            "不得消费其他 Run 的草稿。",
            None,
            None,
            None,
            std::slice::from_ref(&unsent_document_reference),
            &[],
            22,
        )
        .is_err());
    let unsent_status: String = Connection::open(store.database())
        .unwrap()
        .query_row(
            "SELECT status FROM windows_agent_document_drafts \
                 WHERE owner_user_id = 'alice' AND run_id = 'run-sender-doc' \
                   AND document_ref = ?1",
            params![unsent_document_reference],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unsent_status, "available");

    let connection = Connection::open(store.database()).unwrap();
    let relative_path: String = connection
        .query_row(
            "SELECT attachment.relative_path \
                 FROM project_agent_message_attachments attachment \
                 JOIN windows_agent_chat_attachment_references reference \
                   ON reference.owner_user_id = attachment.owner_user_id \
                  AND reference.attachment_id = attachment.id \
                 WHERE reference.owner_user_id = 'alice' \
                   AND reference.run_id = 'run-target-doc' \
                   AND reference.attachment_ref = ?1",
            params![first_attachment_reference],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    let attachment_path = store
        .database()
        .parent()
        .unwrap()
        .join("AgentMessageAttachments")
        .join(relative_path);
    let size = fs::metadata(&attachment_path).unwrap().len() as usize;
    fs::write(&attachment_path, vec![b'x'; size]).unwrap();
    assert!(store
        .read_agent_chat_attachment(
            "alice",
            "run-target-doc",
            first_message_reference,
            first_attachment_reference,
            0,
            12_000,
        )
        .is_err());
}
