// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn postgres_schema_uses_tenant_scoped_resource_keys() {
    let Some(pool) = pool().await else {
        return;
    };
    let constraints = sqlx::query_as::<_, (String, String, String)>(
        "SELECT conrelid::regclass::text,contype::text,pg_get_constraintdef(oid) \
         FROM pg_constraint WHERE conrelid::regclass::text=ANY($1) AND contype IN ('p','f')",
    )
    .bind([
        "engine_subjects",
        "engine_subject_memory_scopes",
        "engine_subject_memories",
        "engine_threads",
        "engine_records",
        "engine_compact_turns",
        "engine_summaries",
        "engine_thread_snapshots",
    ])
    .fetch_all(&pool)
    .await
    .expect("load tenant resource constraints");
    let primary_keys = constraints
        .iter()
        .filter(|(_, kind, _)| kind == "p")
        .map(|(table, _, definition)| (table.as_str(), definition.as_str()))
        .collect::<HashSet<_>>();
    for table in [
        "engine_subjects",
        "engine_subject_memory_scopes",
        "engine_subject_memories",
        "engine_threads",
        "engine_records",
        "engine_compact_turns",
        "engine_summaries",
        "engine_thread_snapshots",
    ] {
        assert!(
            primary_keys.contains(&(table, "PRIMARY KEY (tenant_id, source_id, id)")),
            "{table} must use a tenant-scoped primary key"
        );
    }
    let foreign_keys = constraints
        .iter()
        .filter(|(_, kind, _)| kind == "f")
        .map(|(table, _, definition)| (table.as_str(), definition.as_str()))
        .collect::<HashSet<_>>();
    let expected = "FOREIGN KEY (tenant_id, source_id, thread_id) REFERENCES engine_threads(tenant_id, source_id, id) ON DELETE CASCADE";
    for table in [
        "engine_records",
        "engine_compact_turns",
        "engine_summaries",
        "engine_thread_snapshots",
    ] {
        assert!(
            foreign_keys.contains(&(table, expected)),
            "{table} must reference a thread inside the same tenant/source scope"
        );
    }
}
#[tokio::test]
async fn postgres_allows_tenant_scoped_ids_and_keeps_mutations_isolated() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant_a = format!("scope-owner-a-{suffix}");
    let tenant_b = format!("scope-owner-b-{suffix}");
    let source_a = format!("scope-source-a-{suffix}");
    let source_b = source_a.clone();
    let thread_a = format!("scope-thread-shared-{suffix}");
    let thread_b = thread_a.clone();
    let subject_a = format!("scope-subject-a-{suffix}");
    let subject_b = format!("scope-subject-b-{suffix}");
    let timestamp = "2026-10-04T08:00:00Z".to_string();

    let make_thread =
        |tenant: &str, source: &str, subject: &str, title: &str| UpsertThreadRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            subject_id: subject.to_string(),
            thread_type: "chat".to_string(),
            external_thread_id: None,
            title: Some(title.to_string()),
            labels: None,
            metadata: None,
            status: Some("active".to_string()),
            created_at: Some(timestamp.clone()),
            updated_at: Some(timestamp.clone()),
            archived_at: None,
        };
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_a,
        make_thread(&tenant_a, &source_a, &subject_a, "owner title"),
    )
    .await
    .expect("insert owner thread");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_a,
        make_thread(&tenant_b, &source_b, &subject_b, "attacker title"),
    )
    .await
    .expect("same thread id is valid in another tenant");
    let owner_thread = crate::repositories::threads::get_thread_by_id(&pool, &tenant_a, &source_a, &thread_a)
        .await
        .expect("load owner thread")
        .expect("owner thread still exists");
    assert_eq!(owner_thread.title.as_deref(), Some("owner title"));
    let tenant_b_thread = crate::repositories::threads::get_thread_by_id(&pool, &tenant_b, &source_b, &thread_b)
        .await
        .expect("load tenant B thread")
        .expect("tenant B thread still exists");
    assert_eq!(tenant_b_thread.title.as_deref(), Some("attacker title"));

    let record_id = format!("scope-record-shared-{suffix}");
    let owner_record = EngineRecord {
        id: record_id.clone(),
        thread_id: thread_a.clone(),
        tenant_id: tenant_a.clone(),
        source_id: source_a.clone(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: "owner record".to_string(),
        structured_payload: None,
        metadata: None,
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: timestamp.clone(),
    };
    let mut tx = pool.begin().await.expect("begin owner record transaction");
    crate::repositories::records::upsert_record_row(&mut tx, &owner_record)
        .await
        .expect("insert owner record");
    tx.commit().await.expect("commit owner record");

    let attacker_record = EngineRecord {
        thread_id: thread_b.clone(),
        tenant_id: tenant_b.clone(),
        source_id: source_b.clone(),
        content: "attacker record".to_string(),
        ..owner_record.clone()
    };
    let mut tx = pool
        .begin()
        .await
        .expect("begin tenant B record transaction");
    crate::repositories::records::upsert_record_row(&mut tx, &attacker_record)
        .await
        .expect("same record id is valid in another tenant");
    tx.commit().await.expect("commit tenant B record");
    let stored_record =
        crate::repositories::records::get_record_by_id(&pool, &record_id, &tenant_a, &source_a, Some(&thread_a))
            .await
            .expect("load owner record")
            .expect("owner record still exists");
    assert_eq!(stored_record.content, "owner record");
    let tenant_b_record =
        crate::repositories::records::get_record_by_id(&pool, &record_id, &tenant_b, &source_b, Some(&thread_b))
            .await
            .expect("load tenant B record")
            .expect("tenant B record still exists");
    assert_eq!(tenant_b_record.content, "attacker record");

    let summary_id = format!("scope-summary-shared-{suffix}");
    let make_summary =
        |tenant: &str, source: &str, subject: &str, text: &str| UpsertThreadSummaryRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            subject_id: subject.to_string(),
            summary_type: "thread_incremental".to_string(),
            level: Some(0),
            source_digest: None,
            summary_text: text.to_string(),
            source_record_start_id: None,
            source_record_end_id: None,
            source_record_count: Some(1),
            status: Some("done".to_string()),
            rollup_status: Some("pending".to_string()),
            rollup_summary_id: None,
            rolled_up_at: None,
            subject_memory_summarized: Some(0),
            subject_memory_summarized_at: None,
            metadata: None,
            created_at: Some(timestamp.clone()),
            updated_at: Some(timestamp.clone()),
        };
    crate::repositories::summaries::upsert_thread_summary(
        &pool,
        &thread_a,
        &summary_id,
        make_summary(&tenant_a, &source_a, &subject_a, "owner summary"),
    )
    .await
    .expect("insert owner summary");
    crate::repositories::summaries::upsert_thread_summary(
        &pool,
        &thread_b,
        &summary_id,
        make_summary(&tenant_b, &source_b, &subject_b, "attacker summary"),
    )
    .await
    .expect("same summary id is valid in another tenant");
    let stored_summary = sqlx::query_scalar::<_, sqlx::types::Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND id=$4",
    )
    .bind(&tenant_a)
    .bind(&source_a)
    .bind(&thread_a)
    .bind(&summary_id)
    .fetch_one(&pool)
    .await
    .expect("load owner summary");
    let stored_summary: EngineSummary =
        crate::repositories::postgres::decode(stored_summary).expect("decode owner summary");
    assert_eq!(stored_summary.summary_text, "owner summary");

    crate::repositories::records::mark_records_summarized(
        &pool,
        &tenant_a,
        &source_a,
        &thread_a,
        std::slice::from_ref(&record_id),
        &summary_id,
    )
    .await
    .expect("update tenant A record only");
    let tenant_b_record =
        crate::repositories::records::get_record_by_id(&pool, &record_id, &tenant_b, &source_b, Some(&thread_b))
            .await
            .expect("reload tenant B record")
            .expect("tenant B record remains");
    assert_eq!(tenant_b_record.summary_status, "pending");

    crate::repositories::summaries::mark_summaries_subject_memory_summarized(
        &pool,
        &tenant_a,
        &source_a,
        &thread_a,
        std::slice::from_ref(&summary_id),
    )
    .await
    .expect("update tenant A summary only");
    let tenant_b_summary = sqlx::query_scalar::<_, sqlx::types::Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND id=$4",
    )
    .bind(&tenant_b)
    .bind(&source_b)
    .bind(&thread_b)
    .bind(&summary_id)
    .fetch_one(&pool)
    .await
    .expect("load tenant B summary");
    let tenant_b_summary: EngineSummary =
        crate::repositories::postgres::decode(tenant_b_summary).expect("decode tenant B summary");
    assert_eq!(tenant_b_summary.subject_memory_summarized, 0);

    crate::repositories::threads::delete_thread(&pool, &tenant_a, &source_a, &thread_a)
        .await
        .expect("cleanup owner thread");
    assert!(
        crate::repositories::threads::get_thread_by_id(&pool, &tenant_b, &source_b, &thread_b)
            .await
            .expect("load tenant B after tenant A delete")
            .is_some()
    );
    assert!(crate::repositories::records::get_record_by_id(
        &pool,
        &record_id,
        &tenant_b,
        &source_b,
        Some(&thread_b),
    )
    .await
    .expect("load tenant B record after tenant A cascade")
    .is_some());
    crate::repositories::threads::delete_thread(&pool, &tenant_b, &source_b, &thread_b)
        .await
        .expect("cleanup tenant B thread");
}
