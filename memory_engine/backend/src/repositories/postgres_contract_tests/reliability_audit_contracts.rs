// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn m0_audit_lists_p0_migration_objects_without_mutating_them() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject_id = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject_id.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), subject_id.clone()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create audit thread");

    let sentinel_id = format!("record-sentinel-{suffix}");
    let mut tx = pool.begin().await.expect("begin sentinel insert");
    crate::repositories::records::upsert_record_row(
        &mut tx,
        &EngineRecord {
            id: sentinel_id.clone(),
            thread_id: thread_id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            external_record_id: None,
            role: "tool".to_string(),
            record_type: "message".to_string(),
            content: "oversized".to_string(),
            structured_payload: None,
            metadata: Some(json!({"memory_project_id":"alpha"})),
            summary_status: "summarized".to_string(),
            summary_id: Some("skipped_single_record_token_limit".to_string()),
            summarized_at: Some("2026-11-09T00:00:00Z".to_string()),
            created_at: "2026-11-09T00:00:00Z".to_string(),
        },
    )
    .await
    .expect("insert oversized sentinel");
    tx.commit().await.expect("commit sentinel");

    let repair = crate::repositories::summaries::insert_test_summary(
        &pool,
        None,
        &tenant,
        &source,
        &thread_id,
        &subject_id,
        "thread_repair",
        0,
        None,
        "legacy repair",
        None,
        None,
        1,
        Some(json!({"project_subject_id":"project:alpha"})),
        Some("project:alpha"),
    )
    .await
    .expect("create legacy repair");

    let left = crate::repositories::summaries::insert_test_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject_id,
        "project alpha",
        None,
        None,
        1,
        Some("project:alpha"),
    )
    .await
    .expect("create alpha summary");
    let right = crate::repositories::summaries::insert_test_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject_id,
        "project beta",
        None,
        None,
        1,
        Some("project:beta"),
    )
    .await
    .expect("create beta summary");
    let rollup = crate::repositories::summaries::insert_test_summary(
        &pool,
        None,
        &tenant,
        &source,
        &thread_id,
        &subject_id,
        "thread_incremental",
        1,
        Some(format!(
            "thread_rollup:{thread_id}:0:1:{},{}",
            left.id, right.id
        )),
        "mixed rollup",
        Some(left.id.clone()),
        Some(right.id.clone()),
        2,
        Some(json!({"generator":"memory_engine_rollup_v1"})),
        Some("__none__"),
    )
    .await
    .expect("create mixed rollup");
    crate::repositories::summaries::mark_summaries_rolled_up(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &[left.id, right.id],
        rollup.id.as_str(),
    )
    .await
    .expect("link mixed rollup");

    let report = crate::reliability_audit::run_reliability_audit(
        &pool,
        crate::reliability_audit::ReliabilityAuditScope {
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
        },
    )
    .await
    .expect("run reliability audit");
    let kinds = report
        .findings
        .iter()
        .map(|finding| finding.finding_kind.as_str())
        .collect::<HashSet<_>>();
    assert!(kinds.contains("oversized_record_sentinel"));
    assert!(kinds.contains("unreachable_summarized_record"));
    assert!(kinds.contains("legacy_thread_repair"));
    assert!(kinds.contains("cross_project_rollup"));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.object_id == repair.id));

    let stored = crate::repositories::records::get_record_by_id(
        &pool,
        &sentinel_id,
        &tenant,
        &source,
        Some(&thread_id),
    )
    .await
    .expect("reload sentinel")
    .expect("sentinel remains");
    assert_eq!(
        stored.summary_id.as_deref(),
        Some("skipped_single_record_token_limit")
    );
}
