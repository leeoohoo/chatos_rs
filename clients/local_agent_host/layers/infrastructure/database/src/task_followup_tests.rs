// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
#[path = "task_followup_dispatch_tests.rs"]
mod dispatch;
#[path = "task_followup_memory_tests.rs"]
mod memory;
use crate::{LocalAgentTaskStore, LocalAgentToolStore};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunClaim, LocalAgentToolBatch, LocalAgentToolCall,
    LocalAgentToolOutcome, LocalTaskDependency, LocalTaskSpec, LocalTaskStatus,
    ResolveTaskFollowupCommand, TaskFollowupAction, TaskFollowupDecision,
};

fn graph(id: &str, source: &str, ids: &[&str]) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: id.into(),
        owner_user_id: "user-1".into(),
        source_entity_type: "conversation_turn".into(),
        source_entity_id: source.into(),
        tasks: ids
            .iter()
            .map(|id| LocalTaskSpec {
                task_id: (*id).into(),
                title: format!("Work {id}"),
                profile_key: "task_execution".into(),
                model_config_ref: "model-1".into(),
                model_config_revision: "revision-1".into(),
                capability_policy_revision: "policy-1".into(),
            input: json!({"objective": format!("Publish {id}"), "prompt": "Publish",
                "source_conversation_id": if source == "turn-other" { "conversation-2" } else { "conversation-1" }}),
                max_iterations: 8,
            })
            .collect(),
        dependencies: vec![],
    }
}

async fn fixture(storage: &SqliteClientStorage) -> LocalAgentRunClaim {
    create(storage, "conversation-1").await;
    start(storage, &turn("conversation-1", 1, "original"), 2_000)
        .await
        .unwrap();
    let main = claim(storage, "main-original", 3_000).await.unwrap();
    assert_eq!(main.run.run_id, "run-original");
    let mut work = graph(
        "graph-original",
        "turn-original",
        &["root", "child", "spare"],
    );
    work.dependencies.push(LocalTaskDependency {
        task_id: "child".into(),
        prerequisite_task_id: "root".into(),
    });
    storage
        .create_task_graph(&idempotency("create-work"), &work, 4_000)
        .await
        .unwrap();
    storage
        .start_next_task_run("user-1", "run-task", "task-start", 5_000)
        .await
        .unwrap()
        .unwrap();
    let task = claim(storage, "task-original", 6_000).await.unwrap();
    assert_eq!(task.run.owner_entity_id, "root");
    task
}

async fn claim(storage: &SqliteClientStorage, id: &str, now: i64) -> Option<LocalAgentRunClaim> {
    storage
        .claim_next_run(
            &idempotency(&format!("claim-{id}")),
            "user-1",
            "worker",
            &format!("token-{id}"),
            now,
            now + 100_000,
            &format!("event-{id}"),
        )
        .await
        .unwrap()
}

fn transition(claim: &LocalAgentRunClaim, status: LocalAgentRunStatus, id: &str) -> RunTransition {
    RunTransition {
        run_id: claim.run.run_id.clone(),
        claim_token: claim.claim_token.clone(),
        expected_version: claim.run.version,
        expected_status: LocalAgentRunStatus::ModelRunning,
        next_status: status,
        next_model_attempt: 1,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        tool_batch: None,
        checkpoint: Some(json!({"history": "preserved"})),
        clear_continuation_input: true,
        terminal_outcome: Some(json!({"answer": "Done"})),
        event_id: format!("event-{id}"),
        event_type: "run_succeeded".into(),
        event_payload: json!({"answer": "Done"}),
        occurred_at_unix_ms: 20_000,
    }
}

async fn guide(storage: &SqliteClientStorage, id: &str) {
    let detail = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .unwrap()
        .unwrap();
    let main = storage.get_run("run-original").await.unwrap().unwrap();
    storage
        .guide_conversation_turn(
            &idempotency(id),
            &GuideConversationTurnCommand {
                owner_user_id: "user-1".into(),
                conversation_id: "conversation-1".into(),
                expected_conversation_version: detail.conversation.version,
                turn_id: "turn-original".into(),
                expected_run_version: Some(main.version),
                message_id: id.into(),
                message: "Stop publishing; inspect only".into(),
                message_metadata: json!({}),
                attachments: vec![],
            },
            &format!("event-{id}"),
            10_000,
        )
        .await
        .unwrap();
}

fn review(
    run: &str,
    message: &str,
    actions: &[(&str, TaskFollowupAction)],
) -> ResolveTaskFollowupCommand {
    ResolveTaskFollowupCommand {
        owner_user_id: "user-1".into(),
        run_id: run.into(),
        review_message_id: message.into(),
        decisions: actions
            .iter()
            .map(|(id, action)| TaskFollowupDecision {
                task_id: (*id).into(),
                action: *action,
                reason: "Latest user requirement reviewed".into(),
            })
            .collect(),
    }
}

fn unchanged(message: &str) -> ResolveTaskFollowupCommand {
    review(
        "run-original",
        message,
        &[
            ("root", TaskFollowupAction::Continue),
            ("child", TaskFollowupAction::Continue),
            ("spare", TaskFollowupAction::Continue),
        ],
    )
}

async fn blocked(storage: &SqliteClientStorage) {
    assert!(storage
        .start_next_task_run("user-1", "must-not-start", "no-start", 11_000)
        .await
        .unwrap()
        .is_none());
    // Main Chat can still review; the task's invalidated computation cannot restart.
    let main = claim(storage, "review-main", 11_000).await.unwrap();
    assert_eq!(main.run.profile_key, "main_chat");
    assert_eq!(main.run.input["task_followup_review"]["pending"], true);
    assert!(claim(storage, "blocked-task", 12_000).await.is_none());
}

#[tokio::test]
async fn same_turn_guidance_holds_model_ready_and_pending_descendants_atomically() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let old_claim = fixture(&storage).await;
    guide(&storage, "followup-1").await;
    blocked(&storage).await;
    let stale = storage
        .apply_transition(
            &idempotency("stale"),
            &transition(&old_claim, LocalAgentRunStatus::Succeeded, "stale"),
        )
        .await;
    assert!(matches!(stale, Err(ClientStorageError::Conflict(_))));
    let current = storage.get_run("run-task").await.unwrap().unwrap();
    assert_eq!(current.status, LocalAgentRunStatus::ContinuationReady);
    assert!(current.claim_token.is_none());
    let work = storage
        .get_task_graph("user-1", "graph-original")
        .await
        .unwrap()
        .unwrap();
    assert!(work
        .tasks
        .iter()
        .all(|task| task.input["followup_review_state"] == "pending"));
    assert!(work.tasks.iter().all(|task| task.version > 1));
    let invalid_new = graph("bypass", "turn-original", &["new"]);
    assert!(storage
        .create_task_graph(&idempotency("bypass"), &invalid_new, 12_000)
        .await
        .is_err());
    let output = storage
        .resolve_task_followup(&idempotency("release"), &unchanged("followup-1"), 13_000)
        .await
        .unwrap();
    assert_eq!(output["reviewed"], true);
    let task = claim(&storage, "resumed-task", 14_000).await.unwrap();
    assert_eq!(task.run.run_id, old_claim.run.run_id);
    assert_eq!(task.run.iteration, old_claim.run.iteration);
    let ready = storage
        .start_next_task_run("user-1", "run-spare", "spare-start", 15_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready.owner_entity_id, "spare");
}

#[tokio::test]
async fn new_turn_review_is_exact_conversation_scoped_and_cancels_before_replacement() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    fixture(&storage).await;
    create(&storage, "conversation-2").await;
    start(&storage, &turn("conversation-2", 1, "other"), 7_000)
        .await
        .unwrap();
    storage
        .create_task_graph(
            &idempotency("other-work"),
            &graph("graph-other", "turn-other", &["other"]),
            8_000,
        )
        .await
        .unwrap();
    let main = storage.get_run("run-original").await.unwrap().unwrap();
    storage
        .cancel_run(
            &idempotency("finish-original"),
            "run-original",
            Some(main.version),
            "finish original fixture",
            "finish-original",
            9_000,
        )
        .await
        .unwrap();
    let detail = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .unwrap()
        .unwrap();
    start(
        &storage,
        &turn("conversation-1", detail.conversation.version, "new"),
        10_000,
    )
    .await
    .unwrap();
    let context = storage.get_run("run-new").await.unwrap().unwrap().input;
    assert_eq!(
        context["task_followup_review"]["tasks"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let other = storage
        .get_task_graph("user-1", "graph-other")
        .await
        .unwrap()
        .unwrap();
    assert!(other.tasks[0].input.get("followup_review_state").is_none());
    let decisions = review(
        "run-new",
        "message-new",
        &[
            ("root", TaskFollowupAction::Cancel),
            ("child", TaskFollowupAction::Continue),
            ("spare", TaskFollowupAction::Continue),
        ],
    );
    let output = storage
        .resolve_task_followup(&idempotency("stop"), &decisions, 11_000)
        .await
        .unwrap();
    assert!(output["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|task| task["task_id"] != "spare")
        .all(|task| task["state"] == "cancelled"));
    let old = storage.get_run("run-task").await.unwrap().unwrap();
    assert_eq!(old.status, LocalAgentRunStatus::Cancelled);
    let mut replacement = graph("replacement", "turn-new", &["corrected"]);
    assert!(storage
        .create_task_graph(&idempotency("ordinary-bypass"), &replacement, 12_000)
        .await
        .is_err());
    replacement.tasks[0].input["supersedes_task_ids"] = json!(["root", "child"]);
    storage
        .create_task_graph(&idempotency("replacement"), &replacement, 12_000)
        .await
        .unwrap();
    let replay = storage
        .resolve_task_followup(&idempotency("stop"), &decisions, 13_000)
        .await
        .unwrap();
    assert_eq!(replay, output);
}

#[tokio::test]
async fn incomplete_foreign_duplicate_and_stale_decisions_do_not_release_any_task() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    fixture(&storage).await;
    guide(&storage, "review-old").await;
    for (name, actions) in [
        ("missing", vec![("root", TaskFollowupAction::Cancel)]),
        (
            "foreign",
            vec![
                ("root", TaskFollowupAction::Continue),
                ("child", TaskFollowupAction::Continue),
                ("foreign", TaskFollowupAction::Continue),
            ],
        ),
        (
            "duplicate",
            vec![
                ("root", TaskFollowupAction::Continue),
                ("root", TaskFollowupAction::Cancel),
                ("spare", TaskFollowupAction::Continue),
            ],
        ),
    ] {
        assert!(storage
            .resolve_task_followup(
                &idempotency(name),
                &review("run-original", "review-old", &actions),
                11_000
            )
            .await
            .is_err());
    }
    guide(&storage, "review-new").await;
    assert!(storage
        .resolve_task_followup(
            &idempotency("stale-review"),
            &unchanged("review-old"),
            12_000
        )
        .await
        .is_err());
    let tasks = storage
        .get_task_graph("user-1", "graph-original")
        .await
        .unwrap()
        .unwrap()
        .tasks;
    assert!(tasks
        .iter()
        .all(|task| task.input["followup_review_state"] == "pending"));
    assert!(tasks
        .iter()
        .all(|task| task.status != LocalTaskStatus::Cancelled));
    storage
        .resolve_task_followup(
            &idempotency("current-review"),
            &unchanged("review-new"),
            13_000,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn omitted_review_is_visible_failure_and_clarification_holds_until_next_message() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    fixture(&storage).await;
    guide(&storage, "followup").await;
    let main = claim(&storage, "unreviewed", 11_000).await.unwrap();
    let final_run = storage
        .apply_transition(
            &idempotency("false-success"),
            &transition(&main, LocalAgentRunStatus::Succeeded, "false-success"),
        )
        .await
        .unwrap();
    assert_eq!(final_run.status, LocalAgentRunStatus::Failed);
    let detail = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .unwrap()
        .unwrap();
    assert!(detail
        .messages
        .iter()
        .any(|message| message.content.to_string().contains("尚未完成关联任务审查")));
    start(
        &storage,
        &turn("conversation-1", detail.conversation.version, "clarify"),
        21_000,
    )
    .await
    .unwrap();
    let decisions = review(
        "run-clarify",
        "message-clarify",
        &[
            ("root", TaskFollowupAction::Clarify),
            ("child", TaskFollowupAction::Clarify),
            ("spare", TaskFollowupAction::Continue),
        ],
    );
    storage
        .resolve_task_followup(&idempotency("clarify"), &decisions, 22_000)
        .await
        .unwrap();
    let main = claim(&storage, "ask-question", 23_000).await.unwrap();
    assert_eq!(main.run.input["task_followup_review"]["pending"], false);
    assert_eq!(
        main.run.input["task_followup_review"]["tasks"][0]["review_state"],
        "held"
    );
    storage
        .apply_transition(
            &idempotency("question"),
            &transition(&main, LocalAgentRunStatus::Succeeded, "question"),
        )
        .await
        .unwrap();
    assert!(claim(&storage, "held-root", 24_000).await.is_none());
    let detail = storage
        .get_conversation("user-1", "conversation-1")
        .await
        .unwrap()
        .unwrap();
    start(
        &storage,
        &turn("conversation-1", detail.conversation.version, "confirmed"),
        25_000,
    )
    .await
    .unwrap();
    let confirm = review(
        "run-confirmed",
        "message-confirmed",
        &[
            ("root", TaskFollowupAction::Continue),
            ("child", TaskFollowupAction::Continue),
            ("spare", TaskFollowupAction::Continue),
        ],
    );
    storage
        .resolve_task_followup(&idempotency("confirmed"), &confirm, 26_000)
        .await
        .unwrap();
    // Finish Main Chat first, then the original task is claimable again.
    let main = claim(&storage, "confirm-main", 27_000).await.unwrap();
    let task = claim(&storage, "confirmed-root", 28_000).await.unwrap();
    let ids = [main.run.run_id.as_str(), task.run.run_id.as_str()];
    assert!(ids.contains(&"run-confirmed") && ids.contains(&"run-task"));
}
