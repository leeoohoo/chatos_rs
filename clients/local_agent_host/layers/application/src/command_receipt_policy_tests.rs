// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_ports::LocalAgentRunStore;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, CreateRunCommand, GetRunCommand,
    LOCAL_AGENT_PROTOCOL_VERSION,
};

fn envelope(id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: id.into(),
        command,
    }
}

fn create(run_id: &str) -> HostCommand {
    HostCommand::CreateRun(CreateRunCommand {
        run_id: run_id.into(),
        owner_user_id: "alice".into(),
        owner_entity_type: "conversation".into(),
        owner_entity_id: "conversation-1".into(),
        profile_key: "main_chat".into(),
        model_config_ref: "model-1".into(),
        model_config_revision: "revision-1".into(),
        capability_policy_revision: "policy-1".into(),
        input: json!({"message": "hello"}),
        max_iterations: 4,
    })
}

fn claim() -> HostCommand {
    HostCommand::ClaimNextRun(ClaimNextRunCommand {
        owner_user_id: "alice".into(),
        worker_id: "worker-1".into(),
        lease_duration_ms: 10_000,
    })
}

fn get(run_id: &str) -> HostCommand {
    HostCommand::GetRun(GetRunCommand {
        owner_user_id: "alice".into(),
        run_id: run_id.into(),
    })
}

#[tokio::test]
async fn expired_claim_returns_nonretryable_error_without_claiming_or_recovering_other_work() {
    use std::sync::atomic::AtomicI64;

    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let now = Arc::new(AtomicI64::new(10_000));
    let clock = now.clone();
    let runtime = LocalAgentRuntime::with_clock(
        storage.clone(),
        Arc::new(move || Ok(clock.load(Ordering::SeqCst))),
    );
    for id in ["run-1", "run-2"] {
        runtime
            .try_handle(envelope(&format!("create-{id}"), create(id)))
            .await
            .unwrap();
    }
    let HostResult::Claim {
        claim: Some(original_claim),
    } = runtime
        .try_handle(envelope("old-claim", claim()))
        .await
        .unwrap()
    else {
        panic!("expected original claim")
    };
    now.store(86_410_001, Ordering::SeqCst);
    runtime
        .try_handle(envelope("later-create", create("run-3")))
        .await
        .unwrap();
    let before_events = storage.list_events(0, 100, None).await.unwrap();
    let expired = runtime.handle(envelope("old-claim", claim())).await;
    let error = expired.error.expect("expired historical response");
    assert_eq!(error.code, "command_replay_expired");
    assert!(!error.retryable);
    // Replay must be checked before lease recovery as well as new claiming.
    let original = storage
        .get_run(&original_claim.run.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.status, LocalAgentRunStatus::ModelRunning);
    assert_eq!(
        original.claim_token.as_deref(),
        Some(original_claim.claim_token.as_str())
    );
    for id in ["run-2", "run-3"] {
        let run = storage.get_run(id).await.unwrap().unwrap();
        assert_eq!(run.status, LocalAgentRunStatus::Queued);
        assert_eq!(run.version, 1);
    }
    assert_eq!(
        storage.list_events(0, 100, None).await.unwrap(),
        before_events
    );

    let mismatch = runtime
        .handle(envelope("old-claim", create("different-run")))
        .await;
    assert_eq!(mismatch.error.unwrap().code, "command_mismatch");
    // Reads sharing the expired command ID still return current state.
    assert!(runtime
        .try_handle(envelope("old-claim", get("run-2")))
        .await
        .is_ok());
    assert!(storage.get_run("different-run").await.unwrap().is_none());
}

#[tokio::test]
async fn reads_return_fresh_state_and_do_not_reserve_receipt_command_ids() {
    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
    runtime
        .try_handle(envelope("create-1", create("run-1")))
        .await
        .unwrap();
    let first = runtime
        .try_handle(envelope("read-1", get("run-1")))
        .await
        .unwrap();
    runtime
        .try_handle(envelope("claim-1", claim()))
        .await
        .unwrap();
    let latest = runtime
        .try_handle(envelope("read-1", get("run-1")))
        .await
        .unwrap();
    assert_ne!(
        first, latest,
        "read retries must not replay a stale snapshot"
    );
    let HostResult::Run { run } = latest else {
        panic!("expected Run")
    };
    assert_eq!(run.status, LocalAgentRunStatus::ModelRunning);
    assert_eq!(run.version, 2);

    // A read does not leave a replay row that conflicts with a later write.
    runtime
        .try_handle(envelope("read-1", create("run-2")))
        .await
        .unwrap();
    let events = storage.list_events(0, 100, None).await.unwrap();
    assert_eq!(
        events.len(),
        3,
        "two creates and one claim, with no query side effects"
    );
}

#[tokio::test]
async fn trusted_entry_rejects_other_writes_before_any_state_change() {
    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
    let error = runtime
        .try_handle_ephemeral(envelope("create-1", create("run-1")))
        .await
        .expect_err("CreateRun is not a one-shot scheduler operation");
    assert!(matches!(error, LocalAgentRuntimeError::InvalidRequest(_)));
    assert!(storage.get_run("run-1").await.unwrap().is_none());
    assert!(storage.list_events(0, 100, None).await.unwrap().is_empty());
    runtime
        .try_handle(envelope("create-1", create("run-1")))
        .await
        .unwrap();
}

#[tokio::test]
async fn wire_fields_cannot_disable_write_idempotency() {
    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
    let mut wire = serde_json::to_value(envelope("create-1", create("run-1"))).unwrap();
    wire["persist_receipt"] = json!(false);
    wire["command"]["persist_receipt"] = json!(false);
    let request: HostRequestEnvelope = serde_json::from_value(wire).unwrap();
    let first = runtime.try_handle(request.clone()).await.unwrap();
    assert_eq!(first, runtime.try_handle(request).await.unwrap());
    assert_eq!(storage.list_events(0, 100, None).await.unwrap().len(), 1);

    let error = runtime
        .try_handle(envelope("create-1", create("run-2")))
        .await
        .expect_err("same command id with different input must fail");
    assert!(matches!(
        error,
        LocalAgentRuntimeError::Storage(ClientStorageError::CommandMismatch(_))
    ));
    assert!(storage.get_run("run-2").await.unwrap().is_none());
}

#[tokio::test]
async fn ipc_claim_retry_replays_original_claim_after_the_run_has_advanced() {
    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
    for id in ["run-1", "run-2"] {
        runtime
            .try_handle(envelope(&format!("create-{id}"), create(id)))
            .await
            .unwrap();
    }
    let original = runtime
        .try_handle(envelope("claim-1", claim()))
        .await
        .unwrap();
    let HostResult::Claim {
        claim: Some(first_claim),
    } = original.clone()
    else {
        panic!("expected first claim")
    };
    let commit = HostCommand::CommitStep(CommitStepCommand {
        owner_user_id: "alice".into(),
        run_id: first_claim.run.run_id.clone(),
        claim_token: first_claim.claim_token,
        expected_version: first_claim.run.version,
        outcome: LocalAgentStepOutcome::Succeed {
            output: json!({"answer": 42}),
        },
    });
    let completed = runtime
        .try_handle(envelope("commit-1", commit.clone()))
        .await
        .unwrap();
    assert_eq!(
        completed,
        runtime
            .try_handle(envelope("commit-1", commit))
            .await
            .unwrap()
    );
    assert_eq!(
        original,
        runtime
            .try_handle(envelope("claim-1", claim()))
            .await
            .unwrap()
    );
    let still_ready = storage.get_run("run-2").await.unwrap().unwrap();
    assert_eq!(still_ready.status, LocalAgentRunStatus::Queued);
    assert_eq!(
        still_ready.version, 1,
        "replaying a claim must not claim the next Run"
    );
    let HostResult::Claim {
        claim: Some(second_claim),
    } = runtime
        .try_handle(envelope("claim-2", claim()))
        .await
        .unwrap()
    else {
        panic!("expected second claim")
    };
    assert_eq!(second_claim.run.run_id, "run-2");
    assert_eq!(storage.list_events(0, 100, None).await.unwrap().len(), 5);
}

#[tokio::test]
async fn trusted_scheduler_claim_commit_and_reconciliation_remain_available() {
    let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
    let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
    runtime
        .try_handle(envelope("create-1", create("run-1")))
        .await
        .unwrap();
    let HostResult::Claim { claim: Some(claim) } = runtime
        .try_handle_ephemeral(envelope("internal-claim", claim()))
        .await
        .unwrap()
    else {
        panic!("expected scheduler claim")
    };
    let commit = HostCommand::CommitStep(CommitStepCommand {
        owner_user_id: "alice".into(),
        run_id: claim.run.run_id,
        claim_token: claim.claim_token,
        expected_version: claim.run.version,
        outcome: LocalAgentStepOutcome::Succeed {
            output: json!({"answer": 42}),
        },
    });
    let completed = runtime
        .try_handle_ephemeral(envelope("internal-commit", commit.clone()))
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .try_handle_ephemeral(envelope("internal-commit", commit))
            .await,
        Err(LocalAgentRuntimeError::Storage(
            ClientStorageError::Conflict(_)
        ))
    ));
    assert_eq!(
        completed,
        runtime
            .try_handle_ephemeral(envelope("internal-reconcile", get("run-1")))
            .await
            .unwrap()
    );
    assert_eq!(storage.list_events(0, 100, None).await.unwrap().len(), 3);
}
