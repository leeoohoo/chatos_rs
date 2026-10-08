// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{
    IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, RunTransition, SqliteClientStorage,
};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalTaskDependency, LocalTaskGraphStatus, LocalTaskSpec,
    LocalTaskStatus,
};
use serde_json::json;

fn command(id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: id.to_string(),
        persist_receipt: true,
    }
}

fn graph() -> CreateTaskGraphCommand {
    let task = |task_id: &str| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "task": task_id,
            "objective": format!("Complete {task_id}"),
            "prompt": format!("Task Objective:\nComplete {task_id}")
        }),
        max_iterations: 4,
    };
    CreateTaskGraphCommand {
        graph_id: "graph-lifecycle".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![
            task("task-a"),
            task("task-b"),
            task("task-c"),
            task("task-d"),
        ],
        dependencies: vec![
            LocalTaskDependency {
                task_id: "task-b".to_string(),
                prerequisite_task_id: "task-a".to_string(),
            },
            LocalTaskDependency {
                task_id: "task-c".to_string(),
                prerequisite_task_id: "task-b".to_string(),
            },
            LocalTaskDependency {
                task_id: "task-d".to_string(),
                prerequisite_task_id: "task-c".to_string(),
            },
        ],
    }
}

async fn finish_next(
    storage: &SqliteClientStorage,
    run_id: &str,
    status: LocalAgentRunStatus,
    now: i64,
) {
    finish_next_with_outcome(storage, run_id, status, json!({"status": status}), now).await;
}

async fn finish_next_with_outcome(
    storage: &SqliteClientStorage,
    run_id: &str,
    status: LocalAgentRunStatus,
    terminal_outcome: Value,
    now: i64,
) {
    storage
        .start_next_task_run("user-1", run_id, &format!("event-start-{run_id}"), now)
        .await
        .expect("start task")
        .expect("ready task");
    let claim = storage
        .claim_next_run(
            &command(&format!("claim-{run_id}")),
            "user-1",
            "worker-1",
            &format!("token-{run_id}"),
            now + 1,
            now + 10_000,
            &format!("event-claim-{run_id}"),
        )
        .await
        .expect("claim run")
        .expect("run claim");
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
                terminal_outcome: Some(terminal_outcome),
                event_id: format!("event-finish-{run_id}"),
                event_type: format!("run_{}", status.as_str()),
                event_payload: json!({"status": status}),
                occurred_at_unix_ms: now + 2,
            },
        )
        .await
        .expect("finish run");
}

#[tokio::test]
async fn terminal_runs_unlock_or_block_the_remaining_dag() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-graph"), &graph(), 1_000)
        .await
        .expect("create graph");

    finish_next(&storage, "run-a", LocalAgentRunStatus::Succeeded, 2_000).await;
    let after_success = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(after_success.status, LocalTaskGraphStatus::Running);
    assert_eq!(after_success.tasks[0].status, LocalTaskStatus::Succeeded);
    assert_eq!(after_success.tasks[1].status, LocalTaskStatus::Ready);
    assert_eq!(after_success.tasks[2].status, LocalTaskStatus::Pending);

    finish_next(&storage, "run-b", LocalAgentRunStatus::Failed, 3_000).await;
    let after_failure = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(after_failure.status, LocalTaskGraphStatus::Failed);
    assert_eq!(after_failure.tasks[1].status, LocalTaskStatus::Failed);
    assert_eq!(after_failure.tasks[2].status, LocalTaskStatus::Blocked);
    assert_eq!(after_failure.tasks[3].status, LocalTaskStatus::Blocked);
    assert!(storage
        .start_next_task_run("user-1", "run-none", "event-none", 4_000)
        .await
        .expect("start next")
        .is_none());
}

#[tokio::test]
async fn generic_run_cancellation_reconciles_its_task_graph() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-cancel-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run("user-1", "run-cancel", "event-start-cancel", 2_000)
        .await
        .expect("start task")
        .expect("ready task");
    storage
        .cancel_run(
            &command("cancel-run"),
            &run.run_id,
            Some(run.version),
            "user cancelled Run",
            "event-cancel-run",
            3_000,
        )
        .await
        .expect("cancel Run");

    let graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(graph.status, LocalTaskGraphStatus::Cancelled);
    assert_eq!(graph.tasks[0].status, LocalTaskStatus::Cancelled);
    assert!(graph.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Blocked));
}

#[tokio::test]
async fn dependent_task_receives_only_user_visible_prerequisite_results() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-context-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    finish_next_with_outcome(
        &storage,
        "run-prerequisite",
        LocalAgentRunStatus::Succeeded,
        json!({
            "content": "Implemented the data model.",
            "report": {"content": "Tests: 12 passed"},
            "reasoning": "hidden chain of thought must never be forwarded"
        }),
        2_000,
    )
    .await;

    let dependent = storage
        .start_next_task_run("user-1", "run-dependent", "event-start-dependent", 3_000)
        .await
        .expect("start dependent")
        .expect("dependent run");
    let prompt = dependent.input["prompt"].as_str().expect("Task prompt");

    assert!(prompt.contains("[Prerequisite Task Results]"));
    assert!(prompt.contains("Implemented the data model."));
    assert!(prompt.contains("Tests: 12 passed"));
    assert!(prompt.contains("[Current Task]"));
    assert!(!prompt.contains("hidden chain of thought"));
    assert_eq!(
        dependent.input["resolved_prerequisites"][0]["run_id"],
        "run-prerequisite"
    );
}

#[tokio::test]
async fn existing_task_ids_preserve_cross_graph_dependency_execution_and_context() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let task = |task_id: &str, prerequisites: Value| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "objective": format!("Complete {task_id}"),
            "prompt": format!("Task Objective:\nComplete {task_id}"),
            "prerequisite_task_ids": prerequisites,
        }),
        max_iterations: 4,
    };
    storage
        .create_task_graph(
            &command("create-existing-prerequisite"),
            &CreateTaskGraphCommand {
                graph_id: "graph-existing-prerequisite".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-1".to_string(),
                tasks: vec![task("existing-prerequisite", json!([]))],
                dependencies: Vec::new(),
            },
            1_000,
        )
        .await
        .expect("create prerequisite graph");
    let dependent = storage
        .create_task_graph(
            &command("create-cross-graph-dependent"),
            &CreateTaskGraphCommand {
                graph_id: "graph-cross-dependent".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-2".to_string(),
                tasks: vec![task("cross-dependent", json!(["existing-prerequisite"]))],
                dependencies: Vec::new(),
            },
            1_100,
        )
        .await
        .expect("create dependent graph");
    assert_eq!(dependent.tasks[0].status, LocalTaskStatus::Pending);

    finish_next_with_outcome(
        &storage,
        "run-existing-prerequisite",
        LocalAgentRunStatus::Succeeded,
        json!({"content": "Existing prerequisite result"}),
        2_000,
    )
    .await;

    let dependent = storage
        .start_next_task_run(
            "user-1",
            "run-cross-dependent",
            "event-start-cross-dependent",
            3_000,
        )
        .await
        .expect("start cross-graph dependent")
        .expect("dependent is ready");
    assert_eq!(dependent.owner_entity_id, "cross-dependent");
    assert!(dependent.input["prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.contains("Existing prerequisite result")));
    assert_eq!(
        dependent.input["resolved_prerequisites"][0]["task_id"],
        "existing-prerequisite"
    );
}

#[tokio::test]
async fn root_task_input_is_not_rewritten_without_prerequisites() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-root-graph"), &graph(), 1_000)
        .await
        .expect("create graph");

    let root = storage
        .start_next_task_run("user-1", "run-root", "event-start-root", 2_000)
        .await
        .expect("start root")
        .expect("root run");

    assert_eq!(
        root.input,
        json!({
            "task": "task-a",
            "objective": "Complete task-a",
            "prompt": "Task Objective:\nComplete task-a"
        })
    );
}

#[tokio::test]
async fn future_contact_async_task_waits_until_its_run_at_deadline() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let mut scheduled = graph();
    scheduled.graph_id = "graph-scheduled".to_string();
    scheduled.tasks.truncate(1);
    scheduled.dependencies.clear();
    scheduled.tasks[0].input["schedule"] = json!({
        "mode": "contact_async",
        "run_at": "1970-01-01T00:00:05Z",
        "run_at_unix_ms": 5_000,
    });
    storage
        .create_task_graph(&command("create-scheduled-graph"), &scheduled, 1_000)
        .await
        .expect("create scheduled graph");

    assert!(storage
        .start_next_task_run("user-1", "run-too-early", "event-too-early", 4_999)
        .await
        .expect("check early schedule")
        .is_none());
    let run = storage
        .start_next_task_run("user-1", "run-on-time", "event-on-time", 5_000)
        .await
        .expect("start due schedule")
        .expect("scheduled Task is due");
    assert_eq!(run.owner_entity_id, "task-a");
}

#[tokio::test]
async fn ai_reported_blocked_outcome_overrides_a_successful_model_run() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-reported-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    storage
        .start_next_task_run(
            "user-1",
            "run-reported-blocked",
            "event-start-reported",
            2_000,
        )
        .await
        .expect("start task")
        .expect("ready task");
    let claim = storage
        .claim_next_run(
            &command("claim-reported"),
            "user-1",
            "worker-1",
            "token-reported",
            2_001,
            12_000,
            "event-claim-reported",
        )
        .await
        .expect("claim")
        .expect("run claim");
    sqlx::query(
        "INSERT INTO local_agent_tool_invocations(\
             invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
             side_effecting, requires_approval, approval_status, status, result_json, error_text, \
             version, created_at_unix_ms, updated_at_unix_ms) \
             VALUES('outcome-1', ?, 'batch-1', 'call-1', ?, ?, 0, 0, 'not_required', \
             'succeeded', '{}', NULL, 2, 2002, 2002)",
    )
    .bind(&claim.run.run_id)
    .bind(TASK_OUTCOME_REPORT_TOOL)
    .bind(r#"{"status":"blocked","reason":"Waiting for a required credential."}"#)
    .execute(&storage.pool)
    .await
    .expect("store reported outcome");
    storage
        .apply_transition(
            &command("finish-reported"),
            &RunTransition {
                run_id: claim.run.run_id,
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
                terminal_outcome: Some(json!({"content": "Model response"})),
                event_id: "event-finish-reported".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"status": "succeeded"}),
                occurred_at_unix_ms: 2_003,
            },
        )
        .await
        .expect("finish run");

    let graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(graph.tasks[0].status, LocalTaskStatus::Blocked);
    assert!(graph.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Blocked));

    let retried = storage
        .retry_task(
            &command("retry-reported-block"),
            "user-1",
            &graph.tasks[0].task_id,
            graph.tasks[0].version,
            Some("Use the corrected date range."),
            3_000,
        )
        .await
        .expect("retry explicitly blocked task");
    assert_eq!(retried.tasks[0].status, LocalTaskStatus::Ready);
    assert!(retried.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Pending));
}

#[tokio::test]
async fn expired_model_claim_blocks_its_task_instead_of_leaving_it_running() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-expired-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run(
            "user-1",
            "run-expired-task",
            "event-start-expired-task",
            2_000,
        )
        .await
        .expect("start task")
        .expect("ready task");
    storage
        .claim_next_run(
            &command("claim-expired-task"),
            "user-1",
            "worker-1",
            "claim-expired-task",
            2_001,
            3_000,
            "event-claim-expired-task",
        )
        .await
        .expect("claim task run")
        .expect("claimed task run");

    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("recover expired task run"),
        1
    );

    let recovered_run = storage
        .get_run(&run.run_id)
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(recovered_run.status, LocalAgentRunStatus::NeedsReview);
    let recovered_graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(recovered_graph.tasks[0].status, LocalTaskStatus::Blocked);
    assert!(recovered_graph.tasks[0].active_run_id.is_none());
    assert!(recovered_graph.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Blocked));
}

#[tokio::test]
async fn startup_repairs_a_legacy_needs_review_run_with_a_running_task() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-stranded-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run(
            "user-1",
            "run-stranded-task",
            "event-start-stranded-task",
            2_000,
        )
        .await
        .expect("start task")
        .expect("ready task");
    storage
        .claim_next_run(
            &command("claim-stranded-task"),
            "user-1",
            "worker-1",
            "claim-stranded-task",
            2_001,
            3_000,
            "event-claim-stranded-task",
        )
        .await
        .expect("claim task run")
        .expect("claimed task run");
    sqlx::query(
        "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
         claim_token = NULL, claim_until_unix_ms = NULL WHERE run_id = ?",
    )
    .bind(&run.run_id)
    .execute(&storage.pool)
    .await
    .expect("simulate legacy recovery");

    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("repair stranded task run"),
        1
    );

    let recovered_graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(recovered_graph.tasks[0].status, LocalTaskStatus::Blocked);
    assert!(recovered_graph.tasks[0].active_run_id.is_none());
}
