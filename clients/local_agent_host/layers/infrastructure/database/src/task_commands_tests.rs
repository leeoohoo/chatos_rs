// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{LocalAgentRunStore, LocalAgentTaskStore, RunTransition};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunStatus, LocalTaskDependency, LocalTaskGraphStatus,
    LocalTaskSpec,
};
use serde_json::json;

fn command(id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: id.to_string(),
        persist_receipt: true,
    }
}

fn graph(graph_id: &str, with_child: bool) -> CreateTaskGraphCommand {
    let task = |task_id: &str| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"task": task_id}),
        max_iterations: 4,
    };
    CreateTaskGraphCommand {
        graph_id: graph_id.to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: if with_child {
            vec![task("task-root"), task("task-child")]
        } else {
            vec![task("task-root")]
        },
        dependencies: if with_child {
            vec![LocalTaskDependency {
                task_id: "task-child".to_string(),
                prerequisite_task_id: "task-root".to_string(),
            }]
        } else {
            Vec::new()
        },
    }
}

async fn finish_next_success(storage: &SqliteClientStorage, run_id: &str, now_unix_ms: i64) {
    storage
        .start_next_task_run(
            "user-1",
            run_id,
            &format!("event-start-{run_id}"),
            now_unix_ms,
        )
        .await
        .expect("start task")
        .expect("ready task");
    let claim = storage
        .claim_next_run(
            &command(&format!("claim-{run_id}")),
            "user-1",
            "worker-1",
            &format!("token-{run_id}"),
            now_unix_ms + 1,
            now_unix_ms + 10_000,
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
                next_status: LocalAgentRunStatus::Succeeded,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"status": "succeeded"})),
                event_id: format!("event-finish-{run_id}"),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"status": "succeeded"}),
                occurred_at_unix_ms: now_unix_ms + 2,
            },
        )
        .await
        .expect("finish run");
}

#[tokio::test]
async fn cancel_and_retry_recompute_blocked_descendants() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create"), &graph("graph-1", true), 1_000)
        .await
        .expect("create graph");
    let cancelled = storage
        .cancel_task(
            &command("cancel"),
            "user-1",
            "task-root",
            Some(1),
            "no longer needed",
            &[],
            "unused-run-event",
            2_000,
        )
        .await
        .expect("cancel task");
    assert_eq!(cancelled.tasks[0].status, LocalTaskStatus::Cancelled);
    assert_eq!(cancelled.tasks[1].status, LocalTaskStatus::Cancelled);
    assert_eq!(cancelled.status, LocalTaskGraphStatus::Cancelled);
    let replay = storage
        .cancel_task(
            &command("cancel"),
            "user-1",
            "task-root",
            Some(1),
            "no longer needed",
            &[],
            "different-unused-event",
            3_000,
        )
        .await
        .expect("replay cancel");
    assert_eq!(replay, cancelled);

    let retried = storage
        .retry_task(&command("retry"), "user-1", "task-root", 2, None, 4_000)
        .await
        .expect("retry task");
    assert_eq!(retried.tasks[0].status, LocalTaskStatus::Cancelled);
    assert_eq!(retried.tasks[1].status, LocalTaskStatus::Ready);
    assert_eq!(retried.status, LocalTaskGraphStatus::Running);
}

#[tokio::test]
async fn cancelling_running_task_terminates_its_active_run() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create"), &graph("graph-1", false), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run("user-1", "run-root", "event-run-created", 2_000)
        .await
        .expect("start task")
        .expect("run");
    let cancelled = storage
        .cancel_task(
            &command("cancel"),
            "user-1",
            "task-root",
            Some(2),
            "stop",
            &[],
            "event-run-cancelled",
            3_000,
        )
        .await
        .expect("cancel task");
    assert_eq!(cancelled.tasks[0].status, LocalTaskStatus::Cancelled);
    assert!(cancelled.tasks[0].active_run_id.is_none());
    let stored_run = storage
        .get_run(&run.run_id)
        .await
        .expect("get run")
        .expect("stored run");
    assert_eq!(stored_run.status, LocalAgentRunStatus::Cancelled);
    let events = storage
        .list_events(0, 20, Some(&run.run_id))
        .await
        .expect("events");
    assert!(events
        .iter()
        .any(|event| event.event_type == "task_state_reconciled"));

    let retried = storage
        .retry_task(
            &command("retry"),
            "user-1",
            "task-root",
            cancelled.tasks[0].version,
            Some("try another way"),
            4_000,
        )
        .await
        .expect("retry task");
    assert_eq!(retried.status, LocalTaskGraphStatus::Pending);
    storage
        .start_next_task_run("user-1", "run-root-retry", "event-run-retry", 5_000)
        .await
        .expect("start retry")
        .expect("retry run");
    let latest = storage
        .list_task_runs("user-1", "task-root", 1)
        .await
        .expect("latest run");
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].run_id, "run-root-retry");
    assert_eq!(
        latest[0].input["retry_instructions"],
        serde_json::json!(["try another way"])
    );
    let history = storage
        .list_task_runs("user-1", "task-root", 10)
        .await
        .expect("run history");
    assert_eq!(
        history
            .iter()
            .map(|run| run.run_id.as_str())
            .collect::<Vec<_>>(),
        vec!["run-root-retry", "run-root"]
    );
}

#[tokio::test]
async fn force_restart_rewinds_descendants_and_preserves_run_history() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create"), &graph("graph-1", true), 1_000)
        .await
        .expect("create graph");
    finish_next_success(&storage, "run-root", 2_000).await;
    finish_next_success(&storage, "run-child", 3_000).await;
    let completed = storage
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(completed.status, LocalTaskGraphStatus::Succeeded);
    let root = completed
        .tasks
        .iter()
        .find(|task| task.task_id == "task-root")
        .expect("root task");
    let restart_command = command("restart-root");
    let restarted = storage
        .restart_task(
            &restart_command,
            "user-1",
            "task-root",
            root.version,
            "refresh upstream output",
            "event-restart",
            4_000,
        )
        .await
        .expect("restart root");
    assert_eq!(restarted.status, LocalTaskGraphStatus::Pending);
    assert_eq!(restarted.tasks[0].status, LocalTaskStatus::Pending);
    assert_eq!(restarted.tasks[1].status, LocalTaskStatus::Ready);
    assert_eq!(
        storage
            .restart_task(
                &restart_command,
                "user-1",
                "task-root",
                root.version,
                "refresh upstream output",
                "different-event-prefix",
                5_000,
            )
            .await
            .expect("replay restart"),
        restarted
    );

    storage
        .start_next_task_run("user-1", "run-root-restarted", "event-new-root", 6_000)
        .await
        .expect("start restarted root")
        .expect("restarted Run");
    let running = storage
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("get running graph")
        .expect("graph");
    let running_root = running
        .tasks
        .iter()
        .find(|task| task.task_id == "task-root")
        .expect("running root");
    let restarted_again = storage
        .restart_task(
            &command("restart-running-root"),
            "user-1",
            "task-root",
            running_root.version,
            "replace active attempt",
            "event-restart-running",
            7_000,
        )
        .await
        .expect("restart running root");
    assert_eq!(restarted_again.status, LocalTaskGraphStatus::Pending);
    let cancelled = storage
        .get_run("run-root-restarted")
        .await
        .expect("get cancelled Run")
        .expect("cancelled Run");
    assert_eq!(cancelled.status, LocalAgentRunStatus::Cancelled);
    let history = storage
        .list_task_runs("user-1", "task-root", 10)
        .await
        .expect("root history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].run_id, "run-root-restarted");
    assert_eq!(history[1].run_id, "run-root");
}
