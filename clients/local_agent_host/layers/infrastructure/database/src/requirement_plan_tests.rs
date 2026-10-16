// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_ports::{LocalAgentTaskStore, LocalRequirementSurveyStore};
use chatos_local_agent_protocol::{
    ConfirmRequirementPlanCommand, CreateTaskGraphCommand, LocalConversationResourceBinding,
    LocalConversationResourceKind, LocalRequirementPlan, LocalTaskSpec, SaveRequirementPlanCommand,
};

async fn start_project_turn(
    storage: &SqliteClientStorage,
    conversation_id: &str,
    suffix: &str,
    now: i64,
) -> LocalConversationTurnStart {
    let conversation = storage
        .get_conversation("user-1", conversation_id)
        .await
        .unwrap()
        .unwrap();
    let turn = turn(conversation_id, conversation.conversation.version, suffix);
    let mut record = run(&turn, now);
    record.input = json!({"conversation_id": conversation_id, "message": turn.message});
    storage
        .start_conversation_turn(
            &idempotency(&format!("start-{suffix}")),
            &turn,
            &record,
            &format!("start-event-{suffix}"),
            now,
        )
        .await
        .unwrap()
}

async fn claim(
    storage: &SqliteClientStorage,
    run_id: &str,
    suffix: &str,
    now: i64,
) -> chatos_local_agent_protocol::LocalAgentRunClaim {
    let claim = storage
        .claim_next_run(
            &idempotency(&format!("claim-{suffix}")),
            "user-1",
            "worker",
            &format!("claim-token-{suffix}"),
            now,
            now + 10_000,
            &format!("claim-event-{suffix}"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.run.run_id, run_id);
    claim
}

async fn finish(
    storage: &SqliteClientStorage,
    claim: chatos_local_agent_protocol::LocalAgentRunClaim,
    success: bool,
    markdown: &str,
    suffix: &str,
    now: i64,
) {
    storage
        .apply_transition(
            &idempotency(&format!("finish-{suffix}")),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: if success {
                    LocalAgentRunStatus::Succeeded
                } else {
                    LocalAgentRunStatus::Failed
                },
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"content": markdown})),
                event_id: format!("finish-event-{suffix}"),
                event_type: if success {
                    "run_succeeded"
                } else {
                    "run_failed"
                }
                .into(),
                event_payload: json!({}),
                occurred_at_unix_ms: now,
            },
        )
        .await
        .unwrap();
}

async fn fixture() -> (SqliteClientStorage, LocalRequirementPlan) {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let mut conversation = conversation("project-chat");
    conversation.resource = Some(LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Project,
        resource_id: "project-1".into(),
    });
    storage
        .create_conversation(&idempotency("create-project"), &conversation, 1_000)
        .await
        .unwrap();
    let started = start_project_turn(&storage, "project-chat", "proposal", 2_000).await;
    let claim = claim(&storage, &started.run.run_id, "proposal", 3_000).await;
    // Submission/resumption is exercised by the Host tool integration tests.
    // Isolate version/confirmation guards using a valid resolved survey fixture.
    sqlx::query("INSERT INTO local_requirement_surveys(owner_user_id,survey_id,project_resource_id,source_conversation_id,source_run_id,source_task_id,title,questions_json,answers_json,status,version,created_at_unix_ms,updated_at_unix_ms,resolved_at_unix_ms) \
        VALUES('user-1','survey-1','project-1','project-chat',?,NULL,'Fictional finance proposal','[]','{}','resolved',2,2000,3000,3000)")
        .bind(&started.run.run_id).execute(&storage.pool).await.unwrap();
    finish(
        &storage,
        claim,
        true,
        "## Proposal v1\nFictional vouchers only. No real bookkeeping.",
        "proposal",
        4_000,
    )
    .await;
    let plan = storage
        .get_requirement_plan("user-1", "survey-1", None)
        .await
        .unwrap()
        .unwrap();
    (storage, plan)
}

fn confirmation(revision: u64) -> ConfirmRequirementPlanCommand {
    ConfirmRequirementPlanCommand {
        owner_user_id: "user-1".into(),
        survey_id: "survey-1".into(),
        expected_revision: revision,
    }
}

fn graph(turn_id: &str, suffix: &str) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: format!("graph-{suffix}"),
        owner_user_id: "user-1".into(),
        source_entity_type: "conversation_turn".into(),
        source_entity_id: turn_id.into(),
        tasks: vec![LocalTaskSpec {
            task_id: format!("task-{suffix}"),
            title: "Check fictional voucher criteria".into(),
            profile_key: "task_execution".into(),
            model_config_ref: "model-1".into(),
            model_config_revision: "revision-1".into(),
            capability_policy_revision: "policy-1".into(),
            input: json!({"prompt": "Review fictional criteria without executing accounting",
                "expected_outputs": ["a checklist"], "acceptance_criteria": ["no real accounting operations"],
                "requirement_plan": {"snapshot": {"markdown": "forged body", "revision": 100, "confirmed_at_unix_ms": 1}}}),
            max_iterations: 8,
        }],
        dependencies: vec![],
    }
}

#[tokio::test]
async fn captures_exact_final_reply_and_confirmation_is_versioned_owner_scoped_and_non_executing() {
    let (storage, plan) = fixture().await;
    assert_eq!(plan.revision, 1);
    assert!(plan.confirmed_at_unix_ms.is_none());
    assert_eq!(plan.source_run_id, "run-proposal");
    assert!(plan.markdown.contains("No real bookkeeping"));
    assert!(storage
        .get_requirement_plan("other-user", "survey-1", None)
        .await
        .unwrap()
        .is_none());
    assert!(storage
        .confirm_requirement_plan(&idempotency("stale-confirm"), &confirmation(2), 5_000)
        .await
        .is_err());
    let mut foreign = confirmation(1);
    foreign.owner_user_id = "other-user".into();
    assert!(storage
        .confirm_requirement_plan(&idempotency("foreign-confirm"), &foreign, 5_000)
        .await
        .is_err());
    let forged_reply = SaveRequirementPlanCommand {
        owner_user_id: "user-1".into(),
        survey_id: "survey-1".into(),
        source_run_id: "run-proposal".into(),
        expected_revision: 1,
        markdown: "not the source reply".into(),
    };
    assert!(storage
        .save_requirement_plan(&idempotency("forged-reply"), &forged_reply, 5_000)
        .await
        .is_err());
    let confirmed = storage
        .confirm_requirement_plan(&idempotency("confirm-1"), &confirmation(1), 5_000)
        .await
        .unwrap();
    assert_eq!(confirmed.confirmed_at_unix_ms, Some(5_000));
    let replay = storage
        .confirm_requirement_plan(&idempotency("confirm-1"), &confirmation(1), 6_000)
        .await
        .unwrap();
    assert_eq!(confirmed, replay);
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM local_tasks), (SELECT COUNT(*) FROM local_agent_runs)",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 1), "confirmation must not create a Task or Run");
}

#[tokio::test]
async fn unconfirmed_plan_rejects_graph_atomically_and_confirmed_body_is_program_frozen() {
    let (storage, plan) = fixture().await;
    let started = start_project_turn(&storage, "project-chat", "unconfirmed", 5_000).await;
    assert_eq!(
        started.run.input["requirement_plan"]["markdown"],
        plan.markdown
    );
    assert!(storage
        .create_task_graph(
            &idempotency("blocked-graph"),
            &graph(&started.turn.turn_id, "blocked"),
            6_000
        )
        .await
        .is_err());
    let graph_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_task_graphs")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(graph_count, 0);
    finish(
        &storage,
        claim(&storage, &started.run.run_id, "unconfirmed", 6_000).await,
        true,
        "Awaiting confirmation",
        "unconfirmed",
        7_000,
    )
    .await;
    storage
        .confirm_requirement_plan(&idempotency("confirm-1"), &confirmation(1), 8_000)
        .await
        .unwrap();
    assert!(storage
        .create_task_graph(
            &idempotency("old-context"),
            &graph(&started.turn.turn_id, "old"),
            8_001
        )
        .await
        .is_err());
    let started = start_project_turn(&storage, "project-chat", "execute", 9_000).await;
    assert_eq!(
        started.run.input["requirement_plan"]["confirmed_at_unix_ms"],
        8_000
    );
    let created = storage
        .create_task_graph(
            &idempotency("valid-graph"),
            &graph(&started.turn.turn_id, "valid"),
            10_000,
        )
        .await
        .unwrap();
    assert_eq!(
        created.tasks[0].input["requirement_plan"]["snapshot"]["revision"],
        1
    );
    assert_eq!(
        created.tasks[0].input["requirement_plan"]["snapshot"]["markdown"],
        plan.markdown
    );
    assert_eq!(
        created.tasks[0].input["requirement_plan"]["snapshot"]["confirmed_at_unix_ms"],
        8_000
    );
    assert_eq!(
        created.tasks[0].input["requirement_plan"]["markdown_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(
        created.tasks[0].input["requirement_plan"]["task_scope"]["prompt"]
            .as_str()
            .unwrap()
            .contains("fictional")
    );
    assert_eq!(
        created.tasks[0].input["requirement_plan"]["task_scope"]["expected_outputs"],
        json!(["a checklist"])
    );
    let replay = storage
        .create_task_graph(
            &idempotency("valid-graph"),
            &graph(&started.turn.turn_id, "valid"),
            11_000,
        )
        .await
        .unwrap();
    assert_eq!(replay, created);
}

#[tokio::test]
async fn revision_change_requires_new_confirmation_and_old_task_retries_keep_v1() {
    let (storage, original) = fixture().await;
    storage
        .confirm_requirement_plan(&idempotency("confirm-1"), &confirmation(1), 5_000)
        .await
        .unwrap();
    let execute = start_project_turn(&storage, "project-chat", "execute", 6_000).await;
    let created = storage
        .create_task_graph(
            &idempotency("graph-v1"),
            &graph(&execute.turn.turn_id, "v1"),
            7_000,
        )
        .await
        .unwrap();
    finish(
        &storage,
        claim(&storage, &execute.run.run_id, "execute", 8_000).await,
        true,
        "Task arranged",
        "execute",
        9_000,
    )
    .await;
    let task_run = storage
        .start_next_task_run_for_graph(
            "user-1",
            &created.graph_id,
            "run-task-v1",
            "task-start-v1",
            10_000,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        task_run.input["requirement_plan"]["snapshot"]["markdown"],
        original.markdown
    );
    finish(
        &storage,
        claim(&storage, &task_run.run_id, "task-fail", 11_000).await,
        false,
        "simulated review failure",
        "task-fail",
        12_000,
    )
    .await;
    let revise = start_project_turn(&storage, "project-chat", "revise", 13_000).await;
    let revise_claim = claim(&storage, &revise.run.run_id, "revise", 14_000).await;
    let save = SaveRequirementPlanCommand {
        owner_user_id: "user-1".into(),
        survey_id: "survey-1".into(),
        source_run_id: revise.run.run_id.clone(),
        expected_revision: 1,
        markdown: "## Proposal v2\nFictional reconciliation checks. Still no real bookkeeping."
            .into(),
    };
    let v2 = storage
        .save_requirement_plan(&idempotency("save-2"), &save, 15_000)
        .await
        .unwrap();
    assert_eq!(v2.revision, 2);
    assert!(v2.confirmed_at_unix_ms.is_none());
    assert_eq!(
        storage
            .save_requirement_plan(&idempotency("save-2"), &save, 16_000)
            .await
            .unwrap(),
        v2
    );
    assert!(storage
        .confirm_requirement_plan(
            &idempotency("confirm-before-complete"),
            &confirmation(2),
            16_000
        )
        .await
        .is_err());
    assert!(storage
        .save_requirement_plan(&idempotency("stale-save"), &save, 16_000)
        .await
        .is_err());
    finish(
        &storage,
        revise_claim,
        true,
        &save.markdown,
        "revise",
        17_000,
    )
    .await;
    assert!(storage
        .confirm_requirement_plan(&idempotency("confirm-old"), &confirmation(1), 18_000)
        .await
        .is_err());
    storage
        .confirm_requirement_plan(&idempotency("confirm-2"), &confirmation(2), 19_000)
        .await
        .unwrap();
    assert!(storage
        .create_task_graph(
            &idempotency("stale-main-v1"),
            &graph(&execute.turn.turn_id, "stale"),
            20_000
        )
        .await
        .is_err());
    let current = start_project_turn(&storage, "project-chat", "current", 21_000).await;
    assert_eq!(current.run.input["requirement_plan"]["revision"], 2);
    let new = storage
        .create_task_graph(
            &idempotency("graph-v2"),
            &graph(&current.turn.turn_id, "v2"),
            22_000,
        )
        .await
        .unwrap();
    assert_eq!(
        new.tasks[0].input["requirement_plan"]["snapshot"]["revision"],
        2
    );
    let old = storage
        .get_task_graph("user-1", &created.graph_id)
        .await
        .unwrap()
        .unwrap();
    let retried = storage
        .retry_task(
            &idempotency("retry-v1"),
            "user-1",
            &old.tasks[0].task_id,
            old.tasks[0].version,
            Some("review again"),
            23_000,
        )
        .await
        .unwrap();
    assert_eq!(
        retried.tasks[0].input["requirement_plan"]["snapshot"]["revision"],
        1
    );
    let retry_run = storage
        .start_next_task_run_for_graph(
            "user-1",
            &created.graph_id,
            "run-retry-v1",
            "retry-start-v1",
            24_000,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        retry_run.input["requirement_plan"]["snapshot"]["markdown"],
        original.markdown
    );
    assert_eq!(
        retry_run.input["retry_instructions"],
        json!(["review again"])
    );
}

#[tokio::test]
async fn project_isolation_unbound_snapshots_and_bound_input_limit_are_enforced() {
    let (storage, _) = fixture().await;
    let mut other = conversation("other-project-chat");
    other.resource = Some(LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Project,
        resource_id: "project-2".into(),
    });
    storage
        .create_conversation(&idempotency("create-other"), &other, 5_000)
        .await
        .unwrap();
    let started = start_project_turn(&storage, "other-project-chat", "other", 6_000).await;
    assert!(started.run.input.get("requirement_plan").is_none());
    assert!(storage
        .create_task_graph(
            &idempotency("forged-other"),
            &graph(&started.turn.turn_id, "other"),
            7_000
        )
        .await
        .is_err());
    let mut plain = graph(&started.turn.turn_id, "plain");
    plain.tasks[0]
        .input
        .as_object_mut()
        .unwrap()
        .remove("requirement_plan");
    assert!(storage
        .create_task_graph(&idempotency("plain-other"), &plain, 7_001)
        .await
        .is_ok());
    let mut unbound = graph("not-a-turn", "unbound");
    unbound.source_entity_type = "task".into();
    assert!(storage
        .create_task_graph(&idempotency("unbound-plan"), &unbound, 7_002)
        .await
        .is_err());
    let current = storage
        .get_conversation("user-1", "project-chat")
        .await
        .unwrap()
        .unwrap();
    let turn = turn("project-chat", current.conversation.version, "oversized");
    let mut record = run(&turn, 8_000);
    record.input = json!({"message": "x".repeat(chatos_local_agent_protocol::LOCAL_AGENT_MAX_INPUT_BYTES - 128)});
    assert!(
        serde_json::to_vec(&record.input).unwrap().len()
            < chatos_local_agent_protocol::LOCAL_AGENT_MAX_INPUT_BYTES
    );
    assert!(storage
        .start_conversation_turn(
            &idempotency("oversized-start"),
            &turn,
            &record,
            "oversized-event",
            8_000
        )
        .await
        .is_err());
    assert!(storage
        .get_run_for_owner("user-1", &record.run_id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        storage
            .get_conversation("user-1", "project-chat")
            .await
            .unwrap()
            .unwrap()
            .conversation
            .version,
        current.conversation.version
    );
}
