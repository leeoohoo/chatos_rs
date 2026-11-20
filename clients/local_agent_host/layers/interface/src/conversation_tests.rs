use super::*;
use serde_json::json;

fn attachment(attachment_id: &str) -> LocalConversationAttachmentSpec {
    LocalConversationAttachmentSpec {
        attachment_id: attachment_id.to_string(),
        display_name: "brief.pdf".to_string(),
        media_type: "application/pdf".to_string(),
        byte_size: 42,
        sha256: "a".repeat(64),
        authorized_local_ref: "local-attachment:authority-1".to_string(),
        metadata: json!({}),
    }
}

fn command() -> StartConversationTurnCommand {
    StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version: 1,
        conversation_if_missing: None,
        turn_id: "turn-1".to_string(),
        message_id: "message-1".to_string(),
        run_id: "run-1".to_string(),
        message: String::new(),
        message_metadata: json!({}),
        attachments: vec![attachment("attachment-1")],
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    }
}

#[test]
fn attachment_only_turn_is_valid_but_duplicate_or_noncanonical_hash_is_not() {
    assert!(command().validate().is_ok());

    let mut duplicate = command();
    duplicate.attachments.push(attachment("attachment-1"));
    assert!(duplicate.validate().is_err());

    let mut uppercase_hash = command();
    uppercase_hash.attachments[0].sha256 = "A".repeat(64);
    assert!(uppercase_hash.validate().is_err());

    let mut raw_path = command();
    raw_path.attachments[0].authorized_local_ref = "/tmp/brief.pdf".to_string();
    assert!(raw_path.validate().is_err());
}

#[test]
fn conversation_turn_updates_require_versions_and_resumable_status() {
    let mut resume = ResumeConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version: 2,
        turn_id: "turn-1".to_string(),
        expected_run_version: 3,
        expected_run_status: crate::LocalAgentRunStatus::WaitingUser,
        message_id: "message-2".to_string(),
        message: "yes".to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        reason: "user replied".to_string(),
    };
    assert!(resume.validate().is_ok());
    resume.expected_run_status = crate::LocalAgentRunStatus::Queued;
    assert!(resume.validate().is_err());
    resume.expected_run_status = crate::LocalAgentRunStatus::WaitingUser;
    resume.expected_conversation_version = 0;
    assert!(resume.validate().is_err());

    assert!(CancelConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version: 2,
        turn_id: "turn-1".to_string(),
        expected_run_version: Some(3),
        reason: "user stopped".to_string(),
    }
    .validate()
    .is_ok());

    assert!(GuideConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version: 2,
        turn_id: "turn-1".to_string(),
        expected_run_version: None,
        message_id: "message-guidance".to_string(),
        message: "also inspect the tests".to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
    }
    .validate()
    .is_ok());
}

#[test]
fn history_page_requires_a_bounded_positive_cursor() {
    assert!(GetConversationHistoryCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        before_cursor: None,
        limit: LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE,
    }
    .validate()
    .is_ok());
    assert!(GetConversationHistoryCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        before_cursor: Some("".to_string()),
        limit: 10,
    }
    .validate()
    .is_err());
}

#[test]
fn conversation_list_cursor_is_paired_and_owner_scoped() {
    let valid = ListConversationsCommand {
        owner_user_id: "user-1".to_string(),
        before_updated_at_unix_ms: Some(1_000),
        before_conversation_id: Some("conversation-1".to_string()),
        limit: 50,
    };
    assert!(valid.validate().is_ok());
    assert!(ListConversationsCommand {
        before_conversation_id: None,
        ..valid.clone()
    }
    .validate()
    .is_err());
    assert!(ListConversationsCommand {
        owner_user_id: String::new(),
        ..valid.clone()
    }
    .validate()
    .is_err());
    assert!(ListConversationsCommand {
        limit: 201,
        ..valid
    }
    .validate()
    .is_err());
}
