// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_cloud_agent_runtime::{
    create_cloud_agent_run, CloudAgentRunStore, CloudAgentStateRepository, CloudAgentStateStore,
    NewCloudAgentRun,
};
use serde_json::json;
use uuid::Uuid;

use crate::models::{EngineRecord, EngineSummary, UpsertSubjectMemoryRequest, UpsertThreadRequest};
use crate::repositories::postgres::{json, timestamp};

async fn pool() -> Option<crate::db::Db> {
    let url = std::env::var("MEMORY_ENGINE_DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

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
        super::threads::upsert_thread(
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

    let first = super::threads::list_threads(
        &pool,
        super::threads::ListThreadsQuery {
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
    super::threads::upsert_thread(
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
    let second = super::threads::list_threads(
        &pool,
        super::threads::ListThreadsQuery {
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
    let third = super::threads::list_threads(
        &pool,
        super::threads::ListThreadsQuery {
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

    let offset_page = super::threads::list_threads(
        &pool,
        super::threads::ListThreadsQuery {
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
    super::threads::upsert_thread(
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

    let first = super::records::list_records_page(
        &pool,
        super::records::ListRecordsQuery {
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
    let second = super::records::list_records_page(
        &pool,
        super::records::ListRecordsQuery {
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
    let third = super::records::list_records_page(
        &pool,
        super::records::ListRecordsQuery {
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

    let descending_first = super::records::list_records_page(
        &pool,
        super::records::ListRecordsQuery {
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
    let descending_second = super::records::list_records_page(
        &pool,
        super::records::ListRecordsQuery {
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

    let offset_page = super::records::list_records_page(
        &pool,
        super::records::ListRecordsQuery {
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

    super::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("clean up record cursor thread");
}

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
    super::threads::upsert_thread(
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

    let (first, first_has_more) = super::summaries::list_thread_summaries(
        &pool,
        super::summaries::ListSummariesQuery {
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
    let (second, second_has_more) = super::summaries::list_thread_summaries(
        &pool,
        super::summaries::ListSummariesQuery {
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
    let (third, third_has_more) = super::summaries::list_thread_summaries(
        &pool,
        super::summaries::ListSummariesQuery {
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

    let (offset_page, _) = super::summaries::list_thread_summaries(
        &pool,
        super::summaries::ListSummariesQuery {
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

    super::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("clean up summary cursor thread");
}

#[tokio::test]
async fn postgres_repositories_round_trip_memory_and_coordination_state() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject_id = format!("subject-{suffix}");

    super::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject_id.clone(),
            thread_type: "chat".to_string(),
            external_thread_id: Some(format!("ext-{suffix}")),
            title: Some("contract".to_string()),
            labels: Some(vec!["contract".to_string()]),
            metadata: Some(json!({"mapping_source":"contract"})),
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert thread");
    let listed =
        super::threads::list_threads_by_label(&pool, &tenant, &source, "contract", None, 10, 0)
            .await
            .expect("list threads");
    assert_eq!(listed.len(), 1);

    assert!(super::threads::try_acquire_summary_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "summary-job",
    )
    .await
    .expect("claim summary slot")
    .is_some());
    assert!(!super::threads::refresh_summary_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "wrong-job",
        300,
    )
    .await
    .expect("reject wrong summary owner"));
    super::threads::release_summary_slot(&pool, &tenant, &source, &thread_id, "summary-job", 0, 0)
        .await
        .expect("release summary slot")
        .expect("owned slot");

    let record = EngineRecord {
        id: format!("record-{suffix}"),
        thread_id: thread_id.clone(),
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: "hello".to_string(),
        structured_payload: None,
        metadata: Some(json!({"conversation_turn_id":"turn-1"})),
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: crate::models::now_rfc3339(),
    };
    sqlx::query("INSERT INTO engine_records(id,thread_id,tenant_id,source_id,role,record_type,summary_status,created_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&record.id).bind(&record.thread_id).bind(&record.tenant_id).bind(&record.source_id)
        .bind(&record.role).bind(&record.record_type).bind(&record.summary_status)
        .bind(timestamp(&record.created_at).expect("record time")).bind(json(&record).expect("record json"))
        .execute(&pool).await.expect("insert record");
    assert_eq!(
        super::records::claim_records_for_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&record.id),
            "record-job",
        )
        .await
        .expect("claim records"),
        1
    );

    let summary = super::summaries::create_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject_id,
        "summary",
        None,
        None,
        0,
    )
    .await
    .expect("create summary");
    assert!(
        super::summaries::get_pending_rollup_dispatch(&pool, &tenant, &source, &summary.id)
            .await
            .expect("rollup outbox")
            .is_some()
    );
    assert_eq!(
        super::records::mark_claimed_records_summarized(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&record.id),
            "record-job",
            &summary.id,
        )
        .await
        .expect("summarize records"),
        1
    );
    let rollup_event =
        super::summaries::get_pending_rollup_dispatch(&pool, &tenant, &source, &summary.id)
            .await
            .expect("rollup event")
            .expect("pending rollup event");
    assert!(
        super::summaries::mark_rollup_dispatch_published(&pool, &rollup_event)
            .await
            .expect("publish rollup")
    );
    assert!(
        super::summaries::mark_rollup_dispatch_consumed(&pool, &rollup_event)
            .await
            .expect("consume rollup")
    );
    assert!(super::summaries::rearm_rollup_dispatch_if_eligible(
        &pool, &tenant, &source, &thread_id, 8,
    )
    .await
    .expect("rearm rollup")
    .is_some());

    let memory = super::subject_memories::upsert_subject_memory(
        &pool,
        &subject_id,
        "profile:name",
        UpsertSubjectMemoryRequest {
            id: None,
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            memory_type: "profile".to_string(),
            text: "Alice".to_string(),
            level: Some(0),
            source_digest: None,
            confidence: Some(1.0),
            last_seen_at: None,
            metadata: Some(json!({"relation_subject_id":"agent-1"})),
            rollup_status: None,
            rollup_memory_key: None,
            rolled_up_at: None,
            status: None,
            created_at: None,
            updated_at: None,
        },
    )
    .await
    .expect("upsert subject memory");
    assert_eq!(memory.text, "Alice");
    assert_eq!(
        super::subject_memories::list_subject_memories(
            &pool,
            &tenant,
            &source,
            &subject_id,
            None,
            None,
            10,
            0
        )
        .await
        .expect("list memories")
        .len(),
        1
    );
    assert_eq!(
        super::subject_memories::mark_subject_memories_rolled_up(
            &pool,
            &tenant,
            &source,
            &subject_id,
            std::slice::from_ref(&memory.id),
            "profile:rollup",
        )
        .await
        .expect("roll up memories"),
        1
    );
    assert_eq!(
        super::summaries::mark_summaries_subject_memory_summarized_for_scope(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&summary.id),
            "scope-1",
        )
        .await
        .expect("mark summary scope"),
        1
    );

    let cloud_agent_repository =
        super::cloud_agent::CloudAgentPostgresStore::new(pool.clone());
    let store = CloudAgentStateStore::from_repository(cloud_agent_repository.clone());
    let run_id = format!("memory-run-{suffix}");
    let run = create_cloud_agent_run(
        &store,
        NewCloudAgentRun {
            ordering_lane_key: format!("memory-lane-{suffix}"),
            agent_run_id: run_id.clone(),
            owner_service: "memory-engine".to_string(),
            owner_entity_type: "summary".to_string(),
            owner_entity_id: summary.id.clone(),
            owner_user_id: tenant.clone(),
            agent_key: "summary".to_string(),
            input: json!({"conversation":"stored only in Memory Engine"}),
            model_config_ref: "model".to_string(),
            model_runtime_snapshot_ref: "model".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "checksum".to_string(),
            capability_policy_revision: "none".to_string(),
            mcp_runtime_session_ref: None,
            current_input_items_ref: "input".to_string(),
            max_iterations: 4,
            deadline_at: None,
            runtime_routing_key: "memory.contract".to_string(),
            start_causation_id: summary.id.clone(),
            start_payload: json!({"summary_id":summary.id}),
        },
    )
    .await
    .expect("create cloud run");
    assert_eq!(
        store
            .load_run(&run_id)
            .await
            .expect("load run")
            .expect("run")
            .input,
        run.input
    );

    let outbox_event_id = format!("cloud_agent_run_started_{run_id}_1_1");
    let publisher_a = format!("publisher-a:{suffix}");
    let publisher_b = format!("publisher-b:{suffix}");
    let claim_until = chrono::Utc::now() + chrono::Duration::seconds(30);
    let (claimed_a, claimed_b) = tokio::join!(
        cloud_agent_repository.claim_ready_outbox_with_attempts(10, &publisher_a, claim_until),
        cloud_agent_repository.claim_ready_outbox_with_attempts(10, &publisher_b, claim_until),
    );
    let claimed_a = claimed_a.expect("publisher A claim");
    let claimed_b = claimed_b.expect("publisher B claim");
    assert_eq!(claimed_a.len() + claimed_b.len(), 1);
    let winning_token = if claimed_a.is_empty() {
        publisher_b
    } else {
        publisher_a
    };
    assert!(!cloud_agent_repository
        .mark_claimed_outbox_published(&outbox_event_id, "publisher-wrong")
        .await
        .expect("wrong publisher token"));
    assert!(cloud_agent_repository
        .claim_ready_outbox_with_attempts(
            10,
            "publisher-blocked",
            chrono::Utc::now() + chrono::Duration::seconds(30),
        )
        .await
        .expect("active claim blocks a second publisher")
        .is_empty());

    sqlx::query(
        "UPDATE cloud_agent_outbox SET claim_until=now()-interval '1 second' WHERE event_id=$1",
    )
    .bind(&outbox_event_id)
    .execute(&pool)
    .await
    .expect("expire publisher lease");
    let recovery_token = format!("publisher-recovery:{suffix}");
    assert_eq!(
        cloud_agent_repository
            .claim_ready_outbox_with_attempts(
                10,
                &recovery_token,
                chrono::Utc::now() + chrono::Duration::seconds(30),
            )
            .await
            .expect("recover expired publisher lease")
            .len(),
        1
    );
    assert!(!cloud_agent_repository
        .mark_claimed_outbox_published(&outbox_event_id, &winning_token)
        .await
        .expect("stale publisher token"));
    assert!(cloud_agent_repository
        .mark_claimed_outbox_published(&outbox_event_id, &recovery_token)
        .await
        .expect("publish outbox"));

    let coordination_has_data: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='cloud_agent_runs' AND column_name='data')",
    ).fetch_one(&pool).await.expect("inspect coordination schema");
    assert!(!coordination_has_data);

    super::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("cleanup thread");
}
