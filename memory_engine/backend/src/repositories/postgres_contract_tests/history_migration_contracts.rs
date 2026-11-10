// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn history_repair_quarantines_untrusted_outputs_and_restores_partition_backlog() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");

    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec![subject.clone(), "memory-schema:v2".to_string()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create history migration thread");
    let summary = crate::repositories::summaries::insert_test_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject,
        "legacy unstructured summary",
        Some(record_id.clone()),
        Some(record_id.clone()),
        1,
        Some("project:alpha"),
    )
    .await
    .expect("insert legacy summary");
    let mut tx = pool.begin().await.expect("begin legacy record insert");
    crate::repositories::records::upsert_record_row(
        &mut tx,
        &EngineRecord {
            id: record_id.clone(),
            thread_id: thread_id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: "historical source".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_turn_id": "turn-1",
                "memory_project_id": "project:alpha"
            })),
            summary_status: "summarized".to_string(),
            summary_id: Some(summary.id.clone()),
            summarized_at: Some("2026-11-09T00:00:00Z".to_string()),
            created_at: "2026-11-09T00:00:00Z".to_string(),
        },
    )
    .await
    .expect("insert legacy summarized record");
    sqlx::query(
        "UPDATE engine_records SET project_partition_key='project:alpha' \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .execute(&mut *tx)
    .await
    .expect("set historical record partition");
    tx.commit().await.expect("commit legacy record");
    let memory_id = format!("memory-{suffix}");
    sqlx::query(
        "INSERT INTO engine_subject_memories \
         (id,tenant_id,source_id,subject_id,memory_key,memory_type,level,source_digest, \
          relation_subject_id,status,rollup_status,created_at,updated_at,data, \
          project_partition_key,recall_eligible) \
         VALUES($1,$2,$3,$4,$5,'agent_memory',0,NULL,NULL,'active','pending',now(),now(),$6, \
                'project:alpha',true)",
    )
    .bind(&memory_id)
    .bind(&tenant)
    .bind(&source)
    .bind(&subject)
    .bind(format!("legacy:{suffix}"))
    .bind(sqlx::types::Json(json!({"text": "legacy memory"})))
    .execute(&pool)
    .await
    .expect("insert legacy subject memory");

    let report = crate::repositories::history_migrations::run_m2a_history_repair(
        &pool,
        &tenant,
        &source,
        "test-operator",
        "contract test history repair",
    )
    .await
    .expect("run history repair");
    assert_eq!(report.restored_records, 1);
    assert_eq!(report.retired_summaries, 1);
    assert_eq!(report.retired_subject_memories, 1);
    assert_eq!(report.rebuild_backlog_items, 4);
    assert_eq!(report.pending_partitions, 1);

    let record_state: (String, Option<String>, String) = sqlx::query_as(
        "SELECT summary_status,summary_id,project_partition_key FROM engine_records \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("load repaired record");
    assert_eq!(
        record_state,
        ("pending".to_string(), None, "project:alpha".to_string())
    );
    let summary_state: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status,recall_eligible,retired_reason FROM engine_summaries \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .fetch_one(&pool)
    .await
    .expect("load retired summary");
    assert_eq!(
        summary_state,
        (
            "retired".to_string(),
            false,
            Some("legacy_summary_untrusted".to_string())
        )
    );
    let memory_state: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status,recall_eligible,retired_reason FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&memory_id)
    .fetch_one(&pool)
    .await
    .expect("load retired memory");
    assert_eq!(
        memory_state,
        (
            "retired".to_string(),
            false,
            Some("legacy_memory_lineage_untrusted".to_string())
        )
    );
    let statuses: (i64, i64, String) = sqlx::query_as(
        "SELECT pending_record_count,pending_summary_tokens,summary_status \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
           AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load rebuilt partition state");
    assert_eq!(statuses.0, 1);
    assert!(statuses.1 > 0);
    assert_eq!(statuses.2, "pending");
    let dispatch: (i64, i64, i64) = sqlx::query_as(
        "SELECT summary_dispatch_version,summary_dispatch_force_version, \
                (SELECT COUNT(*) FROM engine_derived_event_outbox event \
                  WHERE event.tenant_id=partition.tenant_id AND event.source_id=partition.source_id \
                    AND event.event_kind='thread_summary_requested' \
                    AND event.project_partition_key=partition.project_partition_key \
                    AND event.dispatch_version=partition.summary_dispatch_version) \
           FROM engine_thread_summary_partitions partition \
          WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
            AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load migration rebuild dispatch");
    assert!(dispatch.0 > 0);
    assert_eq!(dispatch.1, dispatch.0);
    assert_eq!(dispatch.2, 1);
    let run_status: String =
        sqlx::query_scalar("SELECT status FROM engine_reliability_migration_runs WHERE id=$1")
            .bind(&report.migration_run_id)
            .fetch_one(&pool)
            .await
            .expect("load migration run");
    assert_eq!(run_status, "completed");

    for expected_kind in [
        "thread_l0",
        "thread_rollup",
        "project_memory",
        "agent_memory",
    ] {
        let claim =
            crate::repositories::partition_rebuilds::claim_next(&pool, "contract-worker", 300)
                .await
                .expect("claim rebuild backlog")
                .expect("expected rebuild backlog item");
        assert_eq!(claim.migration_run_id, report.rebuild_run_id);
        assert_eq!(claim.rebuild_kind, expected_kind);
        assert!(crate::repositories::partition_rebuilds::record_completed(
            &pool,
            &claim,
            json!({"phase": format!("{expected_kind}_complete")}),
        )
        .await
        .expect("complete rebuild backlog item"));
    }
    assert!(
        crate::repositories::partition_rebuilds::claim_next(&pool, "contract-worker", 300,)
            .await
            .expect("check drained rebuild backlog")
            .is_none()
    );
    let rebuild_run: (String, String, i64, i64) = sqlx::query_as(
        "SELECT status,cursor->>'phase', \
                (SELECT COUNT(*) FROM engine_partition_rebuild_attempts \
                  WHERE migration_run_id=run.id AND status='completed'), \
                (SELECT COUNT(*) FROM engine_reliability_migration_actions \
                  WHERE migration_run_id=run.id AND action_kind='complete_partition_rebuild') \
           FROM engine_reliability_migration_runs run WHERE id=$1",
    )
    .bind(&report.rebuild_run_id)
    .fetch_one(&pool)
    .await
    .expect("load completed rebuild run");
    assert_eq!(
        rebuild_run,
        ("completed".into(), "m3b_complete".into(), 4, 4)
    );
}

#[tokio::test]
async fn rebuild_rearm_preserves_attempt_history_and_resets_retry_budget() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec![subject, "memory-schema:v2".to_string()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create rebuild rearm thread");
    let migration_run_id = format!("migration-{suffix}");
    sqlx::query(
        "INSERT INTO engine_reliability_migration_runs( \
         id,migration_kind,tenant_id,source_id,status) \
         VALUES($1,'m3b_partition_rebuild',$2,$3,'running')",
    )
    .bind(&migration_run_id)
    .bind(&tenant)
    .bind(&source)
    .execute(&pool)
    .await
    .expect("create rebuild migration run");
    sqlx::query(
        "INSERT INTO engine_partition_rebuild_backlog( \
         tenant_id,source_id,thread_id,project_partition_key,rebuild_kind, \
         status,migration_run_id,reason) \
         VALUES($1,$2,$3,'project:alpha','thread_l0','pending',$4,'contract test')",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(&migration_run_id)
    .execute(&pool)
    .await
    .expect("create rebuild backlog");

    for expected_attempt in 1..=8 {
        let claim = crate::repositories::partition_rebuilds::claim_next(
            &pool,
            "contract-worker",
            300,
        )
        .await
        .expect("claim rebuild attempt")
        .expect("pending rebuild item");
        assert_eq!(claim.attempt_count, expected_attempt);
        assert_eq!(claim.attempt_number, expected_attempt);
        assert!(crate::repositories::partition_rebuilds::record_failed(
            &pool,
            &claim,
            "contract_failure",
        )
        .await
        .expect("record failed rebuild attempt"));
    }
    sqlx::query(
        "UPDATE engine_partition_rebuild_backlog SET status='pending',attempt_count=0, \
         lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,last_error=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
           AND project_partition_key='project:alpha' AND rebuild_kind='thread_l0' \
           AND status='failed'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .execute(&pool)
    .await
    .expect("rearm failed rebuild item");

    let claim = crate::repositories::partition_rebuilds::claim_next(
        &pool,
        "contract-worker",
        300,
    )
    .await
    .expect("claim rearmed rebuild attempt")
    .expect("rearmed rebuild item");
    assert_eq!(claim.attempt_count, 1);
    assert_eq!(claim.attempt_number, 9);
    assert!(crate::repositories::partition_rebuilds::record_completed(
        &pool,
        &claim,
        json!({"phase": "complete_after_rearm"}),
    )
    .await
    .expect("complete rearmed rebuild item"));
    let attempts: Vec<(i64, String)> = sqlx::query_as(
        "SELECT attempt_number,status FROM engine_partition_rebuild_attempts \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         ORDER BY attempt_number",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_all(&pool)
    .await
    .expect("load preserved rebuild attempts");
    assert_eq!(attempts.len(), 9);
    assert_eq!(attempts.last(), Some(&(9, "completed".to_string())));
}
