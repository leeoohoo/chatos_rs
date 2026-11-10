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

#[tokio::test]
async fn terminal_summary_jobs_release_uncovered_record_claims() {
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
            labels: Some(vec!["memory-schema:v2".to_string(), subject.clone()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create recovery thread");
    let failed_record_id = format!("record-failed-{suffix}");
    let running_record_id = format!("record-running-{suffix}");
    crate::repositories::records::batch_sync_records(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &crate::models::BatchSyncRecordsRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            project_partition_key: "project:alpha".to_string(),
            runtime_context_assertion: "repository-test".to_string(),
            records: vec![
                record(failed_record_id.as_str(), "alpha"),
                record(running_record_id.as_str(), "alpha"),
            ],
        },
    )
    .await
    .expect("seed recovery records");

    let create_job = |trigger_type: &str| crate::models::CreateEngineJobRunRequest {
        job_type: "summary".to_string(),
        trigger_type: trigger_type.to_string(),
        tenant_id: Some(tenant.clone()),
        source_id: Some(source.clone()),
        thread_id: Some(thread_id.clone()),
        subject_id: Some(subject.clone()),
        thread_label: None,
        metadata: None,
    };
    let failed_job = crate::repositories::control_plane::create_job_run(
        &pool,
        create_job("failed-recovery-test"),
    )
    .await
    .expect("create failed summary job");
    let running_job = crate::repositories::control_plane::create_job_run(
        &pool,
        create_job("running-recovery-test"),
    )
    .await
    .expect("create running summary job");
    assert_eq!(
        crate::repositories::records::claim_records_for_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&failed_record_id),
            &failed_job.id,
        )
        .await
        .expect("claim failed job record"),
        1
    );
    assert_eq!(
        crate::repositories::records::claim_records_for_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&running_record_id),
            &running_job.id,
        )
        .await
        .expect("claim running job record"),
        1
    );
    crate::repositories::control_plane::finish_job_run(
        &pool,
        &failed_job.id,
        crate::models::FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: 1,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: 1,
            metadata: None,
            error_message: Some("simulated terminal failure".to_string()),
        },
    )
    .await
    .expect("finish failed summary job");

    assert_eq!(
        crate::repositories::records::release_records_from_terminal_summary_jobs(&pool, 10)
            .await
            .expect("release terminal summary claims"),
        1
    );
    let failed_record = crate::repositories::records::get_record_by_id(
        &pool,
        &failed_record_id,
        &tenant,
        &source,
        Some(&thread_id),
    )
    .await
    .expect("load released record")
    .expect("released record");
    let running_record = crate::repositories::records::get_record_by_id(
        &pool,
        &running_record_id,
        &tenant,
        &source,
        Some(&thread_id),
    )
    .await
    .expect("load running record")
    .expect("running record");
    assert_eq!(failed_record.summary_status, "pending");
    assert_eq!(running_record.summary_status, "summarizing");
}

#[tokio::test]
async fn durable_job_can_reenter_released_partition_but_cannot_cross_owner_fence() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{thread_id}");
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
            labels: Some(vec!["memory-schema:v2".to_string(), subject]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create restart recovery thread");
    crate::repositories::records::batch_sync_records(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &crate::models::BatchSyncRecordsRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            project_partition_key: "project:alpha".to_string(),
            runtime_context_assertion: "repository-test".to_string(),
            records: vec![record(&record_id, "alpha")],
        },
    )
    .await
    .expect("seed restart recovery record");

    let summary_job = crate::repositories::control_plane::create_job_run(
        &pool,
        crate::models::CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "restart-contract".to_string(),
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
            thread_id: Some(thread_id.clone()),
            subject_id: None,
            thread_label: None,
            metadata: None,
        },
    )
    .await
    .expect("create durable summary parent");

    crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        &summary_job.id,
        300,
    )
    .await
    .expect("claim summary partition")
    .expect("summary partition available");
    assert_eq!(
        crate::repositories::records::claim_records_for_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&record_id),
            &summary_job.id,
        )
        .await
        .expect("freeze original summary record"),
        1
    );
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET summary_lock_expires_at=now()-interval '1 second' \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .execute(&pool)
    .await
    .expect("expire original summary partition");
    crate::repositories::thread_partitions::release_expired_summary_claims(&pool, 10)
        .await
        .expect("preserve expired partition for running parent");
    let preserved_owner: Option<String> = sqlx::query_scalar(
        "SELECT summary_job_run_id FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha' \
           AND summary_status='running'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_optional(&pool)
    .await
    .expect("load partition preserved for running parent")
    .flatten();
    assert_eq!(preserved_owner.as_deref(), Some(summary_job.id.as_str()));
    assert!(crate::repositories::thread_partitions::try_claim_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "summary-other",
        300,
    )
    .await
    .expect("reject takeover from a running parent")
    .is_none());
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET summary_status='pending', \
         summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .execute(&pool)
    .await
    .expect("simulate a partition released by the pre-fix cleaner");
    assert!(
        crate::repositories::thread_partitions::refresh_summary_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            &summary_job.id,
            300,
        )
        .await
        .expect("reenter released summary partition")
    );
    assert!(
        !crate::repositories::thread_partitions::refresh_summary_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            "summary-other",
            300,
        )
        .await
        .expect("fence different summary owner")
    );

    assert!(
        crate::repositories::thread_partitions::release_summary_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            &summary_job.id,
            0,
            0,
        )
        .await
        .expect("release recovered summary partition")
    );
    let rollup_job = crate::repositories::control_plane::create_job_run(
        &pool,
        crate::models::CreateEngineJobRunRequest {
            job_type: "rollup".to_string(),
            trigger_type: "restart-contract".to_string(),
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
            thread_id: Some(thread_id.clone()),
            subject_id: None,
            thread_label: None,
            metadata: None,
        },
    )
    .await
    .expect("create durable rollup parent");
    assert!(crate::repositories::thread_partitions::try_claim_rollup(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        &rollup_job.id,
        300,
    )
    .await
    .expect("claim original rollup partition")
    .is_some());
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET rollup_lock_expires_at=now()-interval '1 second' \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .execute(&pool)
    .await
    .expect("expire original rollup partition");
    assert!(crate::repositories::thread_partitions::try_claim_rollup(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        "rollup-other",
        300,
    )
    .await
    .expect("reject rollup takeover from a running parent")
    .is_none());
    assert!(
        crate::repositories::thread_partitions::release_rollup_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            &rollup_job.id,
        )
        .await
        .expect("release original rollup partition")
    );
    assert!(
        crate::repositories::thread_partitions::refresh_rollup_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            &rollup_job.id,
            300,
        )
        .await
        .expect("reenter released rollup partition")
    );
    assert!(
        !crate::repositories::thread_partitions::refresh_rollup_claim(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            "rollup-other",
            300,
        )
        .await
        .expect("fence different rollup owner")
    );
}
