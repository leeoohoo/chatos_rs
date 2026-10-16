// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, LocalConversationStore,
    RunTransition, SqliteClientStorage,
};
use chatos_local_agent_protocol::{
    CreateConversationCommand, CreateTaskGraphCommand, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalConversationTurnStatus, LocalTaskDependency, LocalTaskGraphStatus, LocalTaskSpec,
    LocalTaskStatus, StartConversationTurnCommand,
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
fn completed_callback_keeps_managed_artifact_markdown_link_clickable() {
    let artifact_link = "[下载《收工提醒实施计划》](chatos-plugin-artifacts/agent-7766819f-98e3-45c7-9c2f-1754e080c9fa/pa_ae358dcf1e4140a3bbc01e78c61d4ce1/%E6%94%B6%E5%B7%A5%E6%8F%90%E9%86%92-V1-%E5%9B%9B%E4%BB%BB%E5%8A%A1%E5%AE%9E%E6%96%BD%E8%AE%A1%E5%88%92-solution-dc645096.md)";
    let content = super::callback_content(
        "Deliver plan",
        "Deliver the confirmed plan.",
        "task.completed",
        Some(&json!({
            "content": format!("## 已交付\n\n{artifact_link}\n\n### 交付摘要\n\n- 已保留两项需求和四个任务。")
        })),
        false,
    );

    assert!(
        content.contains(artifact_link),
        "the callback must not truncate a managed artifact URL inside Markdown"
    );
}

#[test]
fn completed_callback_finds_managed_artifact_link_after_preview_limit() {
    let artifact_link = "[下载 IAM 完整实施方案](chatos-plugin-artifacts/agent-iam/pa_solution/IAM-%E5%AE%8C%E6%95%B4%E5%AE%9E%E6%96%BD%E6%96%B9%E6%A1%88.md)";
    let content = super::callback_content(
        "Deliver IAM solution",
        "Deliver the complete IAM solution as a file.",
        "task.completed",
        Some(&json!({
            "content": format!(
                "## 正式实施方案已交付\n\n- **文件名**：IAM-完整实施方案.md\n- **格式**：Solution Studio 原生 Markdown\n- **版本**：v2.0\n- **日期**：2026-10-15\n- **打开 / 下载**：{artifact_link}\n\n文件正文已完整包含方案与研发任务。"
            )
        })),
        false,
    );

    assert!(
        content.starts_with(artifact_link),
        "managed delivery must be prioritized even when it appears after three preview lines: {content}"
    );
    assert!(
        content.contains("]("),
        "the managed Markdown destination must remain intact"
    );
}

#[test]
fn failed_task_display_distinguishes_execution_from_unverified_acceptance() {
    let unverified = json!({"task_acceptance": {"status": "failed", "reason": "Download not checked", "acceptance_checks": [
        {"criterion_index": 0, "status": "passed", "evidence": "Created draft"},
        {"criterion_index": 1, "status": "not_verified", "evidence": "Download not checked"}
    ]}});
    let rejected = json!({"task_acceptance": {"status": "failed", "reason": "Export missing", "acceptance_checks": [
        {"criterion_index": 0, "status": "failed", "evidence": "Export missing"}
    ]}});
    assert_eq!(
        super::task_display_status("failed", Some("succeeded"), Some(&unverified)),
        "evidence_incomplete"
    );
    assert_eq!(
        super::task_display_status("failed", Some("succeeded"), Some(&rejected)),
        "acceptance_failed"
    );
    assert_eq!(
        super::task_display_status("failed", Some("failed"), Some(&unverified)),
        "execution_failed"
    );
    assert_eq!(
        super::task_display_status("blocked", Some("succeeded"), Some(&unverified)),
        "blocked"
    );
    assert_eq!(
        super::task_display_status("succeeded", Some("succeeded"), None),
        "completed"
    );
    let content = super::callback_content_with_status(
        "Research",
        "Research the source",
        "task.failed",
        Some(&json!({"reason": "Publication date not verified"})),
        true,
        "evidence_incomplete",
    );
    assert!(content.starts_with("I completed part of this, but couldn't verify everything yet."));
    let summary = super::graph_summary_text(
        LocalTaskGraphStatus::Failed,
        &[
            json!({"title":"Research", "status":"failed", "display_status":"evidence_incomplete", "summary":"Publication date not verified"}),
        ],
        true,
    );
    assert!(
        summary.starts_with("The work has partial results; some items still need verification.")
    );
    assert!(summary.contains("Research [partially complete; not fully verified]"));
}

#[test]
fn progress_delivery_without_synthesis_includes_all_full_reports_in_one_text() {
    let first = format!("# 第一份\n{}\nFIRST_REPORT_END", "财务依据\n".repeat(2000));
    let second = "# 第二份\n完整独立结论\nSECOND_REPORT_END";
    let text = super::graph_delivery_text(
        LocalTaskGraphStatus::Succeeded,
        &[
            json!({"title": "流程", "status": "succeeded", "summary": "简述", "delivery": first}),
            json!({"title": "权限", "status": "succeeded", "summary": "简述", "delivery": second}),
        ],
        false,
    );
    assert!(text.contains("全部 2 个任务均已完成"));
    assert!(text.contains(&first));
    assert!(text.contains(second));
}

#[tokio::test]
async fn progress_summary_four_tasks_start_on_claim_update_in_place_and_deliver_full_synthesis() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let turn =
        create_callback_conversation(&storage, "progress", "验收财务项目并交付完整综合结论").await;
    let mut tasks = vec![
        summary_task("progress-a", "流程审查"),
        summary_task("progress-b", "权限审查"),
        summary_task("progress-c", "动态验证"),
        summary_task("progress-final", "综合验收"),
    ];
    for task in &mut tasks {
        task.input["graph_notification_policy"] = json!("progress_summary");
    }
    tasks[3].input["final_delivery"] = json!(true);
    storage
        .create_task_graph(
            &command("create-progress-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-progress".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: turn.turn_id.clone(),
                tasks,
                dependencies: ["progress-a", "progress-b", "progress-c"]
                    .into_iter()
                    .map(|id| LocalTaskDependency {
                        task_id: "progress-final".to_string(),
                        prerequisite_task_id: id.to_string(),
                    })
                    .collect(),
            },
            4_000,
        )
        .await
        .unwrap();
    assert_eq!(
        storage
            .get_conversation("user-1", &turn.conversation_id)
            .await
            .unwrap()
            .unwrap()
            .messages
            .len(),
        1
    );
    let queued = storage
        .start_next_task_run("user-1", "progress-run-a", "start-progress-a", 5_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(queued.owner_entity_id, "progress-a");
    assert_eq!(
        storage
            .get_conversation("user-1", &turn.conversation_id)
            .await
            .unwrap()
            .unwrap()
            .messages
            .len(),
        1
    );
    let claim = storage
        .claim_next_run(
            &command("claim-progress-a"),
            "user-1",
            "worker-1",
            "progress-token",
            5_001,
            15_000,
            "claim-progress-event",
        )
        .await
        .unwrap()
        .unwrap();
    let started = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.messages.len(), 2);
    let card_id = started.messages[1].message_id.clone();
    assert_eq!(started.messages[1].content, json!("已开始：流程审查"));
    assert_eq!(
        started.messages[1].metadata["task_runner_async"]["event"],
        "task.run.started"
    );
    assert_eq!(
        started.messages[1].metadata["task_runner_async"]["started_at_unix_ms"],
        5_001
    );
    // Replayed start and graph reconciliation neither duplicate nor invent starts.
    let mut connection = storage.pool.acquire().await.unwrap();
    assert!(
        !super::write_back_task_run_started(&mut connection, &claim.run, 5_002)
            .await
            .unwrap()
    );
    assert!(
        !super::write_back_graph(&mut connection, "graph-progress", 5_002)
            .await
            .unwrap()
    );
    drop(connection);
    storage
        .apply_transition(
            &command("finish-progress-a"),
            &RunTransition {
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::Succeeded,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"content": "内部流程报告，不应刷到进度消息"})),
                event_id: "finish-progress-a-event".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({}),
                occurred_at_unix_ms: 5_003,
            },
        )
        .await
        .unwrap();
    let finished = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finished.messages.len(), 2);
    assert_eq!(finished.messages[1].message_id, card_id);
    assert_eq!(finished.messages[1].content, json!("已完成：流程审查"));
    assert_eq!(
        finished.messages[1].metadata["task_runner_async"]["started_at_unix_ms"],
        5_001
    );
    let mut connection = storage.pool.acquire().await.unwrap();
    assert!(
        !super::write_back_task_run_started(&mut connection, &claim.run, 5_004)
            .await
            .unwrap()
    );
    drop(connection);
    finish_task_run(
        &storage,
        "progress-run-b",
        LocalAgentRunStatus::Succeeded,
        6_000,
    )
    .await;
    let partial = storage
        .get_task_graph("user-1", "graph-progress")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(partial.tasks[3].status, LocalTaskStatus::Pending);
    finish_task_run(
        &storage,
        "progress-run-c",
        LocalAgentRunStatus::Succeeded,
        7_000,
    )
    .await;
    let report = format!(
        "# 完整综合结论\n\n{}\n\n```python\nprint('verified')\n```\n\n最终尾部验收证据",
        "详细证据：虚构数据通过验证。\n".repeat(1500)
    );
    finish_task_run_with_outcome(
        &storage,
        "progress-run-final",
        LocalAgentRunStatus::Succeeded,
        8_000,
        json!({
            "content": format!("{report}\npassword=real-secret\nclaim_token: private-token"),
            "result_summary": "这只是短摘要", "reasoning": "private reasoning",
        }),
    )
    .await;
    let completed = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.messages.len(), 6); // User + 4 progress messages + one full delivery.
    assert_eq!(completed.messages[5].content["delivery_text"], report);
    assert_eq!(
        completed.messages[5].content["notification_policy"],
        "progress_summary"
    );
    assert!(completed.messages[1..5].iter().all(|message| message
        .content
        .as_str()
        .unwrap()
        .chars()
        .count()
        < 80));
    let mut connection = storage.pool.acquire().await.unwrap();
    assert!(
        !super::write_back_graph(&mut connection, "graph-progress", 9_000)
            .await
            .unwrap()
    );
    drop(connection);
    let events = storage.list_events(0, 200, None).await.unwrap();
    let starts = events
        .iter()
        .filter(|event| {
            event.event_type == "task_callback_written_back"
                && event.payload["event"] == "task.run.started"
        })
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 4);
    assert_eq!(starts[3].payload["task_id"], "progress-final");
    assert!(starts[3].created_at_unix_ms > starts[2].created_at_unix_ms);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_graph_written_back")
            .count(),
        1
    );
}

#[test]
fn full_delivery_preserves_markdown_code_and_safe_error_discussion_without_runtime_details() {
    let value = json!({"content": "# 测试\n\n503 timeout 为故障注入场景，不是服务失败。\n\n```python\n    print('ok')\n    password='secret-value'\n```\nreasoning: private\nrun_id: secret-id\nAuthorization: Bearer actual-secret\n结论保留", "reasoning": "hidden"});
    let detail = super::full_delivery_detail(&value, false).unwrap();
    assert!(detail.contains("# 测试\n\n503 timeout"));
    assert!(detail.contains("```python\n    print('ok')\n```"));
    assert!(detail.ends_with("结论保留"));
    assert!(!detail.contains("secret"));
    assert!(!detail.contains("hidden"));
}

#[tokio::test]
async fn progress_summary_failed_synthesis_falls_back_then_retry_delivers_new_generation() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let turn = create_callback_conversation(
        &storage,
        "progress-retry",
        "Complete both areas and deliver the integrated result.",
    )
    .await;
    let mut tasks = vec![
        summary_task("retry-a", "Independent findings"),
        summary_task("retry-b", "Required foundation"),
        summary_task("retry-final", "Synthesis"),
    ];
    for task in &mut tasks {
        task.input["graph_notification_policy"] = json!("progress_summary");
    }
    tasks[2].input["final_delivery"] = json!(true);
    storage
        .create_task_graph(
            &command("create-progress-retry"),
            &CreateTaskGraphCommand {
                graph_id: "graph-progress-retry".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: turn.turn_id.clone(),
                tasks,
                dependencies: ["retry-a", "retry-b"]
                    .into_iter()
                    .map(|id| LocalTaskDependency {
                        task_id: "retry-final".to_string(),
                        prerequisite_task_id: id.to_string(),
                    })
                    .collect(),
            },
            4_000,
        )
        .await
        .unwrap();
    let available = format!(
        "# Available findings\n{}\nFULL_REPORT_END",
        "Verified independently.\n".repeat(1000)
    );
    finish_task_run_with_outcome(
        &storage,
        "retry-run-a",
        LocalAgentRunStatus::Succeeded,
        5_000,
        json!({"content": available}),
    )
    .await;
    finish_task_run_with_outcome(
        &storage,
        "retry-run-b",
        LocalAgentRunStatus::Failed,
        6_000,
        json!({"reason": "Missing required input.\npassword=real-secret"}),
    )
    .await;
    let failed = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    let delivery = failed
        .messages
        .iter()
        .find(|message| message.message_id == "task-graph:graph-progress-retry:1")
        .unwrap();
    let text = delivery.content["delivery_text"].as_str().unwrap();
    assert!(text.contains("partial results: 1 succeeded, 1 failed, and 1 blocked"));
    assert!(text.contains("FULL_REPORT_END"));
    assert!(!text.contains("real-secret"));
    assert!(failed.messages.iter().any(|message| message
        .content
        .as_str()
        .is_some_and(|text| text.contains("Missing required input"))));
    assert!(!failed
        .messages
        .iter()
        .any(
            |message| message.metadata["task_runner_async"]["event"] == "task.run.started"
                && message.metadata["task_runner_async"]["task_id"] == "retry-final"
        ));
    let graph = storage
        .get_task_graph("user-1", "graph-progress-retry")
        .await
        .unwrap()
        .unwrap();
    storage
        .retry_task(
            &command("retry-progress-foundation"),
            "user-1",
            "retry-b",
            graph.tasks[1].version,
            Some("Required input supplied."),
            7_000,
        )
        .await
        .unwrap();
    finish_task_run(
        &storage,
        "retry-run-b-second",
        LocalAgentRunStatus::Succeeded,
        8_000,
    )
    .await;
    finish_task_run_with_outcome(
        &storage,
        "retry-run-final",
        LocalAgentRunStatus::Succeeded,
        9_000,
        json!({"content": "# Complete synthesis\nAll evidence reviewed."}),
    )
    .await;
    let recovered = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    let delivery = recovered
        .messages
        .iter()
        .find(|message| message.message_id == "task-graph:graph-progress-retry:2")
        .unwrap();
    assert_eq!(
        delivery.content["delivery_text"],
        "# Complete synthesis\nAll evidence reviewed."
    );
    let run_cards = recovered
        .messages
        .iter()
        .filter(|message| message.metadata["task_runner_async"]["task_id"] == "retry-b")
        .collect::<Vec<_>>();
    assert_eq!(run_cards.len(), 2);
    assert_eq!(
        run_cards[0].metadata["task_runner_async"]["status"],
        "failed"
    );
    assert_eq!(
        run_cards[1].metadata["task_runner_async"]["status"],
        "succeeded"
    );
    assert_ne!(run_cards[0].message_id, run_cards[1].message_id);
    let mut connection = storage.pool.acquire().await.unwrap();
    assert!(
        !super::write_back_graph(&mut connection, "graph-progress-retry", 10_000)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn progress_summary_cancelled_queued_graph_does_not_invent_start_callbacks() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let turn =
        create_callback_conversation(&storage, "progress-cancel", "Stop this work if cancelled.")
            .await;
    let mut tasks = vec![
        summary_task("cancel-progress-root", "Draft"),
        summary_task("cancel-progress-final", "Synthesis"),
    ];
    for task in &mut tasks {
        task.input["graph_notification_policy"] = json!("progress_summary");
    }
    tasks[1].input["final_delivery"] = json!(true);
    storage
        .create_task_graph(
            &command("create-progress-cancel"),
            &CreateTaskGraphCommand {
                graph_id: "graph-progress-cancel".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: turn.turn_id.clone(),
                tasks,
                dependencies: vec![LocalTaskDependency {
                    task_id: "cancel-progress-final".to_string(),
                    prerequisite_task_id: "cancel-progress-root".to_string(),
                }],
            },
            4_000,
        )
        .await
        .unwrap();
    storage
        .start_next_task_run("user-1", "cancel-queued-run", "queue-cancel", 4_500)
        .await
        .unwrap();
    let graph = storage
        .get_task_graph("user-1", "graph-progress-cancel")
        .await
        .unwrap()
        .unwrap();
    let version = graph
        .tasks
        .iter()
        .find(|task| task.task_id == "cancel-progress-root")
        .unwrap()
        .version;
    storage
        .cancel_task(
            &command("cancel-progress-root"),
            "user-1",
            "cancel-progress-root",
            Some(version),
            "User cancelled.",
            &[],
            "cancel-progress-event",
            5_000,
        )
        .await
        .unwrap();
    let messages = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap()
        .messages;
    assert!(messages
        .iter()
        .all(|message| message.metadata["task_runner_async"]["started_at_unix_ms"].is_null()));
    assert!(messages
        .iter()
        .any(|message| message.metadata["task_runner_async"]["event"] == "task.cancelled"));
    assert!(messages.last().unwrap().content["delivery_text"]
        .as_str()
        .unwrap()
        .contains("work was stopped"));
    let events = storage.list_events(0, 100, None).await.unwrap();
    assert!(events
        .iter()
        .all(|event| event.payload["event"] != "task.run.started"));
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
    finish_task_run_with_outcome(
        storage,
        run_id,
        status,
        now_unix_ms,
        json!({
            "content": format!("Result from {run_id}"),
            "reasoning": "internal reasoning must not be displayed",
        }),
    )
    .await;
}

async fn finish_task_run_with_outcome(
    storage: &SqliteClientStorage,
    run_id: &str,
    status: LocalAgentRunStatus,
    now_unix_ms: i64,
    outcome: Value,
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
                terminal_outcome: Some(outcome),
                event_id: format!("finish-event-{run_id}"),
                event_type: format!("run_{}", status.as_str()),
                event_payload: json!({"status": status}),
                occurred_at_unix_ms: now_unix_ms + 2,
            },
        )
        .await
        .expect("finish Task Run");
}

async fn create_callback_conversation(
    storage: &SqliteClientStorage,
    suffix: &str,
    message: &str,
) -> StartConversationTurnCommand {
    let conversation_id = format!("conversation-{suffix}");
    let turn = StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: conversation_id.clone(),
        expected_conversation_version: 1,
        turn_id: format!("turn-{suffix}"),
        message_id: format!("message-{suffix}"),
        run_id: format!("conversation-run-{suffix}"),
        message: message.to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    };
    storage
        .create_conversation(
            &command(&format!("create-{suffix}-conversation")),
            &CreateConversationCommand {
                conversation_id,
                owner_user_id: "user-1".to_string(),
                title: suffix.to_string(),
                resource: None,
            },
            1_000,
        )
        .await
        .expect("create callback Conversation");
    storage
        .start_conversation_turn(
            &command(&format!("start-{suffix}-turn")),
            &turn,
            &conversation_run(&turn),
            &format!("start-{suffix}-event"),
            2_000,
        )
        .await
        .expect("start callback Turn");
    storage
        .cancel_run(
            &command(&format!("cancel-{suffix}-main")),
            &turn.run_id,
            Some(1),
            "main response completed elsewhere",
            &format!("cancel-{suffix}-main-event"),
            3_000,
        )
        .await
        .expect("close callback source Turn");
    turn
}

fn summary_task(task_id: &str, title: &str) -> LocalTaskSpec {
    LocalTaskSpec {
        task_id: task_id.to_string(),
        title: title.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "prompt": format!("Complete {title}"),
            "objective": format!("Complete {title}")
        }),
        max_iterations: 8,
    }
}

#[tokio::test]
async fn expired_claim_reason_is_visible_and_legacy_backfill_is_owner_scoped_and_idempotent() {
    for policy in ["all_tasks", "progress_summary", "graph_summary"] {
        let storage = SqliteClientStorage::connect_memory().await.unwrap();
        let turn = create_callback_conversation(&storage, policy, "复核虚构财务文字").await;
        let mut task = summary_task("interrupted-task", "财务复核");
        task.input["graph_notification_policy"] = json!(policy);
        storage
            .create_task_graph(
                &command("create-interrupted-graph"),
                &CreateTaskGraphCommand {
                    graph_id: "interrupted-graph".into(),
                    owner_user_id: "user-1".into(),
                    source_entity_type: "conversation_turn".into(),
                    source_entity_id: turn.turn_id,
                    tasks: vec![task],
                    dependencies: vec![],
                },
                4_000,
            )
            .await
            .unwrap();
        storage
            .start_next_task_run("user-1", "interrupted-run", "start-interrupted-run", 5_000)
            .await
            .unwrap()
            .unwrap();
        storage
            .claim_next_run(
                &command("claim-interrupted-run"),
                "user-1",
                "worker",
                "claim",
                5_001,
                6_000,
                "claim-interrupted-event",
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            storage
                .recover_expired_claims("user-1", 6_001)
                .await
                .unwrap(),
            1
        );
        let history = storage
            .get_conversation("user-1", &turn.conversation_id)
            .await
            .unwrap()
            .unwrap();
        assert!(
            history
                .messages
                .iter()
                .any(|message| message.content.to_string().contains("任务执行意外中断")),
            "{policy}"
        );
        let graph = storage
            .get_task_graph("user-1", "interrupted-graph")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Blocked);
        let version = graph.tasks[0].version;
        // Simulate the old format: no outcome, but a durable expired-claim event exists.
        sqlx::query("UPDATE local_agent_runs SET terminal_outcome_json = NULL WHERE run_id = 'interrupted-run'").execute(&storage.pool).await.unwrap();
        assert_eq!(
            storage
                .recover_expired_claims("user-2", 7_000)
                .await
                .unwrap(),
            0
        );
        assert!(storage
            .get_run("interrupted-run")
            .await
            .unwrap()
            .unwrap()
            .terminal_outcome
            .is_none());
        assert_eq!(
            storage
                .recover_expired_claims("user-1", 7_000)
                .await
                .unwrap(),
            1
        );
        let repaired = storage.get_run("interrupted-run").await.unwrap().unwrap();
        assert_eq!(repaired.status, LocalAgentRunStatus::NeedsReview);
        assert_eq!(repaired.updated_at_unix_ms, 6_001);
        assert_eq!(
            repaired.terminal_outcome.as_ref().unwrap()["reason_code"],
            "execution_interrupted"
        );
        assert_eq!(
            storage
                .recover_expired_claims("user-1", 8_000)
                .await
                .unwrap(),
            0
        );
        let after = storage
            .get_task_graph("user-1", "interrupted-graph")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.tasks[0].version, version);
        assert!(after.tasks[0].active_run_id.is_none());
        let final_history = storage
            .get_conversation("user-1", &turn.conversation_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(final_history.messages.len(), history.messages.len());
        // Without the recovery event, an unknown needs_review is not relabelled.
        sqlx::query("UPDATE local_agent_runs SET terminal_outcome_json = NULL WHERE run_id = 'interrupted-run'").execute(&storage.pool).await.unwrap();
        sqlx::query(
            "DELETE FROM local_agent_events WHERE event_type = 'claim_expired_needs_review'",
        )
        .execute(&storage.pool)
        .await
        .unwrap();
        assert_eq!(
            storage
                .recover_expired_claims("user-1", 9_000)
                .await
                .unwrap(),
            0
        );
        assert!(storage
            .get_run("interrupted-run")
            .await
            .unwrap()
            .unwrap()
            .terminal_outcome
            .is_none());
        let cancelled = storage
            .cancel_task(
                &command("cancel-interrupted"),
                "user-1",
                "interrupted-task",
                Some(version),
                "不再继续这项任务",
                &[],
                "cancel-interrupted-event",
                10_000,
            )
            .await
            .unwrap();
        assert_eq!(cancelled.tasks[0].status, LocalTaskStatus::Cancelled);
        let cancelled_history = storage
            .get_conversation("user-1", &turn.conversation_id)
            .await
            .unwrap()
            .unwrap();
        if policy != "graph_summary" {
            let callback = cancelled_history
                .messages
                .iter()
                .find(|message| {
                    message.metadata.pointer("/task_runner_async/event")
                        == Some(&json!("task.cancelled"))
                })
                .unwrap();
            assert_eq!(
                callback
                    .metadata
                    .pointer("/task_runner_async/finished_at_unix_ms"),
                Some(&json!(10_000))
            );
            assert!(!callback.content.to_string().contains("需要检查执行结果"));
        }
        assert_eq!(
            storage
                .get_run("interrupted-run")
                .await
                .unwrap()
                .unwrap()
                .status,
            LocalAgentRunStatus::NeedsReview
        );
    }
}

#[tokio::test]
async fn historical_diagnostic_repair_preserves_successful_retry_and_existing_callback_ids() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let turn = create_callback_conversation(&storage, "legacy-callback", "复核虚构财务文字").await;
    storage
        .create_task_graph(
            &command("create-legacy-callback"),
            &CreateTaskGraphCommand {
                graph_id: "legacy-callback-graph".into(),
                owner_user_id: "user-1".into(),
                source_entity_type: "conversation_turn".into(),
                source_entity_id: turn.turn_id,
                tasks: vec![summary_task("legacy-callback-task", "财务复核")],
                dependencies: vec![],
            },
            4_000,
        )
        .await
        .unwrap();
    storage
        .start_next_task_run(
            "user-1",
            "legacy-interrupted",
            "start-legacy-interrupted",
            5_000,
        )
        .await
        .unwrap()
        .unwrap();
    storage
        .claim_next_run(
            &command("claim-legacy-interrupted"),
            "user-1",
            "worker",
            "claim",
            5_001,
            6_000,
            "claim-legacy-event",
        )
        .await
        .unwrap()
        .unwrap();
    storage
        .recover_expired_claims("user-1", 6_001)
        .await
        .unwrap();
    let graph = storage
        .get_task_graph("user-1", "legacy-callback-graph")
        .await
        .unwrap()
        .unwrap();
    storage
        .retry_task(
            &command("retry-legacy"),
            "user-1",
            "legacy-callback-task",
            graph.tasks[0].version,
            None,
            7_000,
        )
        .await
        .unwrap();
    finish_task_run(
        &storage,
        "successful-retry",
        LocalAgentRunStatus::Succeeded,
        8_000,
    )
    .await;
    let run = storage.get_run("successful-retry").await.unwrap().unwrap();
    let mut connection = storage.pool.acquire().await.unwrap();
    SqliteClientStorage::insert_event(
        &mut connection,
        &format!(
            "task-callback:legacy-callback-task:successful-retry:task.completed:{}",
            run.version
        ),
        "successful-retry",
        "task_callback_written_back",
        &json!({"event": "task.completed"}),
        8_002,
    )
    .await
    .unwrap();
    let callback_count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_agent_events WHERE event_type = 'task_callback_written_back'",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    // Simulate an older message display and its version-only event ID, with no Run change.
    sqlx::query("UPDATE local_conversation_messages SET content_json = ? WHERE json_extract(metadata_json, '$.task_runner_async.run_id') = 'successful-retry'")
        .bind(json!("旧的完成提示").to_string()).execute(&mut *connection).await.unwrap();
    assert!(
        super::write_back_terminal_task_run(&mut connection, &run, 8_003)
            .await
            .unwrap()
    );
    assert!(
        !super::write_back_terminal_task_run(&mut connection, &run, 8_004)
            .await
            .unwrap()
    );
    // Restoring the same content a second time must emit a new mutation event,
    // while replaying an unchanged message must not emit anything.
    sqlx::query("UPDATE local_conversation_messages SET content_json = ? WHERE json_extract(metadata_json, '$.task_runner_async.run_id') = 'successful-retry'")
        .bind(json!("旧的完成提示").to_string()).execute(&mut *connection).await.unwrap();
    assert!(
        super::write_back_terminal_task_run(&mut connection, &run, 8_005)
            .await
            .unwrap()
    );
    assert!(
        !super::write_back_terminal_task_run(&mut connection, &run, 8_006)
            .await
            .unwrap()
    );
    let callback_count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_agent_events WHERE event_type = 'task_callback_written_back'",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(callback_count_after, callback_count_before + 2);
    drop(connection);
    let before = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    let task_before = storage
        .get_task_graph("user-1", "legacy-callback-graph")
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE local_agent_runs SET terminal_outcome_json = NULL WHERE run_id = 'legacy-interrupted'").execute(&storage.pool).await.unwrap();
    assert_eq!(
        storage
            .recover_expired_claims("user-1", 9_000)
            .await
            .unwrap(),
        1
    );
    let after = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.messages, before.messages, "Repairing an old interrupted Run cannot rewrite current or historical callbacks after a successful retry");
    let task_after = storage
        .get_task_graph("user-1", "legacy-callback-graph")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task_after.tasks, task_before.tasks);
    assert_eq!(task_after.tasks[0].status, LocalTaskStatus::Succeeded);
    assert_eq!(
        storage
            .recover_expired_claims("user-1", 10_000)
            .await
            .unwrap(),
        0
    );
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

#[tokio::test]
async fn multi_task_graph_defaults_to_one_sanitized_terminal_summary() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_conversation(
            &command("create-summary-conversation"),
            &CreateConversationCommand {
                conversation_id: "conversation-summary".to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Summary".to_string(),
                resource: None,
            },
            1_000,
        )
        .await
        .expect("create Conversation");
    let turn = StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-summary".to_string(),
        expected_conversation_version: 1,
        turn_id: "turn-summary".to_string(),
        message_id: "message-summary".to_string(),
        run_id: "conversation-run-summary".to_string(),
        message: "完成两个工作流并汇总结果".to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    };
    storage
        .start_conversation_turn(
            &command("start-summary-turn"),
            &turn,
            &conversation_run(&turn),
            "start-summary-event",
            2_000,
        )
        .await
        .expect("start Turn");
    storage
        .cancel_run(
            &command("cancel-summary-main"),
            &turn.run_id,
            Some(1),
            "main response completed elsewhere",
            "cancel-summary-main-event",
            3_000,
        )
        .await
        .expect("close source Turn");
    let task = |task_id: &str, title: &str| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: title.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "prompt": format!("Complete {title}"),
            "objective": format!("Complete {title}")
        }),
        max_iterations: 8,
    };
    storage
        .create_task_graph(
            &command("create-summary-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-summary".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: "turn-summary".to_string(),
                tasks: vec![
                    task("summary-task-a", "macOS implementation"),
                    task("summary-task-b", "Verification"),
                ],
                dependencies: Vec::new(),
            },
            4_000,
        )
        .await
        .expect("create summary graph");

    finish_task_run(
        &storage,
        "summary-run-a",
        LocalAgentRunStatus::Succeeded,
        5_000,
    )
    .await;
    let interim = storage
        .get_conversation("user-1", "conversation-summary")
        .await
        .expect("load interim Conversation")
        .expect("interim Conversation");
    assert_eq!(interim.messages.len(), 1);

    finish_task_run(
        &storage,
        "summary-run-b",
        LocalAgentRunStatus::Succeeded,
        6_000,
    )
    .await;
    let completed = storage
        .get_conversation("user-1", "conversation-summary")
        .await
        .expect("load completed Conversation")
        .expect("completed Conversation");
    assert_eq!(completed.messages.len(), 2);
    let summary = &completed.messages[1];
    assert_eq!(summary.message_id, "task-graph:graph-summary:1");
    assert_eq!(summary.metadata["kind"], "task_graph_terminal");
    assert_eq!(summary.metadata["notification_policy"], "graph_summary");
    assert_eq!(summary.content["type"], "task_graph_terminal");
    assert_eq!(summary.content["notification_policy"], "graph_summary");
    assert_eq!(summary.content["tasks"].as_array().map(Vec::len), Some(2));
    let summary_text = summary.content["summary"].as_str().expect("summary text");
    assert!(summary_text.contains("全部 2 个任务均已完成"));
    assert!(summary_text.contains("Result from summary-run-a"));
    assert!(summary_text.contains("Result from summary-run-b"));
    assert!(!summary_text.contains("internal reasoning"));

    let writebacks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_task_graph_writebacks WHERE graph_id = 'graph-summary'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("count graph summaries");
    assert_eq!(writebacks, 1);
    let events = storage
        .list_events(0, 100, None)
        .await
        .expect("list summary events");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_callback_written_back")
            .count(),
        0
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_graph_written_back")
            .count(),
        1
    );

    let mut milestone = task("milestone-task", "Milestone implementation");
    milestone.input["graph_notification_policy"] = json!("milestones");
    milestone.input["notification_milestone"] = json!(true);
    let mut quiet = task("quiet-task", "Internal verification");
    quiet.input["graph_notification_policy"] = json!("milestones");
    quiet.input["notification_milestone"] = json!(false);
    storage
        .create_task_graph(
            &command("create-milestone-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-milestones".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: "turn-summary".to_string(),
                tasks: vec![milestone, quiet],
                dependencies: Vec::new(),
            },
            7_000,
        )
        .await
        .expect("create milestone graph");
    finish_task_run(
        &storage,
        "milestone-run",
        LocalAgentRunStatus::Succeeded,
        8_000,
    )
    .await;
    finish_task_run(&storage, "quiet-run", LocalAgentRunStatus::Succeeded, 9_000).await;
    let milestone_conversation = storage
        .get_conversation("user-1", "conversation-summary")
        .await
        .expect("load milestone Conversation")
        .expect("milestone Conversation");
    assert_eq!(milestone_conversation.messages.len(), 4);
    assert_eq!(
        milestone_conversation.messages[2].metadata["task_runner_async"]["event"],
        "task.completed"
    );
    assert_eq!(
        milestone_conversation.messages[3].metadata["notification_policy"],
        "milestones"
    );
    assert!(milestone_conversation
        .messages
        .iter()
        .all(|message| !message.message_id.contains("quiet-task")));
    let events = storage
        .list_events(0, 200, None)
        .await
        .expect("list milestone events");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_callback_written_back")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_graph_written_back")
            .count(),
        2
    );
}

#[tokio::test]
async fn failed_branch_reports_partial_success_and_blocked_dependents_then_retry_generation() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let turn = create_callback_conversation(
        &storage,
        "partial-summary",
        "Complete the independent work and report the combined result.",
    )
    .await;
    storage
        .create_task_graph(
            &command("create-partial-summary-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-partial-summary".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: turn.turn_id.clone(),
                tasks: vec![
                    summary_task("partial-a-success", "Independent deliverable"),
                    summary_task("partial-b-failure", "Required foundation"),
                    summary_task("partial-c-blocked", "Dependent integration"),
                ],
                dependencies: vec![LocalTaskDependency {
                    task_id: "partial-c-blocked".to_string(),
                    prerequisite_task_id: "partial-b-failure".to_string(),
                }],
            },
            4_000,
        )
        .await
        .expect("create partial summary graph");

    finish_task_run(
        &storage,
        "partial-run-success",
        LocalAgentRunStatus::Succeeded,
        5_000,
    )
    .await;
    finish_task_run(
        &storage,
        "partial-run-failure",
        LocalAgentRunStatus::Failed,
        6_000,
    )
    .await;

    let failed_graph = storage
        .get_task_graph("user-1", "graph-partial-summary")
        .await
        .expect("load partial graph")
        .expect("partial graph");
    assert_eq!(failed_graph.status, LocalTaskGraphStatus::Failed);
    assert_eq!(failed_graph.tasks[0].status, LocalTaskStatus::Succeeded);
    assert_eq!(failed_graph.tasks[1].status, LocalTaskStatus::Failed);
    assert_eq!(failed_graph.tasks[2].status, LocalTaskStatus::Blocked);

    let conversation = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .expect("load partial Conversation")
        .expect("partial Conversation");
    assert_eq!(conversation.messages.len(), 2);
    let summary = &conversation.messages[1];
    assert_eq!(summary.message_id, "task-graph:graph-partial-summary:1");
    let text = summary.content["summary"].as_str().expect("summary text");
    assert!(text
        .contains("The work finished with partial results: 1 succeeded, 1 failed, and 1 blocked."));
    assert!(text.contains("- Independent deliverable [succeeded]:"));
    assert!(text.contains("- Required foundation [execution failed]:"));
    assert!(text.contains("- Dependent integration [blocked]"));
    assert!(!text.contains('：'));

    let failed_version = failed_graph.tasks[1].version;
    storage
        .retry_task(
            &command("retry-partial-failure"),
            "user-1",
            "partial-b-failure",
            failed_version,
            Some("Retry the required foundation."),
            7_000,
        )
        .await
        .expect("retry failed branch");
    finish_task_run(
        &storage,
        "partial-run-retry",
        LocalAgentRunStatus::Succeeded,
        8_000,
    )
    .await;
    finish_task_run(
        &storage,
        "partial-run-dependent",
        LocalAgentRunStatus::Succeeded,
        9_000,
    )
    .await;

    let recovered = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .expect("load recovered Conversation")
        .expect("recovered Conversation");
    assert_eq!(recovered.messages.len(), 3);
    assert_eq!(
        recovered.messages[2].message_id,
        "task-graph:graph-partial-summary:2"
    );
    assert!(recovered.messages[2].content["summary"]
        .as_str()
        .expect("recovered summary")
        .contains("All 3 tasks completed successfully."));
    let writebacks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_task_graph_writebacks \
         WHERE graph_id = 'graph-partial-summary'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("count partial summary generations");
    assert_eq!(writebacks, 2);
}

#[tokio::test]
async fn graph_summary_surfaces_active_blocker_then_writes_one_terminal_summary() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let turn = create_callback_conversation(
        &storage,
        "active-blocker",
        "Complete both workstreams and tell me if input is required.",
    )
    .await;
    storage
        .create_task_graph(
            &command("create-active-blocker-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-active-blocker".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: turn.turn_id.clone(),
                tasks: vec![
                    summary_task("blocker-a-needs-review", "Credential-gated work"),
                    summary_task("blocker-b-independent", "Independent work"),
                ],
                dependencies: Vec::new(),
            },
            4_000,
        )
        .await
        .expect("create active blocker graph");

    finish_task_run(
        &storage,
        "blocker-run-needs-review",
        LocalAgentRunStatus::NeedsReview,
        5_000,
    )
    .await;
    let active = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .expect("load blocker Conversation")
        .expect("blocker Conversation");
    assert_eq!(active.messages.len(), 2);
    assert_eq!(
        active.messages[1].metadata["task_runner_async"]["event"],
        "task.blocked"
    );
    assert_eq!(
        active.messages[1].metadata["task_runner_async"]["task_id"],
        "blocker-a-needs-review"
    );

    finish_task_run(
        &storage,
        "blocker-run-independent",
        LocalAgentRunStatus::Succeeded,
        6_000,
    )
    .await;
    let terminal = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .expect("load blocker terminal Conversation")
        .expect("blocker terminal Conversation");
    assert_eq!(terminal.messages.len(), 3);
    assert_eq!(
        terminal.messages[2].message_id,
        "task-graph:graph-active-blocker:1"
    );
    assert!(terminal.messages[2].content["summary"]
        .as_str()
        .expect("blocker summary")
        .contains("1 succeeded, 0 failed, and 1 blocked"));
}

#[tokio::test]
async fn cancelling_multi_task_graph_writes_only_one_cancelled_summary() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let turn = create_callback_conversation(
        &storage,
        "cancelled-summary",
        "Prepare the deliverable, but stop if I cancel it.",
    )
    .await;
    storage
        .create_task_graph(
            &command("create-cancelled-summary-graph"),
            &CreateTaskGraphCommand {
                graph_id: "graph-cancelled-summary".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation_turn".to_string(),
                source_entity_id: turn.turn_id.clone(),
                tasks: vec![
                    summary_task("cancel-a-root", "Draft deliverable"),
                    summary_task("cancel-b-dependent", "Review deliverable"),
                ],
                dependencies: vec![LocalTaskDependency {
                    task_id: "cancel-b-dependent".to_string(),
                    prerequisite_task_id: "cancel-a-root".to_string(),
                }],
            },
            4_000,
        )
        .await
        .expect("create cancellable summary graph");
    let cancelled = storage
        .cancel_task(
            &command("cancel-summary-root"),
            "user-1",
            "cancel-a-root",
            Some(1),
            "The user no longer needs this work.",
            &[],
            "cancel-summary-root-event",
            5_000,
        )
        .await
        .expect("cancel summary root");
    assert_eq!(cancelled.status, LocalTaskGraphStatus::Cancelled);
    assert!(cancelled
        .tasks
        .iter()
        .all(|task| task.status == LocalTaskStatus::Cancelled));

    let conversation = storage
        .get_conversation("user-1", &turn.conversation_id)
        .await
        .expect("load cancelled Conversation")
        .expect("cancelled Conversation");
    assert_eq!(conversation.messages.len(), 2);
    assert_eq!(
        conversation.messages[1].message_id,
        "task-graph:graph-cancelled-summary:1"
    );
    let text = conversation.messages[1].content["summary"]
        .as_str()
        .expect("cancelled summary");
    assert!(text.contains("The work was stopped: 0 succeeded and 2 cancelled."));
    assert!(conversation
        .messages
        .iter()
        .all(|message| { message.metadata["kind"] != "task_execution_callback" }));

    let events = storage
        .list_events(0, 100, None)
        .await
        .expect("list cancellation events");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_graph_written_back")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "task_callback_written_back")
            .count(),
        0
    );
}
