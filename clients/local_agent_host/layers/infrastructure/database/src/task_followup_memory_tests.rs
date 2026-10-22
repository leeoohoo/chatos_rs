// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_ports::{LocalMemoryOutboxStore, LocalMemoryRunClaim};

async fn stage(
    storage: &SqliteClientStorage,
    claim: &LocalAgentRunClaim,
    text: &str,
    now: i64,
) -> Result<(), ClientStorageError> {
    storage
        .stage_step_memory_record(
            &LocalMemoryRunClaim {
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token.clone(),
                version: claim.run.version,
            },
            &format!("{}:assistant:{}", claim.run.run_id, claim.run.iteration),
            "user-1",
            "chatos",
            "root",
            &json!({"content": text}),
            now,
        )
        .await
}

async fn assert_not_syncable(storage: &SqliteClientStorage, now: i64) {
    assert!(storage
        .claim_next_memory_record("user-1", "sync", now, now + 100_000)
        .await
        .unwrap()
        .is_none());
}

async fn staged_count(storage: &SqliteClientStorage) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_step_memory")
        .fetch_one(&storage.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn followup_discards_old_memory_before_or_after_model_reply_and_reuses_iteration() {
    for reply_before_interrupt in [true, false] {
        let storage = SqliteClientStorage::connect_memory().await.unwrap();
        let old = fixture(&storage).await;
        if reply_before_interrupt {
            stage(&storage, &old, "obsolete reply", 7_000)
                .await
                .unwrap();
            assert_eq!(staged_count(&storage).await, 1);
            assert_not_syncable(&storage, 8_000).await;
        }
        guide(&storage, "followup-memory").await;
        assert_eq!(staged_count(&storage).await, 0);
        assert!(matches!(
            stage(&storage, &old, "late obsolete reply", 10_001).await,
            Err(ClientStorageError::Conflict(_))
        ));
        blocked(&storage).await;
        storage
            .resolve_task_followup(
                &idempotency("release-memory"),
                &unchanged("followup-memory"),
                13_000,
            )
            .await
            .unwrap();
        let resumed = claim(&storage, "resumed-memory", 14_000).await.unwrap();
        assert_eq!(resumed.run.iteration, old.run.iteration);
        stage(&storage, &resumed, "current reply", 15_000)
            .await
            .unwrap();
        // Idempotent duplicates are allowed, differing content still conflicts.
        stage(&storage, &resumed, "current reply", 15_001)
            .await
            .unwrap();
        assert!(matches!(
            stage(&storage, &resumed, "different reply", 15_002).await,
            Err(ClientStorageError::Conflict(_))
        ));
        assert_not_syncable(&storage, 16_000).await;
        let command = idempotency("commit-memory");
        let done = transition(&resumed, LocalAgentRunStatus::Succeeded, "memory-done");
        let run = storage.apply_transition(&command, &done).await.unwrap();
        assert_eq!(run.status, LocalAgentRunStatus::Succeeded);
        assert_eq!(staged_count(&storage).await, 0);
        storage.apply_transition(&command, &done).await.unwrap();
        let record = storage
            .claim_next_memory_record("user-1", "sync", 21_000, 30_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.payload["content"], "current reply");
        assert_eq!(
            record.record_id,
            format!("{}:assistant:{}", old.run.run_id, old.run.iteration)
        );
        assert_not_syncable(&storage, 21_001).await;
    }
}

#[tokio::test]
async fn failed_run_commit_rolls_back_memory_publication_and_recovery_discards_staging() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let old = fixture(&storage).await;
    stage(&storage, &old, "uncommitted reply", 7_000)
        .await
        .unwrap();
    let mut expired = transition(&old, LocalAgentRunStatus::Succeeded, "expired");
    expired.occurred_at_unix_ms = 200_000;
    assert!(matches!(
        storage
            .apply_transition(&idempotency("expired"), &expired)
            .await,
        Err(ClientStorageError::Conflict(_))
    ));
    assert_not_syncable(&storage, 200_001).await;
    assert_eq!(staged_count(&storage).await, 1);
    assert!(stage(&storage, &old, "late reply", 200_002).await.is_err());
    storage
        .recover_expired_claims("user-1", 200_003)
        .await
        .unwrap();
    assert_eq!(staged_count(&storage).await, 0);
    assert_not_syncable(&storage, 200_004).await;
}

#[tokio::test]
async fn cancelling_a_run_discards_staged_memory() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let old = fixture(&storage).await;
    stage(&storage, &old, "cancelled reply", 7_000)
        .await
        .unwrap();
    storage
        .cancel_run(
            &idempotency("cancel-memory"),
            &old.run.run_id,
            Some(old.run.version),
            "stop",
            "cancel-memory",
            8_000,
        )
        .await
        .unwrap();
    assert_eq!(staged_count(&storage).await, 0);
    assert_not_syncable(&storage, 9_000).await;
    assert!(stage(&storage, &old, "late reply", 9_001).await.is_err());
}
