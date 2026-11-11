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
        let partition = memory_project_id
            .map(|value| {
                if value.starts_with("project:") {
                    value.to_string()
                } else {
                    format!("project:{value}")
                }
            })
            .unwrap_or_else(|| "__none__".to_string());
        sqlx::query(
            "UPDATE engine_records SET project_partition_key=$4 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
        )
        .bind(&tenant)
        .bind(&source)
        .bind(format!("record-{index}-{suffix}"))
        .bind(partition)
        .execute(&mut *tx)
        .await
        .expect("set scoped record partition");
    }
    tx.commit().await.expect("commit context records");

    for (text, project) in [
        ("summary-project-a", Some(project_a.as_str())),
        ("summary-project-b", Some(project_b.as_str())),
        ("summary-main-chat", None),
    ] {
        crate::repositories::summaries::insert_test_thread_summary(
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
