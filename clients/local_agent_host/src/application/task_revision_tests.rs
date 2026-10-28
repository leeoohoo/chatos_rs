// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::application::CREATE_TASKS_TOOL;
use chatos_local_agent_protocol::{
    CancelRunCommand, ClaimNextRunCommand, CommitStepCommand, CreateTaskGraphCommand, HostResult,
    LocalAgentRunStatus, LocalAgentStepOutcome, LocalTaskSpec, LocalTaskStatus, RestartTaskCommand,
};

fn corrected_input(task_id: &str) -> Value {
    json!({
        "title": "Corrected work", "objective": "Inspect existing effects; do not publish",
        "requires_execution": false, "enabled_builtin_kinds": [],
        "supersedes_task_ids": [task_id]
    })
}

#[tokio::test]
async fn correction_requires_cancel_and_does_not_reuse_unaffected_active_graph() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    let original = succeeded_output(&executor, invocation(
        "initial-plan", "parent-run", CREATE_TASKS_TOOL,
        json!({"tasks": [
            {"client_ref": "publish", "title": "Publish", "objective": "Publish old result",
             "requires_execution": false, "enabled_builtin_kinds": []},
            {"client_ref": "unrelated", "title": "Unrelated", "objective": "Read a different report",
             "requires_execution": false, "enabled_builtin_kinds": []}
        ]}),
    )).await;
    let old_id = original["created_tasks"][0]["task_id"].as_str().unwrap();
    let unaffected_id = original["created_tasks"][1]["task_id"].as_str().unwrap();
    let old = runtime
        .get_task_for_conversation("user-1", "conversation-1", old_id)
        .await
        .unwrap()
        .unwrap();
    let old_run_id = old.active_run_id.unwrap();
    let create = invocation(
        "corrected",
        "parent-run",
        CREATE_TASK_TOOL,
        corrected_input(old_id),
    );
    let error = executor
        .execute_tool(&create)
        .await
        .expect("replacement rejection precedes mutation");
    let error = preparation_error(error);
    assert!(error.contains("must be cancelled or held for replacement"));

    let cancelled = succeeded_output(
        &executor,
        invocation(
            "stop-old",
            "parent-run",
            CANCEL_TASK_TOOL,
            json!({"task_id": old_id, "reason": "Latest user intent says do not publish"}),
        ),
    )
    .await;
    assert_eq!(cancelled["status"], "cancelled");
    let replacement = succeeded_output(&executor, create.clone()).await;
    assert_ne!(replacement["id"], old_id);
    assert_ne!(replacement["id"], unaffected_id);
    assert_eq!(replacement["title"], "Corrected work");
    let replay = succeeded_output(&executor, create).await;
    assert_eq!(replacement["id"], replay["id"]);
    let model_retry = succeeded_output(
        &executor,
        invocation(
            "corrected-model-retry",
            "parent-run",
            CREATE_TASK_TOOL,
            corrected_input(old_id),
        ),
    )
    .await;
    assert_eq!(replacement["id"], model_retry["id"]);
    let old_run = runtime
        .get_run_for_host_worker(&old_run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_run.status, LocalAgentRunStatus::Cancelled);
    let unaffected = runtime
        .get_task_for_conversation("user-1", "conversation-1", unaffected_id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(unaffected.status, LocalTaskStatus::Cancelled);
    let replacement_id = replacement["id"].as_str().unwrap();
    let stored = runtime
        .get_task_for_conversation("user-1", "conversation-1", replacement_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.input["supersedes_task_ids"], json!([old_id]));
    let old_after_replacement = runtime
        .get_task_for_conversation("user-1", "conversation-1", old_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        old_after_replacement.replacement_task_ids,
        vec![replacement_id.to_string()]
    );
    let old_graph = runtime
        .task_graph_by_id("user-1", &old_after_replacement.graph_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        old_graph
            .tasks
            .iter()
            .find(|task| task.task_id == old_id)
            .unwrap()
            .replacement_task_ids,
        vec![replacement_id.to_string()]
    );
    let queried = succeeded_output(
        &executor,
        invocation(
            "query-old-after-replacement",
            "parent-run",
            GET_TASK_TOOL,
            json!({"task_id": old_id}),
        ),
    )
    .await;
    assert_eq!(queried["requested_task"]["id"], old_id);
    assert_eq!(queried["is_superseded"], true);
    assert_eq!(queried["current_tasks"][0]["id"], replacement_id);
    let local_context = runtime
        .latest_task_context_items("user-1", "conversation-1")
        .await
        .unwrap();
    assert!(local_context.iter().any(|item| {
        item.pointer("/metadata/task_runner_async/task_id")
            .and_then(Value::as_str)
            == Some(old_id)
    }));
}

#[tokio::test]
async fn failed_task_can_be_superseded_without_impossible_cancellation() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    for run_id in ["parent-run", "other-parent-run"] {
        let run = runtime
            .get_run_for_host_worker(run_id)
            .await
            .unwrap()
            .unwrap();
        runtime
            .try_handle(envelope(
                &format!("cancel-{run_id}"),
                HostCommand::CancelRun(CancelRunCommand {
                    owner_user_id: "user-1".into(),
                    run_id: run_id.into(),
                    expected_version: Some(run.version),
                    reason: "isolate the Task Run under test".into(),
                }),
            ))
            .await
            .unwrap();
    }
    let original = succeeded_output(
        &executor,
        invocation(
            "failed-original",
            "parent-run",
            CREATE_TASK_TOOL,
            json!({"title": "Original", "objective": "Attempt work",
                "requires_execution": false, "enabled_builtin_kinds": []}),
        ),
    )
    .await;
    let old_id = original["id"].as_str().unwrap();
    let old_run_id = runtime
        .get_task_for_conversation("user-1", "conversation-1", old_id)
        .await
        .unwrap()
        .unwrap()
        .active_run_id
        .unwrap();

    let HostResult::Claim { claim: Some(claim) } = runtime
        .try_handle(envelope(
            "claim-original-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".into(),
                worker_id: "local-model-worker".into(),
                lease_duration_ms: 30_000,
            }),
        ))
        .await
        .unwrap()
    else {
        panic!("expected the Task Run")
    };
    assert_eq!(claim.run.run_id, old_run_id);
    runtime
        .try_handle(envelope(
            "fail-original-run",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".into(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Fail {
                    error: "provider rejected unsupported reasoning level".into(),
                    detail: Value::Null,
                },
            }),
        ))
        .await
        .unwrap();

    let failed = runtime
        .get_task_for_conversation("user-1", "conversation-1", old_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, LocalTaskStatus::Failed);

    let replacement = succeeded_output(
        &executor,
        invocation(
            "replace-failed",
            "parent-run",
            CREATE_TASK_TOOL,
            corrected_input(old_id),
        ),
    )
    .await;
    assert_ne!(replacement["id"], old_id);
}

#[tokio::test]
async fn correction_cannot_use_cancelled_task_from_another_conversation() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").unwrap();
    let other = succeeded_output(
        &executor,
        invocation(
            "other-create",
            "other-parent-run",
            CREATE_TASK_TOOL,
            json!({"title": "Other", "objective": "Other work", "requires_execution": false,
               "enabled_builtin_kinds": []}),
        ),
    )
    .await;
    let other_id = other["id"].as_str().unwrap();
    succeeded_output(
        &executor,
        invocation(
            "other-stop",
            "other-parent-run",
            CANCEL_TASK_TOOL,
            json!({"task_id": other_id, "reason": "stop"}),
        ),
    )
    .await;
    let error = executor
        .execute_tool(&invocation(
            "cross-scope-correction",
            "parent-run",
            CREATE_TASK_TOOL,
            corrected_input(other_id),
        ))
        .await
        .expect("foreign replacement rejection precedes mutation");
    let error = preparation_error(error);
    assert!(error.contains("superseded task not found"));
}

#[tokio::test]
async fn corrected_batch_creates_new_graph_and_reuses_it_on_model_retry() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").unwrap();
    let original = succeeded_output(
        &executor,
        invocation(
            "batch-original",
            "parent-run",
            CREATE_TASK_TOOL,
            json!({"title": "Original", "objective": "Old work", "requires_execution": false,
               "enabled_builtin_kinds": []}),
        ),
    )
    .await;
    let old_id = original["id"].as_str().unwrap();
    succeeded_output(
        &executor,
        invocation(
            "batch-stop-original",
            "parent-run",
            CANCEL_TASK_TOOL,
            json!({"task_id": old_id, "reason": "Correct the plan"}),
        ),
    )
    .await;
    let args = json!({
        "supersedes_task_ids": [old_id],
        "tasks": [
            {"client_ref": "corrected", "title": "Corrected batch", "objective": "New work",
             "requires_execution": false, "enabled_builtin_kinds": []},
            {"client_ref": "verify", "title": "Verify corrected batch", "objective": "Verify new work",
             "prerequisite_refs": ["corrected"], "requires_execution": false,
             "enabled_builtin_kinds": []}
        ]
    });
    let first = succeeded_output(
        &executor,
        invocation(
            "batch-corrected",
            "parent-run",
            CREATE_TASKS_TOOL,
            args.clone(),
        ),
    )
    .await;
    let retry = succeeded_output(
        &executor,
        invocation(
            "batch-corrected-retry",
            "parent-run",
            CREATE_TASKS_TOOL,
            args,
        ),
    )
    .await;
    assert_eq!(first["created_tasks"][0]["title"], "Corrected batch");
    assert_ne!(first["created_tasks"][0]["task_id"], original["id"]);
    assert_eq!(
        first["created_tasks"][0]["task_id"],
        retry["created_tasks"][0]["task_id"]
    );
    assert_eq!(retry["idempotent_reused"], true);
}

#[tokio::test]
async fn replacement_transaction_rejects_old_task_restarted_after_preflight() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").unwrap();
    let original = succeeded_output(
        &executor,
        invocation(
            "race-original",
            "parent-run",
            CREATE_TASK_TOOL,
            json!({"title": "Original", "objective": "Old work", "requires_execution": false,
               "enabled_builtin_kinds": []}),
        ),
    )
    .await;
    let old_id = original["id"].as_str().unwrap();
    succeeded_output(
        &executor,
        invocation(
            "race-stop",
            "parent-run",
            CANCEL_TASK_TOOL,
            json!({"task_id": old_id, "reason": "Correction"}),
        ),
    )
    .await;
    let parent = executor.parent_run("parent-run").await.unwrap();
    assert!(executor
        .reusable_source_graph(&parent, "conversation-1", &[old_id.into()])
        .await
        .unwrap()
        .is_none());
    let old = runtime
        .get_task_for_conversation("user-1", "conversation-1", old_id)
        .await
        .unwrap()
        .unwrap();
    runtime
        .try_handle(envelope(
            "race-user-restart",
            HostCommand::RestartTask(RestartTaskCommand {
                owner_user_id: "user-1".into(),
                task_id: old_id.into(),
                expected_version: old.version,
                reason: "User restarted old task after replacement preflight".into(),
            }),
        ))
        .await
        .unwrap();
    let mut input = old.input.clone();
    input["supersedes_task_ids"] = json!([old_id]);
    let result = runtime
        .try_handle(envelope(
            "race-create",
            HostCommand::CreateTaskGraph(CreateTaskGraphCommand {
                graph_id: "race-replacement-graph".into(),
                owner_user_id: "user-1".into(),
                source_entity_type: parent.owner_entity_type,
                source_entity_id: parent.owner_entity_id,
                tasks: vec![LocalTaskSpec {
                    task_id: "race-replacement".into(),
                    title: "Replacement".into(),
                    profile_key: old.profile_key,
                    model_config_ref: old.model_config_ref,
                    model_config_revision: old.model_config_revision,
                    capability_policy_revision: old.capability_policy_revision,
                    input,
                    max_iterations: old.max_iterations,
                }],
                dependencies: vec![],
            }),
        ))
        .await
        .expect_err("transaction must recheck cancellation state");
    assert!(result.to_string().contains("became active again"));
    assert!(runtime
        .task_graph_by_id("user-1", "race-replacement-graph")
        .await
        .unwrap()
        .is_none());
}
