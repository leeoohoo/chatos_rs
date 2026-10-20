// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_protocol::{GetConversationCommand, GuideConversationTurnCommand};

#[tokio::test]
async fn invalid_review_tool_result_resumes_the_model_and_delivers_a_normal_reply() {
    use super::super::{
        LocalAgentScheduler, LocalToolRegistry, LocalToolScheduler, ToolSchedulerTick,
    };
    use chatos_local_agent_protocol::{
        LocalAgentRunClaim, LocalAgentRunStatus, LocalAgentStepOutcome, LocalAgentToolCall,
    };
    use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry};

    struct StatusQuestionProfile;
    #[async_trait::async_trait]
    impl LocalAgentProfile for StatusQuestionProfile {
        async fn execute_step(
            &self,
            claim: &LocalAgentRunClaim,
        ) -> Result<LocalAgentStepOutcome, String> {
            if let Some(results) = &claim.run.continuation_input {
                assert_eq!(results["invocations"][0]["status"], "failed");
                assert!(results["invocations"][0]["error"]
                    .as_str()
                    .unwrap()
                    .contains("no pending task_followup_review"));
                return Ok(LocalAgentStepOutcome::Succeed {
                    output: json!({"content": "No task review is pending. Here is the current status."}),
                });
            }
            Ok(LocalAgentStepOutcome::WaitForTool {
                batch_id: "invalid-review-batch".into(),
                tool_calls: vec![LocalAgentToolCall {
                    call_id: "invalid-review-call".into(),
                    tool_name: "resolve_task_followup".into(),
                    arguments: json!({"review_message_id": "invented-message", "decisions": [
                        {"task_id": "historical-task", "action": "continue", "reason": "Status inquiry"}
                    ]}),
                    side_effecting: true,
                    requires_approval: false,
                }],
                checkpoint: json!({"step": "status inquiry"}),
            })
        }
    }

    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.unwrap();
    add_project_conversation(
        &runtime,
        "conversation-1",
        "project-1",
        "turn-1",
        "parent-run",
    )
    .await;
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", StatusQuestionProfile)
        .unwrap();
    let model =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker").unwrap();
    let mut tools = LocalToolRegistry::new();
    tools
        .register(
            "resolve_task_followup",
            LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap(),
        )
        .unwrap();
    let tool =
        LocalToolScheduler::new(Arc::clone(&runtime), tools, "user-1", "tool-worker").unwrap();

    model.run_once().await.unwrap();
    let ToolSchedulerTick::Committed(result) = tool.run_once().await.unwrap() else {
        panic!("expected tool result");
    };
    assert_eq!(result.invocation.status, LocalAgentToolStatus::Failed);
    assert_eq!(result.run.status, LocalAgentRunStatus::ContinuationReady);
    model.run_once().await.unwrap();
    let run = runtime
        .get_run_for_host_worker("parent-run")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, LocalAgentRunStatus::Succeeded);
    let HostResult::Conversation { conversation } = runtime
        .try_handle(envelope(
            "final-conversation",
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
            }),
        ))
        .await
        .unwrap()
    else {
        panic!("conversation result")
    };
    assert_eq!(
        conversation.messages.last().unwrap().content["content"],
        "No task review is pending. Here is the current status."
    );
}

async fn guide_review(runtime: &LocalAgentRuntime, message_id: &str) {
    let HostResult::Conversation { conversation } = runtime
        .try_handle(envelope(
            &format!("detail-{message_id}"),
            HostCommand::GetConversation(GetConversationCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
            }),
        ))
        .await
        .unwrap()
    else {
        panic!("conversation result");
    };
    runtime
        .try_handle(envelope(
            &format!("guide-{message_id}"),
            HostCommand::GuideConversationTurn(GuideConversationTurnCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
                expected_conversation_version: conversation.conversation.version,
                turn_id: "turn-1".into(),
                expected_run_version: None,
                message_id: message_id.into(),
                message: "Is the work finished?".into(),
                message_metadata: json!({}),
                attachments: vec![],
            }),
        ))
        .await
        .unwrap();
}

fn review_invocation(
    id: &str,
    message: &str,
    task_ids: &[Value],
) -> LocalAgentToolInvocationRecord {
    let mut call = invocation(
        id,
        "parent-run",
        "resolve_task_followup",
        json!({
            "review_message_id": message,
            "decisions": task_ids.iter().map(|task_id| json!({
                "task_id": task_id, "action": "cancel", "reason": "Changed requirements"
            })).collect::<Vec<_>>()
        }),
    );
    call.side_effecting = true;
    call
}

#[tokio::test]
async fn invented_followup_without_pending_review_is_a_known_failure_and_status_remains_readable() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    let outcome = executor
        .execute_tool(&review_invocation(
            "invented",
            "historical-message",
            &[json!("old-task")],
        ))
        .await
        .expect("invalid follow-up is recoverable, not an unknown side effect");
    let LocalAgentToolOutcome::Failed { error, detail } = outcome else {
        panic!("expected known failure: {outcome:?}");
    };
    assert!(error.contains("no pending task_followup_review"));
    assert_eq!(detail["phase"], "task_followup_review");
    assert_eq!(detail["side_effects_started"], false);
    let tasks = succeeded_output(
        &executor,
        invocation("read-status", "parent-run", LIST_TASKS_TOOL, json!({})),
    )
    .await;
    assert_eq!(tasks, json!([]));
}

#[tokio::test]
async fn invalid_pending_reviews_do_not_cancel_tasks_and_can_be_corrected() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    let graph = succeeded_output(
        &executor,
        invocation(
            "batch",
            "parent-run",
            CREATE_TASKS_TOOL,
            json!({"tasks": [
                {"client_ref": "one", "title": "One", "objective": "First output",
                 "requires_execution": false, "enabled_builtin_kinds": []},
                {"client_ref": "two", "title": "Two", "objective": "Second output",
                 "requires_execution": false, "enabled_builtin_kinds": []}
            ]}),
        ),
    )
    .await;
    let ids = vec![
        graph["created_tasks"][0]["task_id"].clone(),
        graph["created_tasks"][1]["task_id"].clone(),
    ];
    guide_review(&runtime, "current").await;
    for call in [
        review_invocation("stale", "old-message", &ids),
        review_invocation("omitted", "current", &ids[..1]),
        review_invocation(
            "foreign",
            "current",
            &[ids[0].clone(), json!("foreign-task")],
        ),
        review_invocation("duplicate", "current", &[ids[0].clone(), ids[0].clone()]),
    ] {
        let error = preparation_error(executor.execute_tool(&call).await.unwrap());
        assert!(!error.is_empty());
        let parent = runtime
            .get_run_for_host_worker("parent-run")
            .await
            .unwrap()
            .unwrap();
        let review = &parent.input["task_followup_review"];
        assert_eq!(review["pending"], true);
        assert!(review["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|task| task["review_state"] == "pending" && task["status"] != "cancelled"));
    }
    let reviewed =
        succeeded_output(&executor, review_invocation("corrected", "current", &ids)).await;
    assert_eq!(reviewed["reviewed"], true);
    let error = preparation_error(
        executor
            .execute_tool(&review_invocation("already-resolved", "current", &ids))
            .await
            .unwrap(),
    );
    assert!(error.contains("no pending task_followup_review"));
}

#[tokio::test]
async fn review_replaced_after_preflight_is_a_known_transaction_rejection() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    let original = succeeded_output(
        &executor,
        invocation(
            "original",
            "parent-run",
            CREATE_TASK_TOOL,
            json!({"title": "Inspect", "objective": "Inspect", "requires_execution": false,
            "enabled_builtin_kinds": []}),
        ),
    )
    .await;
    guide_review(&runtime, "old-review").await;
    let stale_parent = runtime
        .get_run_for_host_worker("parent-run")
        .await
        .unwrap()
        .unwrap();
    guide_review(&runtime, "new-review").await;
    let mut side_effects_started = false;
    let error = super::super::task_followup_tool::resolve(
        &executor,
        &review_invocation("stale-snapshot", "old-review", &[original["id"].clone()]),
        &stale_parent,
        &mut side_effects_started,
    )
    .await
    .unwrap_err();
    assert!(error.contains("storage conflict"));
    assert!(
        !side_effects_started,
        "rolled-back conflict must not become NeedsReview"
    );
    let current = runtime
        .get_run_for_host_worker("parent-run")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        current.input["task_followup_review"]["review_message_id"],
        "new-review"
    );
    assert_eq!(current.input["task_followup_review"]["pending"], true);
    assert_ne!(
        current.input["task_followup_review"]["tasks"][0]["status"],
        "cancelled"
    );
    let corrected = succeeded_output(
        &executor,
        review_invocation("current-review", "new-review", &[original["id"].clone()]),
    )
    .await;
    assert_eq!(corrected["reviewed"], true);
}

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
