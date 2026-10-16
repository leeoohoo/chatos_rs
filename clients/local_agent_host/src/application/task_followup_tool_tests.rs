// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_protocol::{GetConversationCommand, GuideConversationTurnCommand};

#[tokio::test]
async fn pending_review_cannot_be_bypassed_by_reusing_source_graph_and_host_tool_releases_it() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    let args = json!({"title": "Publish", "objective": "Publish", "requires_execution": false, "enabled_builtin_kinds": []});
    let original = succeeded_output(
        &executor,
        invocation("original", "parent-run", CREATE_TASK_TOOL, args.clone()),
    )
    .await;
    let detail = runtime
        .try_handle(envelope(
            "detail",
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
            }),
        ))
        .await
        .unwrap();
    let HostResult::Conversation { conversation } = detail else {
        panic!("conversation result");
    };
    runtime
        .try_handle(envelope(
            "guidance",
            HostCommand::GuideConversationTurn(GuideConversationTurnCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
                expected_conversation_version: conversation.conversation.version,
                turn_id: "turn-1".into(),
                expected_run_version: None,
                message_id: "latest".into(),
                message: "Stop publishing".into(),
                message_metadata: json!({}),
                attachments: vec![],
            }),
        ))
        .await
        .unwrap();
    let error = executor
        .execute_tool(&invocation(
            "bypass",
            "parent-run",
            CREATE_TASK_TOOL,
            args.clone(),
        ))
        .await
        .expect("pending review rejection precedes graph writes");
    let error = preparation_error(error);
    assert!(error.contains("task_followup_review_required"));
    let review = succeeded_output(&executor, invocation("review", "parent-run", "resolve_task_followup", json!({
        "review_message_id": "latest", "decisions": [{"task_id": original["id"], "action": "continue", "reason": "Status inquiry only"}]
    }))).await;
    assert_eq!(review["reviewed"], true);
    let new_work = succeeded_output(
        &executor,
        invocation("retry", "parent-run", CREATE_TASK_TOOL, args),
    )
    .await;
    assert_ne!(new_work["id"], original["id"]);
    let retry = succeeded_output(&executor, invocation("model-retry", "parent-run", CREATE_TASK_TOOL,
        json!({"title": "Publish", "objective": "Publish", "requires_execution": false, "enabled_builtin_kinds": []}))).await;
    assert_eq!(retry["id"], new_work["id"]);
}
