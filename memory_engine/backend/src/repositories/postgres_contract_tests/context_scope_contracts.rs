// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn stable_agent_context_is_isolated_by_current_project() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("client-agent:agent:{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    let project_a = format!("project:a-{suffix}");
    let project_b = format!("project:b-{suffix}");

    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                agent_subject_id.clone(),
                "thread-kind:agent_conversation".to_string(),
            ]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert stable agent thread");

    let records = [
        ("project-a", Some(project_a.trim_start_matches("project:"))),
        ("project-b", Some(project_b.as_str())),
        ("main-chat", None),
    ];
    let mut tx = pool.begin().await.expect("begin record insert");
    for (index, (content, memory_project_id)) in records.into_iter().enumerate() {
        let metadata = memory_project_id.map(|project_id| {
            json!({
                "memory_project_id": project_id,
                "conversation_turn_id": format!("turn-{index}"),
            })
        });
        crate::repositories::records::upsert_record_row(
            &mut tx,
            &EngineRecord {
                id: format!("record-{index}-{suffix}"),
                thread_id: thread_id.clone(),
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                external_record_id: None,
                role: "user".to_string(),
                record_type: "message".to_string(),
                content: content.to_string(),
                structured_payload: None,
                metadata,
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: format!("2026-11-05T00:00:0{index}Z"),
            },
        )
        .await
        .expect("insert scoped context record");
    }
    tx.commit().await.expect("commit context records");

    for (text, project) in [
        ("summary-project-a", Some(project_a.as_str())),
        ("summary-project-b", Some(project_b.as_str())),
        ("summary-main-chat", None),
    ] {
        crate::repositories::summaries::create_thread_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            &agent_subject_id,
            text,
            None,
            None,
            1,
            project,
            false,
        )
        .await
        .expect("create scoped level-0 summary");
    }

    for (project, expected_record, expected_summary) in [
        (Some(project_a.as_str()), "project-a", "summary-project-a"),
        (Some(project_b.as_str()), "project-b", "summary-project-b"),
        (None, "main-chat", "summary-main-chat"),
    ] {
        let scoped_records = crate::repositories::records::list_context_records_for_project(
            &pool, &tenant, &source, &thread_id, project, None,
        )
        .await
        .expect("list project-scoped context records");
        assert_eq!(
            scoped_records
                .iter()
                .map(|record| record.content.as_str())
                .collect::<Vec<_>>(),
            vec![expected_record]
        );

        let scoped_summaries =
            crate::repositories::summaries::list_latest_thread_summaries_at_level_for_project(
                &pool,
                &tenant,
                &source,
                &thread_id,
                "thread_incremental",
                0,
                project,
                10,
            )
            .await
            .expect("list project-scoped level-0 summaries");
        assert_eq!(
            scoped_summaries
                .iter()
                .map(|summary| summary.summary_text.as_str())
                .collect::<Vec<_>>(),
            vec![expected_summary]
        );
    }
}

#[tokio::test]
async fn rollup_owner_can_refresh_its_own_slot_but_other_jobs_cannot_take_it() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "task".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), agent_subject_id]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert rollup thread");

    let first = crate::repositories::threads::try_acquire_rollup_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "rollup-job-1",
        300,
    )
    .await
    .expect("acquire rollup slot");
    assert!(first.is_some());
    let refresh = crate::repositories::threads::try_acquire_rollup_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "rollup-job-1",
        300,
    )
    .await
    .expect("refresh owned rollup slot");
    assert!(refresh.is_some());
    let other = crate::repositories::threads::try_acquire_rollup_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "rollup-job-2",
        300,
    )
    .await
    .expect("try competing rollup slot");
    assert!(other.is_none());
}
