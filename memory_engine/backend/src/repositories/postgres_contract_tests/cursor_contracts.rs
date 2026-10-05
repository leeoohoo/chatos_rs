// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn postgres_thread_cursor_preserves_stable_pages_and_offset_compatibility() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("cursor-tenant-{suffix}");
    let source = format!("cursor-source-{suffix}");
    let timestamp = "2026-09-18T08:00:00Z";
    let ids = (0..6)
        .map(|index| format!("cursor-thread-{suffix}-{index}"))
        .collect::<Vec<_>>();

    for id in &ids {
        crate::repositories::threads::upsert_thread(
            &pool,
            id,
            UpsertThreadRequest {
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                subject_id: format!("subject-{suffix}"),
                thread_type: "chat".to_string(),
                external_thread_id: None,
                title: Some(id.clone()),
                labels: None,
                metadata: None,
                status: Some("active".to_string()),
                created_at: Some(timestamp.to_string()),
                updated_at: Some(timestamp.to_string()),
                archived_at: None,
            },
        )
        .await
        .expect("insert cursor thread");
    }

    let first = crate::repositories::threads::list_threads(
        &pool,
        crate::repositories::threads::ListThreadsQuery {
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            status: Some("active"),
            limit: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list first cursor page");
    assert_eq!(first.len(), 2);

    let inserted_later_id = format!("cursor-thread-{suffix}-later");
    crate::repositories::threads::upsert_thread(
        &pool,
        &inserted_later_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: format!("subject-{suffix}"),
            thread_type: "chat".to_string(),
            external_thread_id: None,
            title: Some(inserted_later_id.clone()),
            labels: None,
            metadata: None,
            status: Some("active".to_string()),
            created_at: Some("2026-09-18T08:01:00Z".to_string()),
            updated_at: Some("2026-09-18T08:01:00Z".to_string()),
            archived_at: None,
        },
    )
    .await
    .expect("insert newer thread after first page");

    let first_cursor = first.last().expect("first cursor row");
    let second = crate::repositories::threads::list_threads(
        &pool,
        crate::repositories::threads::ListThreadsQuery {
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            status: Some("active"),
            before_updated_at: Some(&first_cursor.updated_at),
            before_created_at: Some(&first_cursor.created_at),
            before_id: Some(&first_cursor.id),
            limit: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list second cursor page");
    assert!(!second.iter().any(|thread| thread.id == inserted_later_id));

    let second_cursor = second.last().expect("second cursor row");
    let third = crate::repositories::threads::list_threads(
        &pool,
        crate::repositories::threads::ListThreadsQuery {
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            status: Some("active"),
            before_updated_at: Some(&second_cursor.updated_at),
            before_created_at: Some(&second_cursor.created_at),
            before_id: Some(&second_cursor.id),
            limit: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list third cursor page");

    let mut expected = ids.clone();
    expected.sort_by(|left, right| right.cmp(left));
    let actual = first
        .iter()
        .chain(second.iter())
        .chain(third.iter())
        .map(|thread| thread.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);

    let offset_page = crate::repositories::threads::list_threads(
        &pool,
        crate::repositories::threads::ListThreadsQuery {
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            status: Some("active"),
            limit: 2,
            offset: 2,
            ..Default::default()
        },
    )
    .await
    .expect("list offset compatibility page");
    assert_eq!(
        offset_page
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        expected[1..3]
    );

    sqlx::query("DELETE FROM engine_threads WHERE tenant_id=$1 AND source_id=$2")
        .bind(&tenant)
        .bind(&source)
        .execute(&pool)
        .await
        .expect("clean up cursor threads");
}
#[tokio::test]
async fn postgres_record_cursor_preserves_both_orders_and_offset_compatibility() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("record-cursor-tenant-{suffix}");
    let source = format!("record-cursor-source-{suffix}");
    let thread_id = format!("record-cursor-thread-{suffix}");
    let created_at = "2026-09-18T09:00:00Z";
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: format!("record-cursor-subject-{suffix}"),
            thread_type: "chat".to_string(),
            external_thread_id: None,
            title: Some("record cursor contract".to_string()),
            labels: None,
            metadata: None,
            status: Some("active".to_string()),
            created_at: Some(created_at.to_string()),
            updated_at: Some(created_at.to_string()),
            archived_at: None,
        },
    )
    .await
    .expect("insert record cursor thread");

    let ids = (0..6)
        .map(|index| format!("record-cursor-{suffix}-{index}"))
        .collect::<Vec<_>>();
    for id in &ids {
        let record = EngineRecord {
            id: id.clone(),
            thread_id: thread_id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: id.clone(),
            structured_payload: None,
            metadata: None,
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: created_at.to_string(),
        };
        sqlx::query("INSERT INTO engine_records(id,thread_id,tenant_id,source_id,role,record_type,summary_status,created_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&record.id).bind(&record.thread_id).bind(&record.tenant_id).bind(&record.source_id)
            .bind(&record.role).bind(&record.record_type).bind(&record.summary_status)
            .bind(timestamp(&record.created_at).expect("record cursor time"))
            .bind(json(&record).expect("record cursor json"))
            .execute(&pool).await.expect("insert cursor record");
    }

    let first = crate::repositories::records::list_records_page(
        &pool,
        crate::repositories::records::ListRecordsQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            limit: 2,
            asc: true,
            ..Default::default()
        },
    )
    .await
    .expect("list first ascending record page");
    assert_eq!(first.total, 6);
    assert!(first.has_more);

    let earlier_id = format!("record-cursor-{suffix}-earlier");
    let earlier = EngineRecord {
        id: earlier_id.clone(),
        thread_id: thread_id.clone(),
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: "inserted after first page".to_string(),
        structured_payload: None,
        metadata: None,
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: "2026-09-18T08:59:00Z".to_string(),
    };
    sqlx::query("INSERT INTO engine_records(id,thread_id,tenant_id,source_id,role,record_type,summary_status,created_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&earlier.id).bind(&earlier.thread_id).bind(&earlier.tenant_id).bind(&earlier.source_id)
        .bind(&earlier.role).bind(&earlier.record_type).bind(&earlier.summary_status)
        .bind(timestamp(&earlier.created_at).expect("earlier record time"))
        .bind(json(&earlier).expect("earlier record json"))
        .execute(&pool).await.expect("insert earlier cursor record");

    let first_cursor = first.items.last().expect("first record cursor");
    let second = crate::repositories::records::list_records_page(
        &pool,
        crate::repositories::records::ListRecordsQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            after_created_at: Some(&first_cursor.created_at),
            after_id: Some(&first_cursor.id),
            limit: 2,
            asc: true,
            ..Default::default()
        },
    )
    .await
    .expect("list second ascending record page");
    assert!(!second.items.iter().any(|record| record.id == earlier_id));
    let second_cursor = second.items.last().expect("second record cursor");
    let third = crate::repositories::records::list_records_page(
        &pool,
        crate::repositories::records::ListRecordsQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            after_created_at: Some(&second_cursor.created_at),
            after_id: Some(&second_cursor.id),
            limit: 2,
            asc: true,
            ..Default::default()
        },
    )
    .await
    .expect("list third ascending record page");
    let ascending = first
        .items
        .iter()
        .chain(second.items.iter())
        .chain(third.items.iter())
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ascending, ids);

    let descending_first = crate::repositories::records::list_records_page(
        &pool,
        crate::repositories::records::ListRecordsQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            limit: 2,
            asc: false,
            ..Default::default()
        },
    )
    .await
    .expect("list first descending record page");
    let descending_cursor = descending_first.items.last().expect("descending cursor");
    let descending_second = crate::repositories::records::list_records_page(
        &pool,
        crate::repositories::records::ListRecordsQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            after_created_at: Some(&descending_cursor.created_at),
            after_id: Some(&descending_cursor.id),
            limit: 2,
            asc: false,
            ..Default::default()
        },
    )
    .await
    .expect("list second descending record page");
    assert_eq!(
        descending_second
            .items
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        [&ids[3], &ids[2]]
    );

    let offset_page = crate::repositories::records::list_records_page(
        &pool,
        crate::repositories::records::ListRecordsQuery {
            thread_id: &thread_id,
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            limit: 2,
            offset: 2,
            asc: true,
            ..Default::default()
        },
    )
    .await
    .expect("list record offset compatibility page");
    assert_eq!(
        offset_page
            .items
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        [&ids[1], &ids[2]]
    );

    crate::repositories::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("clean up record cursor thread");
}
