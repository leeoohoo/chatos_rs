// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, LocalConversationStore,
    RunTransition, SqliteClientStorage,
};
use chatos_local_agent_protocol::{
    CreateConversationCommand, CreateTaskGraphCommand, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalConversationTurnStatus, LocalTaskSpec, StartConversationTurnCommand,
};
use serde_json::{json, Value};

fn command(command_id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: command_id.to_string(),
        request_fingerprint: command_id.to_string(),
        persist_receipt: true,
    }
}

fn conversation_run(turn: &StartConversationTurnCommand) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: turn.run_id.clone(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation_turn".to_string(),
        owner_entity_id: turn.turn_id.clone(),
        profile_key: "main_chat".to_string(),
        model_config_ref: turn.model_config_ref.clone(),
        model_config_revision: turn.model_config_revision.clone(),
        capability_policy_revision: turn.capability_policy_revision.clone(),
        input: json!({"message": turn.message, "attachments": []}),
        status: LocalAgentRunStatus::Queued,
        iteration: 0,
        model_attempt: 1,
        max_iterations: turn.max_iterations,
        version: 1,
        claim_token: None,
        claim_until_unix_ms: None,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: Value::Null,
        continuation_input: None,
        terminal_outcome: None,
        created_at_unix_ms: 2_000,
        updated_at_unix_ms: 2_000,
    }
}

#[test]
fn completed_callback_preserves_the_original_objective_fallback() {
    let content = super::callback_content(
        "Inspect project",
        "Inspect the bound project and explain how to run it.",
        "task.completed",
        None,
        true,
    );

    assert_eq!(
        content,
        "Inspect the bound project and explain how to run it."
    );
}

#[test]
fn callback_detail_hides_runtime_secrets_ids_and_transient_errors() {
    assert_eq!(
        super::sanitize_visible_detail(
            "authorization: Bearer secret-token\nreasoning: private",
            true
        ),
        None
    );
    assert_eq!(
        super::sanitize_visible_detail(
            "Completed the inspection.\npassword=real-secret-value\nNo files changed.",
            true
        ),
        Some("Completed the inspection.\nNo files changed.".to_string())
    );
    assert_eq!(
        super::sanitize_visible_detail(
            "Completed the inspection.\nSend Bearer actual-secret-token to the API.",
            true
        ),
        Some("Completed the inspection.".to_string())
    );
    assert_eq!(
        super::sanitize_visible_detail("任务 `123e4567-e89b-12d3-a456-426614174000` 已完成", false),
        Some("任务 已完成".to_string())
    );
    assert_eq!(
        super::sanitize_visible_detail("upstream returned status 503", true),
        Some("The service is temporarily unavailable. Please try again later.".to_string())
    );
}

#[test]
fn completed_callback_keeps_documented_authentication_placeholders() {
    let outcome = json!({
        "content": "## 扫码报工当前流程\n\n确认结论：扫码入口按二维码类型分流。\n\nAPI 客户端从 storage 读取 `wms_access_token` 并附带 `Authorization: Bearer ...`，401 会清除 token。"
    });

    let content = super::callback_content(
        "梳理扫码报工逻辑",
        "检查扫码报工逻辑",
        "task.completed",
        Some(&outcome),
        false,
    );

    assert!(content.contains("扫码报工当前流程"));
    assert!(content.contains("确认结论：扫码入口按二维码类型分流。"));
    assert_ne!(content, "请求失败，请稍后重试。");
}

async fn finish_task_run(
    storage: &SqliteClientStorage,
    run_id: &str,
    status: LocalAgentRunStatus,
    now_unix_ms: i64,
) {
    storage
        .start_next_task_run("user-1", run_id, &format!("start-{run_id}"), now_unix_ms)
        .await
        .expect("start Task Run")
        .expect("runnable Task");
    let claim = storage
        .claim_next_run(
            &command(&format!("claim-{run_id}")),
            "user-1",
            "worker-1",
            &format!("token-{run_id}"),
            now_unix_ms + 1,
            now_unix_ms + 10_000,
            &format!("claim-event-{run_id}"),
        )
        .await
        .expect("claim Task Run")
        .expect("Task Run claim");
    storage
        .apply_transition(
            &command(&format!("finish-{run_id}")),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: status,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({
                    "content": format!("Result from {run_id}"),
                    "reasoning": "internal reasoning must not be displayed",
                })),
                event_id: format!("finish-event-{run_id}"),
                event_type: format!("run_{}", status.as_str()),
                event_payload: json!({"status": status}),
                occurred_at_unix_ms: now_unix_ms + 2,
            },
        )
        .await
        .expect("finish Task Run");
}

#[tokio::test]
async fn main_chat_handoff_reply_is_persisted_before_task_started_callback() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_conversation(
            &command("create-order-conversation"),
            &CreateConversationCommand {
                conversation_id: "conversation-order".to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Ordering".to_string(),
                resource: None,
            },
            1_000,
        )
        .await
        .expect("create Conversation");
    let turn = StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-order".to_string(),
        expected_conversation_version: 1,
        turn_id: "turn-order".to_string(),
        message_id: "message-order".to_string(),
        run_id: "conversation-run-order".to_string(),
        message: "Do the task".to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    };
    storage
        .start_conversation_turn(
            &command("start-order-turn"),
            &turn,
            &conversation_run(&turn),
            "start-order-event",
            2_000,
        )
        .await
        .expect("start Turn");
    let main_claim = storage
        .claim_next_run(
            &command("claim-order-main"),
            "user-1",
            "worker-main",
            "token-main",
            2_100,
            20_000,
            "claim-order-main-event",
        )
        .await
        .expect("claim Main Chat")
        .expect("Main Chat claim");
    storage
        .create_task_graph(
            &command("create-order-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-order".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: "turn-order".to_string(),
                tasks: vec![LocalTaskSpec {
                    task_id: "task-order".to_string(),
                    title: "Ordered Task".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"prompt": "work"}),
                    max_iterations: 8,
                }],
                dependencies: Vec::new(),
            },
            2_200,
        )
        .await
        .expect("create Task Graph");
    storage
        .start_next_task_run("user-1", "task-run-order", "task-run-order-event", 2_300)
        .await
        .expect("start Task Run")
        .expect("Task Run");
    let before_handoff = storage
        .get_conversation("user-1", "conversation-order")
        .await
        .expect("load Conversation")
        .expect("Conversation");
    assert_eq!(before_handoff.messages.len(), 1);

    storage
        .apply_transition(
            &command("finish-order-main"),
            &RunTransition {
                run_id: main_claim.run.run_id,
                claim_token: main_claim.claim_token,
                expected_version: main_claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::Succeeded,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"content": "I've started working on it."})),
                event_id: "finish-order-main-event".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"status": "succeeded"}),
                occurred_at_unix_ms: 2_400,
            },
        )
        .await
        .expect("finish Main Chat");
    storage
        .claim_next_run(
            &command("claim-order-task"),
            "user-1",
            "worker-task",
            "token-task",
            2_500,
            20_000,
            "claim-order-task-event",
        )
        .await
        .expect("claim Task")
        .expect("Task claim");

    let conversation = storage
        .get_conversation("user-1", "conversation-order")
        .await
        .expect("load Conversation")
        .expect("Conversation");
    assert_eq!(conversation.messages.len(), 3);
    assert_eq!(
        conversation.messages[1].content["content"],
        "I've started working on it."
    );
    assert_eq!(
        conversation.messages[2].metadata["task_runner_async"]["event"],
        "task.run.started"
    );
}

#[tokio::test]
async fn task_runs_write_back_original_task_callback_protocol() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_conversation(
            &command("create-conversation"),
            &CreateConversationCommand {
                conversation_id: "conversation-1".to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Local conversation".to_string(),
                resource: None,
            },
            1_000,
        )
        .await
        .expect("create Conversation");
    let turn = StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version: 1,
        turn_id: "turn-1".to_string(),
        message_id: "message-1".to_string(),
        run_id: "conversation-run-1".to_string(),
        message: "Do the task".to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    };
    storage
        .start_conversation_turn(
            &command("start-turn"),
            &turn,
            &conversation_run(&turn),
            "start-turn-event",
            2_000,
        )
        .await
        .expect("start Turn");
    storage
        .cancel_run(
            &command("cancel-conversation-run"),
            &turn.run_id,
            Some(1),
            "main response completed elsewhere",
            "cancel-conversation-event",
            3_000,
        )
        .await
        .expect("close source Turn");
    storage
        .create_task_graph(
            &command("create-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-1".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: "turn-1".to_string(),
                tasks: vec![LocalTaskSpec {
                    task_id: "task-1".to_string(),
                    title: "Inspect project".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({
                        "prompt": "inspect",
                        "objective": "Inspect the bound project and explain how to run it.",
                        "schedule": {"mode": "contact_async"}
                    }),
                    max_iterations: 8,
                }],
                dependencies: Vec::new(),
            },
            4_000,
        )
        .await
        .expect("create Task Graph");

    finish_task_run(
        &storage,
        "task-run-failed",
        LocalAgentRunStatus::Failed,
        5_000,
    )
    .await;
    let failed_version = storage
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("load failed Task Graph")
        .expect("failed Task Graph")
        .tasks[0]
        .version;
    storage
        .retry_task(
            &command("retry-task"),
            "user-1",
            "task-1",
            failed_version,
            None,
            6_000,
        )
        .await
        .expect("retry Task");
    finish_task_run(
        &storage,
        "task-run-succeeded",
        LocalAgentRunStatus::Succeeded,
        7_000,
    )
    .await;

    storage
        .create_task_graph(
            &command("create-cancelled-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-cancelled".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: "turn-1".to_string(),
                tasks: vec![LocalTaskSpec {
                    task_id: "task-cancelled".to_string(),
                    title: "Cancelled task".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"prompt": "cancel"}),
                    max_iterations: 8,
                }],
                dependencies: Vec::new(),
            },
            8_000,
        )
        .await
        .expect("create cancellable Task Graph");
    storage
        .cancel_task(
            &command("cancel-task"),
            "user-1",
            "task-cancelled",
            Some(1),
            "no longer needed",
            &[],
            "unused-run-event",
            9_000,
        )
        .await
        .expect("cancel pending Task");

    storage
        .create_task_graph(
            &command("create-replaced-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-replaced".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: "turn-1".to_string(),
                tasks: vec![LocalTaskSpec {
                    task_id: "task-replaced".to_string(),
                    title: "Superseded task".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"prompt": "old plan"}),
                    max_iterations: 8,
                }],
                dependencies: Vec::new(),
            },
            10_000,
        )
        .await
        .expect("create superseded Task Graph");
    storage
        .cancel_task(
            &command("cancel-replaced-task"),
            "user-1",
            "task-replaced",
            Some(1),
            "superseded by the replacement plan",
            &["task-1".to_string()],
            "unused-replacement-run-event",
            11_000,
        )
        .await
        .expect("cancel superseded Task");

    let conversation = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .expect("load Conversation")
        .expect("Conversation");
    assert_eq!(conversation.conversation.version, 8);
    assert_eq!(
        conversation.turns[0].status,
        LocalConversationTurnStatus::Cancelled
    );
    assert_eq!(conversation.messages.len(), 4);
    assert!(conversation
        .messages
        .iter()
        .all(|message| !message.message_id.contains("task-replaced")));
    assert_eq!(
        conversation.messages[1].message_id,
        "task_runner_callback::message-1::task-1::task-run-failed"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["event"],
        "task.failed"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["run_id"],
        "task-run-failed"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["task_objective"],
        "Inspect the bound project and explain how to run it."
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["task_status"],
        "failed"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["schedule_mode"],
        "contact_async"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["fallback_locale"],
        "en-US"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["detail_source"],
        "content"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["error_message"],
        "Result from task-run-failed"
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["started_at_unix_ms"],
        5_001
    );
    assert_eq!(
        conversation.messages[1].metadata["task_runner_async"]["finished_at_unix_ms"],
        5_002
    );
    assert_eq!(
        conversation.messages[1].content,
        "I couldn't complete this.\n\nResult from task-run-failed"
    );
    assert!(!conversation.messages[1]
        .content
        .as_str()
        .expect("callback text")
        .contains("internal reasoning"));
    assert_eq!(
        conversation.messages[2].message_id,
        "task_runner_callback::message-1::task-1::task-run-succeeded"
    );
    assert_eq!(
        conversation.messages[2].metadata["task_runner_async"]["event"],
        "task.completed"
    );
    assert_eq!(
        conversation.messages[2].metadata["task_runner_async"]["result_summary"],
        "Result from task-run-succeeded"
    );
    assert_eq!(
        conversation.messages[2].content,
        "Result from task-run-succeeded"
    );
    assert_eq!(
        conversation.messages[3].message_id,
        "task_runner_callback::message-1::task-cancelled::cancelled"
    );
    assert_eq!(
        conversation.messages[3].metadata["task_runner_async"]["event"],
        "task.cancelled"
    );
    assert!(conversation.messages.iter().skip(1).all(|message| {
        message.metadata["kind"] == "task_execution_callback"
            && message.metadata["task_runner_async"]["source_session_id"] == "conversation-1"
            && message.metadata["task_runner_async"]["source_turn_id"] == "turn-1"
            && message.metadata["task_runner_async"]["source_user_message_id"] == "message-1"
    }));
    let writebacks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_task_graph_writebacks WHERE graph_id = 'graph-1'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("count writebacks");
    assert_eq!(writebacks, 0);
    let cancelled_writebacks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_task_graph_writebacks \
             WHERE graph_id = 'graph-cancelled'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("count cancelled writebacks");
    assert_eq!(cancelled_writebacks, 0);
    let events = storage
        .list_events(0, 100, None)
        .await
        .expect("list callback events");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_callback_written_back")
            .count(),
        5
    );
}
