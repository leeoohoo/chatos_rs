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
async fn partition_claims_use_one_writer_and_isolate_projects() {
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
            project_partition_key: format!("project:{project}"),
            runtime_context_assertion: "repository-test".to_string(),
            records: vec![record(
                format!("record-{project}-{suffix}").as_str(),
                project,
            )],
        };
        crate::repositories::records::batch_sync_records(
            &test_config("unused".to_string()),
            &pool,
            &thread_id,
            &request,
        )
        .await
        .expect("seed project partition");
    }

    let _alpha = crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "job-alpha",
        300,
    )
    .await
    .expect("claim alpha")
    .expect("alpha claim");
    assert!(crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
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
            "job-alpha",
            0,
            0,
        )
        .await
        .expect("release alpha")
    );
    assert!(crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "job-alpha-next",
        300,
    )
    .await
    .expect("claim alpha after release")
    .is_some());
    assert!(
        !crate::repositories::thread_partitions::fail_owned_summary_claim_and_dead_letter_dispatch(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            "job-alpha",
            "stale_cloud_terminal_callback",
        )
        .await
        .expect("ignore stale summary terminal callback")
    );
    let active_claim: (String, Option<i64>) = sqlx::query_as(
        "SELECT summary_job_run_id,summary_dispatch_dead_letter_version \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load active summary claim after stale callback");
    assert_eq!(active_claim, ("job-alpha-next".to_string(), None));
    assert!(crate::repositories::thread_partitions::try_claim_rollup(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "rollup-alpha",
        300,
    )
    .await
    .expect("claim rollup")
    .is_some());
    assert!(
        crate::repositories::thread_partitions::release_rollup_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            "rollup-alpha",
        )
        .await
        .expect("release v2 rollup claim")
    );
}
