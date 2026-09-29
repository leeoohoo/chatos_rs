// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, GetConversationCommand,
    GetConversationHistoryCommand, HostCommand, HostRequestEnvelope, HostResult,
    ListConversationsCommand, StartConversationTurnCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::Arc;

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn create(conversation_id: &str, owner_user_id: &str) -> HostCommand {
    HostCommand::CreateConversation(CreateConversationCommand {
        conversation_id: conversation_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        title: conversation_id.to_string(),
    })
}

fn list(
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_conversation_id: Option<&str>,
    limit: u32,
) -> HostCommand {
    HostCommand::ListConversations(ListConversationsCommand {
        owner_user_id: owner_user_id.to_string(),
        before_updated_at_unix_ms,
        before_conversation_id: before_conversation_id.map(str::to_string),
        limit,
    })
}

#[tokio::test]
async fn pages_owner_conversations_and_rejects_cross_account_access() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    for (command_id, command) in [
        ("create-a", create("conversation-a", "user-1")),
        ("create-b", create("conversation-b", "user-1")),
        ("create-z", create("conversation-z", "user-2")),
    ] {
        runtime
            .try_handle(request(command_id, command))
            .await
            .expect("create Conversation");
    }

    let first = runtime
        .try_handle(request("list-first", list("user-1", None, None, 1)))
        .await
        .expect("first page");
    let HostResult::Conversations { page } = first else {
        panic!("expected Conversation page")
    };
    assert_eq!(page.conversations.len(), 1);
    assert_eq!(page.conversations[0].conversation_id, "conversation-a");
    let cursor_time = page.next_before_updated_at_unix_ms.expect("next timestamp");
    let cursor_id = page
        .next_before_conversation_id
        .expect("next Conversation id");

    let second = runtime
        .try_handle(request(
            "list-second",
            list("user-1", Some(cursor_time), Some(&cursor_id), 1),
        ))
        .await
        .expect("second page");
    assert!(matches!(
        second,
        HostResult::Conversations { page }
            if page.conversations.len() == 1
                && page.conversations[0].conversation_id == "conversation-b"
                && page.next_before_conversation_id.is_none()
    ));
    let other = runtime
        .try_handle(request("list-other", list("user-2", None, None, 10)))
        .await
        .expect("other account page");
    assert!(matches!(
        other,
        HostResult::Conversations { page }
            if page.conversations.len() == 1
                && page.conversations[0].conversation_id == "conversation-z"
    ));

    assert!(runtime
        .try_handle(request(
            "cross-account-detail",
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-2".to_string(),
                conversation_id: "conversation-a".to_string(),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-account-history",
            HostCommand::GetConversationHistory(GetConversationHistoryCommand {
                owner_user_id: "user-2".to_string(),
                conversation_id: "conversation-a".to_string(),
                before_ordinal: None,
                limit: 10,
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-account-start",
            HostCommand::StartConversationTurn(StartConversationTurnCommand {
                owner_user_id: "user-2".to_string(),
                conversation_id: "conversation-a".to_string(),
                expected_conversation_version: 1,
                turn_id: "turn-a".to_string(),
                message_id: "message-a".to_string(),
                run_id: "run-a".to_string(),
                message: "hello".to_string(),
                message_metadata: json!({}),
                attachments: Vec::new(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                max_iterations: 4,
            }),
        ))
        .await
        .is_err());

    runtime
        .try_handle(request(
            "start-owned",
            HostCommand::StartConversationTurn(StartConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: "conversation-a".to_string(),
                expected_conversation_version: 1,
                turn_id: "turn-owned".to_string(),
                message_id: "message-owned".to_string(),
                run_id: "run-owned".to_string(),
                message: "hello".to_string(),
                message_metadata: json!({}),
                attachments: Vec::new(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                max_iterations: 4,
            }),
        ))
        .await
        .expect("start owned Turn");
    assert!(runtime
        .try_handle(request(
            "cross-account-cancel",
            HostCommand::CancelConversationTurn(CancelConversationTurnCommand {
                owner_user_id: "user-2".to_string(),
                conversation_id: "conversation-a".to_string(),
                expected_conversation_version: 2,
                turn_id: "turn-owned".to_string(),
                expected_run_version: None,
                reason: "wrong account".to_string(),
            }),
        ))
        .await
        .is_err());
}
