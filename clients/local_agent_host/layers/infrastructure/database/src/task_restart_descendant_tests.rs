// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, RunTransition, SqliteClientStorage,
};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunStatus, LocalTaskDependency, LocalTaskGraphStatus,
    LocalTaskSpec, LocalTaskStatus,
};
use serde_json::json;

fn command(id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: id.to_string(),
    }
}

fn graph() -> CreateTaskGraphCommand {
    let task = |task_id: &str| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_runner".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"task": task_id}),
        max_iterations: 4,
    };
    CreateTaskGraphCommand {
        graph_id: "graph-1".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![task("task-root"), task("task-child")],
        dependencies: vec![LocalTaskDependency {
            task_id: "task-child".to_string(),
            prerequisite_task_id: "task-root".to_string(),
        }],
    }
}

async fn finish_root(storage: &SqliteClientStorage) {
    storage
        .start_next_task_run("user-1", "run-root", "event-start-run-root", 2_000)
        .await
        .expect("start root")
        .expect("root Run");
    let claim = storage
        .claim_next_run(
            &command("claim-root"),
            "user-1",
            "worker-1",
            "token-root",
            2_001,
            12_000,
            "event-claim-root",
        )
        .await
        .expect("claim root")
        .expect("root claim");
    storage
        .apply_transition(
            &command("finish-root"),
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
                event_id: "event-finish-root".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"status": "succeeded"}),
                occurred_at_unix_ms: 2_002,
            },
        )
        .await
        .expect("finish root");
}

#[tokio::test]
async fn force_restart_cancels_running_descendant() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create"), &graph(), 1_000)
        .await
        .expect("create graph");
    finish_root(&storage).await;
    storage
        .start_next_task_run("user-1", "run-child", "event-start-child", 3_000)
        .await
        .expect("start child")
        .expect("child Run");
    let running = storage
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("get graph")
        .expect("graph");
    let root = running
        .tasks
        .iter()
        .find(|task| task.task_id == "task-root")
        .expect("root task");
    let restarted = storage
        .restart_task(
            &command("restart-root"),
            "user-1",
            "task-root",
            root.version,
            "invalidate active descendant",
            "event-restart",
            4_000,
        )
        .await
        .expect("restart root");
    assert_eq!(restarted.status, LocalTaskGraphStatus::Pending);
    assert_eq!(restarted.tasks[0].status, LocalTaskStatus::Pending);
    assert_eq!(restarted.tasks[1].status, LocalTaskStatus::Ready);
    let child_run = storage
        .get_run("run-child")
        .await
        .expect("get child Run")
        .expect("child Run");
    assert_eq!(child_run.status, LocalAgentRunStatus::Cancelled);
}
