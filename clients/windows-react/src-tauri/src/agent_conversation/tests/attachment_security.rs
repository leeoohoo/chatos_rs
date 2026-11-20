use super::*;

#[test]
fn human_team_binary_attachment_is_persisted_and_marked_multimodal_for_trigger_run() {
    use sha2::Digest as _;

    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "多模态团队", "检查图片")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    let data = vec![0x89, b'P', b'N', b'G', 0, 1, 2, 3];
    let sha256 = format!("{:x}", sha2::Sha256::digest(&data));
    let historical = store
        .queue_project_team_message_with_attachments(
            "alice",
            &team.id,
            "",
            &["agent-1".into()],
            &[crate::conversation_attachments::VerifiedAttachmentPayload {
                display_name: "设计图.png".into(),
                media_type: "image/png".into(),
                kind: "image".into(),
                origin: "file".into(),
                data: data.clone(),
                sha256: sha256.clone(),
            }],
        )
        .unwrap();
    let snapshot = store
        .load_project_team_conversation("alice", &team.id, None, 50)
        .unwrap();
    let message = snapshot
        .messages
        .iter()
        .find(|message| message.id == historical.message_id)
        .unwrap();
    assert!(message.content.is_empty());
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(message.attachments[0].name, "设计图.png");
    assert_eq!(message.attachments[0].kind, "image");
    let persisted = store
        .load_message_attachment_payloads("alice", &historical.message_id)
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].data, data);
    assert_eq!(persisted[0].sha256, sha256);

    let historical_delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery(
            "alice",
            &historical_delivery,
            "turn-historical-binary",
            "run-historical-binary",
            20,
        )
        .unwrap();
    store
        .fail_project_team_delivery(
            "alice",
            &historical_delivery.delivery_id,
            "continue with a newer trigger",
        )
        .unwrap();
    store
        .queue_project_team_message_with_attachments(
            "alice",
            &team.id,
            "请检查最新图片",
            &["agent-1".into()],
            &[crate::conversation_attachments::VerifiedAttachmentPayload {
                display_name: "最新设计图.png".into(),
                media_type: "image/png".into(),
                kind: "image".into(),
                origin: "file".into(),
                data,
                sha256,
            }],
        )
        .unwrap();
    let current_delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery(
            "alice",
            &current_delivery,
            "turn-current-binary",
            "run-current-binary",
            21,
        )
        .unwrap();
    let (_, unread) = store
        .read_all_unread_team_chat("alice", "run-current-binary", "read-binary", 20, 22)
        .unwrap();
    let unread_messages = unread["conversations"][0]["messages"].as_array().unwrap();
    let historical_message = unread_messages
        .iter()
        .find(|message| message["content"] == "")
        .unwrap();
    let current_message = unread_messages
        .iter()
        .find(|message| message["content"] == "请检查最新图片")
        .unwrap();
    let (_, historical_attachment) = store
        .read_agent_chat_attachment(
            "alice",
            "run-current-binary",
            historical_message["message_ref"].as_str().unwrap(),
            historical_message["attachments"][0]["attachment_ref"]
                .as_str()
                .unwrap(),
            0,
            12_000,
        )
        .unwrap();
    let (_, current_attachment) = store
        .read_agent_chat_attachment(
            "alice",
            "run-current-binary",
            current_message["message_ref"].as_str().unwrap(),
            current_message["attachments"][0]["attachment_ref"]
                .as_str()
                .unwrap(),
            0,
            12_000,
        )
        .unwrap();
    assert_eq!(historical_attachment["content"], serde_json::Value::Null);
    assert_eq!(historical_attachment["multimodal_on_trigger"], false);
    assert_eq!(current_attachment["content"], serde_json::Value::Null);
    assert_eq!(current_attachment["multimodal_on_trigger"], true);
}

#[cfg(unix)]
#[test]
fn confined_connector_directory_rejects_symlinked_roots() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let actual = directory.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let linked_root = directory.path().join("linked-root");
    symlink(&actual, &linked_root).unwrap();
    assert!(confined_connector_directory(&linked_root).is_err());

    let root = directory.path().join("root");
    fs::create_dir(&root).unwrap();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    symlink(&target, root.join("NativeConnector")).unwrap();
    assert!(confined_connector_directory(&root).is_err());
}

#[test]
fn confined_connector_directory_rejects_non_directory_paths() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("not-a-directory");
    fs::write(&file, b"file").unwrap();
    assert!(connector_storage::reject_link_or_non_directory(&file).is_err());
}
