// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn postgres_summary_cursor_preserves_mixed_sort_and_offset_compatibility() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("summary-cursor-tenant-{suffix}");
    let source = format!("summary-cursor-source-{suffix}");
    let thread_id = format!("summary-cursor-thread-{suffix}");
    let subject_id = format!("summary-cursor-subject-{suffix}");
    let created_at = "2026-09-18T10:00:00Z";
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject_id.clone(),
            thread_type: "chat".to_string(),
            external_thread_id: None,
            title: Some("summary cursor contract".to_string()),
            labels: None,
            metadata: None,
            status: Some("active".to_string()),
            created_at: Some(created_at.to_string()),
            updated_at: Some(created_at.to_string()),
            archived_at: None,
        },
    )
    .await
    .expect("insert summary cursor thread");

    let levels = [2_i64, 2, 1, 1, 0, 0];
    let ids = levels
        .iter()
        .enumerate()
        .map(|(index, _)| format!("summary-cursor-{suffix}-{index}"))
        .collect::<Vec<_>>();
    for (id, level) in ids.iter().zip(levels) {
        let summary = EngineSummary {
            id: id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            thread_id: thread_id.clone(),
            subject_id: subject_id.clone(),
            summary_type: "thread_incremental".to_string(),
            level,
            source_digest: None,
            summary_text: id.clone(),
            source_record_start_id: None,
            source_record_end_id: None,
            source_record_count: 1,
            status: "done".to_string(),
            rollup_status: "pending".to_string(),
            rollup_summary_id: None,
            rolled_up_at: None,
            subject_memory_summarized: 0,
            subject_memory_summarized_at: None,
            metadata: None,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        };
        sqlx::query("INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type,level,status,rollup_status,subject_memory_summarized,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(&summary.id).bind(&summary.tenant_id).bind(&summary.source_id).bind(&summary.thread_id)
            .bind(&summary.subject_id).bind(&summary.summary_type).bind(summary.level).bind(&summary.status)
            .bind(&summary.rollup_status).bind(summary.subject_memory_summarized)
            .bind(timestamp(&summary.created_at).expect("summary cursor created time"))
            .bind(timestamp(&summary.updated_at).expect("summary cursor updated time"))
            .bind(json(&summary).expect("summary cursor json"))
            .execute(&pool).await.expect("insert cursor summary");
    }

    let (first, first_has_more) = crate::repositories::summaries::list_thread_summaries(
        &pool,
        crate::repositories::summaries::ListSummariesQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            limit: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list first summary cursor page");
    assert!(first_has_more);

    let inserted_id = format!("summary-cursor-{suffix}-newer");
    let inserted = EngineSummary {
        id: inserted_id.clone(),
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        thread_id: thread_id.clone(),
        subject_id: subject_id.clone(),
        summary_type: "thread_incremental".to_string(),
        level: 3,
        source_digest: None,
        summary_text: "inserted after first page".to_string(),
        source_record_start_id: None,
        source_record_end_id: None,
        source_record_count: 1,
        status: "done".to_string(),
        rollup_status: "pending".to_string(),
        rollup_summary_id: None,
        rolled_up_at: None,
        subject_memory_summarized: 0,
        subject_memory_summarized_at: None,
        metadata: None,
        created_at: "2026-09-18T10:01:00Z".to_string(),
        updated_at: "2026-09-18T10:01:00Z".to_string(),
    };
    sqlx::query("INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type,level,status,rollup_status,subject_memory_summarized,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(&inserted.id).bind(&inserted.tenant_id).bind(&inserted.source_id).bind(&inserted.thread_id)
        .bind(&inserted.subject_id).bind(&inserted.summary_type).bind(inserted.level).bind(&inserted.status)
        .bind(&inserted.rollup_status).bind(inserted.subject_memory_summarized)
        .bind(timestamp(&inserted.created_at).expect("inserted summary created time"))
        .bind(timestamp(&inserted.updated_at).expect("inserted summary updated time"))
        .bind(json(&inserted).expect("inserted summary json"))
        .execute(&pool).await.expect("insert higher-level summary");

    let first_cursor = first.last().expect("first summary cursor");
    let (second, second_has_more) = crate::repositories::summaries::list_thread_summaries(
        &pool,
        crate::repositories::summaries::ListSummariesQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            after_level: Some(first_cursor.level),
            after_created_at: Some(&first_cursor.created_at),
            after_id: Some(&first_cursor.id),
            limit: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list second summary cursor page");
    assert!(second_has_more);
    assert!(!second.iter().any(|summary| summary.id == inserted_id));
    let second_cursor = second.last().expect("second summary cursor");
    let (third, third_has_more) = crate::repositories::summaries::list_thread_summaries(
        &pool,
        crate::repositories::summaries::ListSummariesQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            after_level: Some(second_cursor.level),
            after_created_at: Some(&second_cursor.created_at),
            after_id: Some(&second_cursor.id),
            limit: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list third summary cursor page");
    assert!(!third_has_more);
    let actual = first
        .iter()
        .chain(second.iter())
        .chain(third.iter())
        .map(|summary| summary.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(actual, ids);

    let (offset_page, _) = crate::repositories::summaries::list_thread_summaries(
        &pool,
        crate::repositories::summaries::ListSummariesQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            limit: 2,
            offset: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list summary offset compatibility page");
    assert_eq!(
        offset_page
            .iter()
            .map(|summary| summary.id.as_str())
            .collect::<Vec<_>>(),
        [ids[1].as_str(), ids[2].as_str()]
    );

    crate::repositories::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("clean up summary cursor thread");
}
