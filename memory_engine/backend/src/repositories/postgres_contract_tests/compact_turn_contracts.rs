// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::EngineCompactTurn;

#[tokio::test]
async fn compact_turn_pages_are_isolated_by_conversation() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: format!("subject-{suffix}"),
            thread_type: "main_chat".to_string(),
            external_thread_id: None,
            title: Some("compact turn conversation filter".to_string()),
            labels: None,
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert thread");

    for (index, conversation_id) in ["conversation-a", "conversation-b"].into_iter().enumerate() {
        let turn_id = format!("turn-{index}-{suffix}");
        let user_record_id = format!("user-{index}-{suffix}");
        let created_at = format!("2026-11-07T00:00:0{index}Z");
        let user_record = EngineRecord {
            id: user_record_id.clone(),
            thread_id: thread_id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: format!("request {index}"),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_id": conversation_id,
                "conversation_turn_id": turn_id,
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: created_at.clone(),
        };
        let compact = EngineCompactTurn {
            id: format!("compact-{index}-{suffix}"),
            thread_id: thread_id.clone(),
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            record_type: "message".to_string(),
            turn_id,
            user_record_id: user_record_id.clone(),
            user_created_at: created_at.clone(),
            user_record,
            final_assistant_record: None,
            has_process: false,
            tool_call_count: 0,
            thinking_count: 0,
            process_message_count: 0,
            updated_at: created_at.clone(),
        };
        sqlx::query(
            "INSERT INTO engine_compact_turns(\
             id,thread_id,tenant_id,source_id,record_type,turn_id,user_record_id,\
             user_created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(&compact.id)
        .bind(&compact.thread_id)
        .bind(&compact.tenant_id)
        .bind(&compact.source_id)
        .bind(&compact.record_type)
        .bind(&compact.turn_id)
        .bind(&compact.user_record_id)
        .bind(timestamp(&compact.user_created_at).expect("user time"))
        .bind(timestamp(&compact.updated_at).expect("updated time"))
        .bind(json(&compact).expect("compact turn json"))
        .execute(&pool)
        .await
        .expect("insert compact turn");
    }

    let (items, has_more, next_before) = crate::repositories::records::list_compact_turn_slices(
        &pool,
        &thread_id,
        Some(&tenant),
        Some(&source),
        Some("conversation-a"),
        Some("message"),
        10,
        None,
    )
    .await
    .expect("list conversation compact turns");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].user_record.content, "request 0");
    assert!(!has_more);
    assert_eq!(next_before, None);

    sqlx::query("DELETE FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3")
        .bind(&tenant)
        .bind(&source)
        .bind(&thread_id)
        .execute(&pool)
        .await
        .expect("delete fixture thread");
}
