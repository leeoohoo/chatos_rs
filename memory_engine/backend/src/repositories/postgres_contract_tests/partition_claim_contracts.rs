// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::UpsertRecordInput;

fn record(id: &str, project: &str) -> UpsertRecordInput {
    UpsertRecordInput {
        id: id.to_string(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: format!("content-{id}"),
        structured_payload: None,
        metadata: Some(json!({
            "conversation_turn_id": format!("turn-{id}"),
            "memory_project_id": project
        })),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: "2026-11-09T00:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn partition_claims_enforce_writer_cutover_and_isolate_projects() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{thread_id}");
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
            labels: Some(vec!["memory-schema:v2".to_string(), subject]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create partition claim thread");
    for project in ["alpha", "beta"] {
        let request = crate::models::BatchSyncRecordsRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            records: vec![record(
                format!("record-{project}-{suffix}").as_str(),
                project,
            )],
        };
        crate::repositories::records::batch_sync_records_with_partition_dispatch(
            &test_config("unused".to_string()),
            &pool,
            &thread_id,
            &request,
            true,
        )
        .await
        .expect("seed project partition");
    }

    let wrong_writer = crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "v2",
        "job-v2-early",
        300,
    )
    .await
    .expect_err("v2 cannot claim before cutover");
    assert!(wrong_writer.contains("not authoritative"));

    let alpha = crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "v1",
        "job-alpha",
        300,
    )
    .await
    .expect("claim alpha")
    .expect("alpha claim");
    assert_eq!(alpha.authoritative_writer, "v1");
    assert!(crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "v1",
        "job-alpha-race",
        300,
    )
    .await
    .expect("competing alpha claim")
    .is_none());
    let beta = crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:beta",
        "v1",
        "job-beta",
        300,
    )
    .await
    .expect("claim beta while alpha is occupied")
    .expect("independent beta claim");
    assert_eq!(beta.project_partition_key, "project:beta");

    assert!(
        crate::repositories::thread_partitions::release_summary_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            "v1",
            "job-alpha",
            0,
            0,
        )
        .await
        .expect("release alpha")
    );
    assert!(!crate::repositories::thread_partitions::mark_ready(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        alpha.pending_record_count,
        alpha.pending_summary_tokens + 1,
    )
    .await
    .expect("reject mismatched readiness snapshot"));
    assert!(crate::repositories::thread_partitions::mark_ready(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        alpha.pending_record_count,
        alpha.pending_summary_tokens,
    )
    .await
    .expect("mark alpha ready"));
    assert!(crate::repositories::thread_partitions::cutover_to_v2(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
    )
    .await
    .expect("cut over alpha"));
    let retired_v1 = crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "v1",
        "job-v1-late",
        300,
    )
    .await
    .expect_err("v1 cannot claim after cutover");
    assert!(retired_v1.contains("not authoritative"));
    assert!(crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "v2",
        "job-v2",
        300,
    )
    .await
    .expect("claim alpha after v2 cutover")
    .is_some());
    let rollback = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET migration_state='ready',authoritative_writer='v1' \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .execute(&pool)
    .await
    .expect_err("database must reject writer rollback");
    assert!(rollback
        .to_string()
        .contains("invalid partition writer transition"));
}
