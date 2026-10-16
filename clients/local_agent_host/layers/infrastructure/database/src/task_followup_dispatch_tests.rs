// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn resume_message_also_requires_task_review_and_iteration_exhaustion_is_visible() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    fixture(&storage).await;
    let main = storage.get_run("run-original").await.unwrap().unwrap();
    let main = LocalAgentRunClaim {
        worker_id: "worker".into(),
        claim_token: main.claim_token.clone().unwrap(),
        run: main,
    };
    let mut wait = transition(&main, LocalAgentRunStatus::WaitingUser, "wait-main");
    wait.terminal_outcome = None;
    let waited = storage
        .apply_transition(&idempotency("wait-main"), &wait)
        .await
        .unwrap();
    let detail = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .unwrap()
        .unwrap();
    storage
        .resume_conversation_turn(
            &idempotency("resume-followup"),
            &ResumeConversationTurnCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
                expected_conversation_version: detail.conversation.version,
                turn_id: "turn-original".into(),
                expected_run_version: waited.version,
                expected_run_status: waited.status,
                message_id: "resumed-message".into(),
                message: "Stop publishing".into(),
                message_metadata: json!({}),
                attachments: vec![],
                reason: "user clarified".into(),
            },
            &json!({"message": "Stop publishing"}),
            "resume-main",
            21_000,
        )
        .await
        .unwrap();
    let main = claim(&storage, "resumed-review", 22_000).await.unwrap();
    assert_eq!(
        main.run.input["task_followup_review"]["review_message_id"],
        "resumed-message"
    );
    let stopped = storage
        .apply_transition(
            &idempotency("exhausted-review"),
            &transition(&main, LocalAgentRunStatus::NeedsReview, "exhausted-review"),
        )
        .await
        .unwrap();
    assert_eq!(stopped.status, LocalAgentRunStatus::Failed);
    let history = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .unwrap()
        .unwrap();
    assert!(history
        .messages
        .iter()
        .any(|message| message.content.to_string().contains("尚未完成关联任务审查")));
    assert!(claim(&storage, "still-held", 23_000).await.is_none());
}

#[tokio::test]
async fn dispatched_tool_result_is_preserved_but_pending_tool_cannot_dispatch_until_review() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let task = fixture(&storage).await;
    let batch = LocalAgentToolBatch {
        batch_id: "batch".into(),
        calls: ["one", "two"]
            .iter()
            .map(|id| LocalAgentToolCall {
                call_id: (*id).into(),
                tool_name: format!("external-{id}"),
                arguments: json!({}),
                side_effecting: true,
                requires_approval: false,
            })
            .collect(),
    };
    let mut request = transition(&task, LocalAgentRunStatus::WaitingToolResult, "batch");
    request.tool_batch = Some(batch);
    request.pending_tool_batch = Some(json!({"batch_id": "batch"}));
    request.terminal_outcome = None;
    storage
        .apply_transition(&idempotency("batch"), &request)
        .await
        .unwrap();
    let tool = storage
        .claim_next_tool(
            &idempotency("claim-one"),
            "user-1",
            "worker",
            "tool-token",
            7_000,
            107_000,
            "tool-claimed",
            Some(&["external-one".into()]),
            &[],
            None,
        )
        .await
        .unwrap()
        .unwrap();
    guide(&storage, "tool-review").await;
    storage
        .commit_tool(
            &idempotency("commit-one"),
            "user-1",
            &tool.invocation.invocation_id,
            &tool.claim_token,
            tool.invocation.version,
            &LocalAgentToolOutcome::Succeeded {
                output: json!({"effect": "completed"}),
            },
            "one-finished",
            "batch-finished",
            11_000,
        )
        .await
        .unwrap();
    assert!(storage
        .claim_next_tool(
            &idempotency("blocked-two"),
            "user-1",
            "worker",
            "blocked-tool",
            12_000,
            112_000,
            "blocked-tool",
            None,
            &[],
            None,
        )
        .await
        .unwrap()
        .is_none());
    storage
        .resolve_task_followup(
            &idempotency("continue-tools"),
            &unchanged("tool-review"),
            13_000,
        )
        .await
        .unwrap();
    let next = storage
        .claim_next_tool(
            &idempotency("claim-two"),
            "user-1",
            "worker",
            "next-tool",
            14_000,
            114_000,
            "next-tool",
            None,
            &[],
            None,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.invocation.call_id, "two");
    assert_eq!(
        storage
            .successful_tool_invocation_count("run-task", "external-one")
            .await
            .unwrap(),
        1
    );
    storage
        .commit_tool(
            &idempotency("commit-two"),
            "user-1",
            &next.invocation.invocation_id,
            &next.claim_token,
            next.invocation.version,
            &LocalAgentToolOutcome::Succeeded {
                output: json!({"effect": "second"}),
            },
            "two-finished",
            "all-finished",
            15_000,
        )
        .await
        .unwrap();
    assert!(storage
        .claim_next_tool(
            &idempotency("no-replay"),
            "user-1",
            "worker",
            "no-replay",
            16_000,
            116_000,
            "no-replay",
            None,
            &[],
            None,
        )
        .await
        .unwrap()
        .is_none());
    let task = storage.get_run("run-task").await.unwrap().unwrap();
    assert_eq!(task.checkpoint, json!({"history": "preserved"}));
    assert!(task
        .continuation_input
        .unwrap()
        .to_string()
        .contains("completed"));
}

#[tokio::test]
async fn review_gate_survives_database_reopen() {
    let path =
        std::env::temp_dir().join(format!("chatos-followup-{}.sqlite", uuid::Uuid::new_v4()));
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    fixture(&storage).await;
    guide(&storage, "restart-review").await;
    storage.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    blocked(&reopened).await;
    reopened
        .resolve_task_followup(
            &idempotency("restart-release"),
            &unchanged("restart-review"),
            13_000,
        )
        .await
        .unwrap();
    assert_eq!(
        claim(&reopened, "restart-task", 14_000)
            .await
            .unwrap()
            .run
            .run_id,
        "run-task"
    );
    reopened.pool.close().await;
    std::fs::remove_file(path).unwrap();
}
