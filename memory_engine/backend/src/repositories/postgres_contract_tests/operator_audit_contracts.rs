// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

async fn create_thread(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
    subject: &str,
) {
    crate::repositories::threads::upsert_thread(
        pool,
        thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            subject_id: subject.to_string(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), subject.to_string()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create operator-audit fixture thread");
}

#[tokio::test]
async fn operator_audit_failure_rolls_back_record_restore_and_dispatch() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");
    create_thread(&pool, &tenant, &source, &thread_id, &subject).await;

    let mut insert = pool.begin().await.expect("begin fixture record insert");
    crate::repositories::records::upsert_record_row(
        &mut insert,
        &EngineRecord {
            id: record_id.clone(),
            thread_id: thread_id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: "restore must remain atomic with its audit".to_string(),
            structured_payload: None,
            metadata: Some(json!({"memory_project_id":"project:atomic"})),
            summary_status: "summarized".to_string(),
            summary_id: Some(format!("missing-summary-{suffix}")),
            summarized_at: Some("2026-11-09T00:00:00Z".to_string()),
            created_at: "2026-11-09T00:00:00Z".to_string(),
        },
    )
    .await
    .expect("insert summarized fixture record");
    sqlx::query(
        "UPDATE engine_records SET project_partition_key='project:atomic' \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .execute(&mut *insert)
    .await
    .expect("set fixture partition");
    insert.commit().await.expect("commit fixture record");

    let mut tx = pool.begin().await.expect("begin atomic restore");
    let restored = crate::repositories::records::restore_record_pending_in_tx(
        &mut tx, &tenant, &source, &record_id,
    )
    .await
    .expect("restore record inside transaction");
    assert!(restored.changed);
    let audit_error = sqlx::query(
        "INSERT INTO engine_reliability_operator_actions( \
         id,tenant_id,source_id,operator_id,action_kind,object_kind,object_id, \
         project_partition_key,reason,before_state,after_state) \
         VALUES($1,$2,$3,'contract-operator','restore_pending','invalid_kind',$4, \
                'project:atomic','force audit constraint failure','{}','{}')",
    )
    .bind(format!("rop-invalid-{suffix}"))
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .execute(&mut *tx)
    .await;
    assert!(audit_error.is_err());
    tx.rollback().await.expect("rollback failed audit transaction");

    let state: (String, Option<String>) = sqlx::query_as(
        "SELECT summary_status,summary_id FROM engine_records \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("load record after rollback");
    assert_eq!(state.0, "summarized");
    assert_eq!(state.1.as_deref(), Some(format!("missing-summary-{suffix}").as_str()));
    let partition_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("count rolled-back dispatch partitions");
    assert_eq!(partition_count, 0);
}

#[tokio::test]
async fn queue_rearm_cas_failure_creates_no_operator_audit() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let event_id = format!("event-{suffix}");
    sqlx::query(
        "INSERT INTO engine_derived_event_outbox(tenant_id,source_id,event_id,event_kind, \
         aggregate_kind,aggregate_id,project_partition_key,dispatch_version,payload,status) \
         VALUES($1,$2,$3,'thread_rollup_requested','thread_summary',$4,'project:atomic',1,$5, \
                'dead_letter')",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&event_id)
    .bind(format!("missing-summary-{suffix}"))
    .bind(sqlx::types::Json(json!({"dispatch_version":1})))
    .execute(&pool)
    .await
    .expect("insert dead-letter fixture");

    let mut tx = pool.begin().await.expect("begin failed CAS rearm");
    let rearmed = crate::repositories::derived_events::rearm_dead_lettered_event_in_tx(
        &mut tx, &tenant, &source, &event_id,
    )
    .await
    .expect("attempt queue rearm");
    assert!(rearmed.is_none());
    tx.commit().await.expect("commit no-op CAS transaction");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_reliability_operator_actions \
         WHERE tenant_id=$1 AND source_id=$2 AND object_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&event_id)
    .fetch_one(&pool)
    .await
    .expect("count operator audits after failed CAS");
    assert_eq!(audit_count, 0);
    let status: String = sqlx::query_scalar(
        "SELECT status FROM engine_derived_event_outbox \
         WHERE tenant_id=$1 AND source_id=$2 AND event_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&event_id)
    .fetch_one(&pool)
    .await
    .expect("load original dead-letter event");
    assert_eq!(status, "dead_letter");
}

#[tokio::test]
async fn resummarize_claim_job_and_operator_audit_commit_together() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    create_thread(&pool, &tenant, &source, &thread_id, &subject).await;
    let summary = crate::repositories::summaries::insert_test_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject,
        "summary to resummarize",
        None,
        None,
        1,
        Some("project:atomic"),
    )
    .await
    .expect("insert resummarize fixture summary");

    let mut tx = pool.begin().await.expect("begin resummarize durable initiation");
    sqlx::query(
        "INSERT INTO engine_covered_range_resummarization_claims(tenant_id,source_id,summary_id, \
         status,lease_token,lease_expires_at,reason) \
         VALUES($1,$2,$3,'running',$4,now()+interval '15 minutes','atomic contract test')",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .bind(format!("lease-{suffix}"))
    .execute(&mut *tx)
    .await
    .expect("insert resummarization claim");
    let job = crate::repositories::control_plane::create_job_run_in_tx(
        &mut tx,
        crate::models::CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "covered_range_resummarize".to_string(),
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
            thread_id: Some(thread_id.clone()),
            subject_id: Some(subject.clone()),
            thread_label: None,
            metadata: Some(json!({"previous_summary_id":summary.id})),
        },
    )
    .await
    .expect("insert resummarization job");
    sqlx::query(
        "UPDATE engine_covered_range_resummarization_claims SET job_run_id=$4 \
         WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 AND status='running'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .bind(&job.id)
    .execute(&mut *tx)
    .await
    .expect("attach resummarization job to claim");
    let action_id = format!("rop-{suffix}");
    sqlx::query(
        "INSERT INTO engine_reliability_operator_actions( \
         id,tenant_id,source_id,operator_id,action_kind,object_kind,object_id, \
         project_partition_key,reason,before_state,after_state) \
         VALUES($1,$2,$3,'contract-operator','resummarize','thread_summary',$4, \
                'project:atomic','atomic contract test','{}',$5)",
    )
    .bind(&action_id)
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .bind(sqlx::types::Json(json!({"job_run_id":job.id,"claim_status":"running"})))
    .execute(&mut *tx)
    .await
    .expect("insert resummarization operator audit");
    tx.commit()
        .await
        .expect("commit claim, job, and audit atomically");

    let durable: (String, String, String) = sqlx::query_as(
        "SELECT claim.status,claim.job_run_id,action.id \
         FROM engine_covered_range_resummarization_claims claim \
         JOIN engine_job_runs job ON job.id=claim.job_run_id \
         JOIN engine_reliability_operator_actions action \
           ON action.tenant_id=claim.tenant_id AND action.source_id=claim.source_id \
          AND action.object_id=claim.summary_id AND action.action_kind='resummarize' \
         WHERE claim.tenant_id=$1 AND claim.source_id=$2 AND claim.summary_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .fetch_one(&pool)
    .await
    .expect("load atomic resummarization initiation");
    assert_eq!(durable, ("running".to_string(), job.id, action_id));
}
