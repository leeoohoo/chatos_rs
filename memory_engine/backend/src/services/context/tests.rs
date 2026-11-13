// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::blocks::{
    build_thread_summary_blocks_from_frontier, build_thread_summary_level0_text,
    format_subject_memory, thread_agent_subject_id, thread_allows_dynamic_project_recall,
};
use super::policy::ResolvedComposeContextPolicy;
use super::retain_newest_records_within_budget;
use super::validate_requested_subject;
use crate::models::{EngineSubjectMemory, EngineSummary, EngineThread};
use serde_json::json;
use uuid::Uuid;

#[test]
fn compose_estimator_does_not_undercount_cjk() {
    assert_eq!(super::estimate_tokens("你好世界"), 4);
}

fn summary(id: &str, level: i64, created_at: &str, text: &str) -> EngineSummary {
    EngineSummary {
        id: id.to_string(),
        tenant_id: "tenant_1".to_string(),
        source_id: "source_1".to_string(),
        thread_id: "thread_1".to_string(),
        subject_id: "subject_1".to_string(),
        summary_type: "thread_incremental".to_string(),
        level,
        source_digest: None,
        summary_text: text.to_string(),
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
        payload_schema: None,
        structured_payload: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    }
}

fn subject_memory(
    subject_id: &str,
    memory_type: &str,
    level: i64,
    key: &str,
    text: &str,
) -> EngineSubjectMemory {
    EngineSubjectMemory {
        id: format!("mem_{key}"),
        tenant_id: "tenant_1".to_string(),
        source_id: "source_1".to_string(),
        subject_id: subject_id.to_string(),
        memory_key: key.to_string(),
        memory_type: memory_type.to_string(),
        text: text.to_string(),
        level,
        source_digest: None,
        confidence: None,
        last_seen_at: None,
        metadata: None,
        payload_schema: None,
        structured_payload: None,
        status: "active".to_string(),
        rollup_status: "pending".to_string(),
        rollup_memory_key: None,
        rolled_up_at: None,
        created_at: "2026-05-12T00:00:00Z".to_string(),
        updated_at: "2026-05-12T00:00:00Z".to_string(),
    }
}

#[test]
fn recent_record_budget_keeps_a_tool_turn_as_one_atomic_unit() {
    let make_record =
        |id: &str, role: &str, content: &str, turn_id: &str| crate::models::EngineRecord {
            id: id.to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            external_record_id: None,
            role: role.to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({"conversation_turn_id": turn_id})),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-09T00:00:00Z".to_string(),
        };
    let records = vec![
        make_record("old", "user", "old", "turn-old"),
        make_record("call", "assistant", "calling tool", "turn-tool"),
        make_record("output", "tool", "tool output", "turn-tool"),
        make_record("answer", "assistant", "finished", "turn-tool"),
    ];
    let tool_turn_budget = records[1..]
        .iter()
        .map(super::estimate_record_tokens)
        .sum::<usize>();
    let retained = retain_newest_records_within_budget(records.clone(), tool_turn_budget);
    assert_eq!(
        retained
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["call", "output", "answer"]
    );
    let too_small = retain_newest_records_within_budget(records, tool_turn_budget - 1);
    assert!(too_small.records.is_empty());
}

fn thread(
    subject_id: &str,
    labels: Option<Vec<&str>>,
    metadata: Option<serde_json::Value>,
) -> EngineThread {
    EngineThread {
        id: "thread_1".to_string(),
        tenant_id: "tenant_1".to_string(),
        source_id: "source_1".to_string(),
        subject_id: subject_id.to_string(),
        thread_type: "chat".to_string(),
        external_thread_id: None,
        title: None,
        labels: labels.map(|items| items.into_iter().map(ToOwned::to_owned).collect()),
        metadata,
        status: "active".to_string(),
        created_at: "2026-05-12T00:00:00Z".to_string(),
        updated_at: "2026-05-12T00:00:00Z".to_string(),
        archived_at: None,
    }
}

#[tokio::test]
async fn compose_context_includes_agent_memory_and_only_the_current_project_context() {
    let Ok(database_url) = std::env::var("MEMORY_ENGINE_DATABASE_URL") else {
        return;
    };
    let Ok(pool) = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
    else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("client-agent:agent:{suffix}");
    let agent_id = format!("agent:{suffix}");
    let project_a = format!("project:a-{suffix}");
    let project_b = format!("project:b-{suffix}");

    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        crate::models::UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_id.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                agent_id.clone(),
                "thread-kind:main_chat".to_string(),
            ]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert stable Agent thread");

    let mut tx = pool.begin().await.expect("begin context records");
    for (index, (content, project)) in [
        ("pending-project-a", Some(project_a.as_str())),
        ("pending-project-b", Some(project_b.as_str())),
        ("pending-main-chat", None),
    ]
    .into_iter()
    .enumerate()
    {
        crate::repositories::records::upsert_record_row(
            &mut tx,
            &crate::models::EngineRecord {
                id: format!("record-{index}-{suffix}"),
                thread_id: thread_id.clone(),
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                external_record_id: None,
                role: "user".to_string(),
                record_type: "message".to_string(),
                content: content.to_string(),
                structured_payload: None,
                metadata: project.map(|value| json!({"memory_project_id": value})),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: format!("2026-11-05T00:00:0{index}Z"),
            },
        )
        .await
        .expect("insert context record");
        sqlx::query(
            "UPDATE engine_records SET project_partition_key=$4 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
        )
        .bind(&tenant)
        .bind(&source)
        .bind(format!("record-{index}-{suffix}"))
        .bind(project.unwrap_or("__none__"))
        .execute(&mut *tx)
        .await
        .expect("set trusted context record partition");
    }
    tx.commit().await.expect("commit context records");

    for (text, project) in [
        ("thread-summary-a", Some(project_a.as_str())),
        ("thread-summary-b", Some(project_b.as_str())),
        ("thread-summary-main", None),
    ] {
        crate::repositories::summaries::insert_test_thread_summary(
            &pool, &tenant, &source, &thread_id, &agent_id, text, None, None, 1, project,
        )
        .await
        .expect("create project-scoped summary");
    }

    for (subject, key, memory_type, text) in [
        (
            agent_id.as_str(),
            "agent-memory",
            "agent_memory",
            "durable-agent-memory",
        ),
        (
            project_a.as_str(),
            "project-a-memory",
            "project_summary",
            "durable-project-a",
        ),
        (
            project_b.as_str(),
            "project-b-memory",
            "project_summary",
            "durable-project-b",
        ),
    ] {
        crate::repositories::subject_memories::insert_test_subject_memory(
            &pool,
            subject,
            key,
            crate::models::UpsertSubjectMemoryRequest {
                id: None,
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                memory_type: memory_type.to_string(),
                text: text.to_string(),
                level: Some(0),
                source_digest: None,
                confidence: None,
                last_seen_at: None,
                metadata: Some(if memory_type == "agent_memory" {
                    json!({
                        "relation_subject_id": subject,
                        "generation_profile": crate::repositories::subject_memory_scopes::AGENT_STABLE_MEMORY_PROFILE,
                    })
                } else {
                    json!({"relation_subject_id": subject})
                }),
                rollup_status: None,
                rollup_memory_key: None,
                rolled_up_at: None,
                status: None,
                created_at: None,
                updated_at: None,
            },
        )
        .await
        .expect("upsert context memory");
    }
    crate::repositories::subject_memories::insert_test_subject_memory(
        &pool,
        agent_id.as_str(),
        "legacy-agent-memory",
        crate::models::UpsertSubjectMemoryRequest {
            id: None,
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            memory_type: "agent_memory".to_string(),
            text: "legacy-project-specific-agent-memory".to_string(),
            level: Some(0),
            source_digest: None,
            confidence: None,
            last_seen_at: None,
            metadata: Some(json!({"relation_subject_id": agent_id})),
            rollup_status: None,
            rollup_memory_key: None,
            rolled_up_at: None,
            status: None,
            created_at: None,
            updated_at: None,
        },
    )
    .await
    .expect("upsert legacy agent memory");

    let compose = |project: Option<&str>| crate::models::ComposeContextRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        subject_id: Some(agent_id.clone()),
        context_project_subject_id: project.map(str::to_string),
        thread_id: thread_id.clone(),
        query_text: Some("project context".to_string()),
        current_turn_id: Some("turn-current".to_string()),
        model_capability_revision: "test-model-capability-v1".to_string(),
        policy: crate::models::ComposeContextPolicy {
            max_memory_tokens: 32_768,
            max_recent_record_tokens: 16_384,
            max_thread_summary_tokens: 8_192,
            max_project_memory_tokens: 8_192,
            max_agent_memory_tokens: 8_192,
        },
        runtime_context_assertion: "service-test".to_string(),
    };

    let project_context = super::compose_context(&pool, compose(Some(project_a.as_str())))
        .await
        .expect("compose project context");
    assert_eq!(
        project_context
            .recent_records
            .iter()
            .map(|record| record.content.as_str())
            .collect::<Vec<_>>(),
        vec!["pending-project-a"]
    );
    let project_blocks = project_context
        .blocks
        .iter()
        .map(|block| format!("{}\n{}", block.block_type, block.text))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(project_blocks.contains("durable-agent-memory"));
    assert!(!project_blocks.contains("legacy-project-specific-agent-memory"));
    assert!(project_blocks.contains("durable-project-a"));
    assert!(project_blocks.contains("thread-summary-a"));
    assert!(!project_blocks.contains("durable-project-b"));
    assert!(!project_blocks.contains("thread-summary-b"));
    assert!(!project_blocks.contains("thread-summary-main"));

    let main_context = super::compose_context(&pool, compose(None))
        .await
        .expect("compose projectless Main Chat context");
    assert_eq!(
        main_context
            .recent_records
            .iter()
            .map(|record| record.content.as_str())
            .collect::<Vec<_>>(),
        vec!["pending-main-chat"]
    );
    let main_blocks = main_context
        .blocks
        .iter()
        .map(|block| format!("{}\n{}", block.block_type, block.text))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(main_blocks.contains("durable-agent-memory"));
    assert!(main_blocks.contains("thread-summary-main"));
    assert!(!main_blocks.contains("durable-project-a"));
    assert!(!main_blocks.contains("durable-project-b"));
    assert!(!main_blocks.contains("thread-summary-a"));
    assert!(!main_blocks.contains("thread-summary-b"));
}

#[test]
fn level0_summaries_are_rendered_in_chronological_order() {
    let rows = vec![
        summary("sum_3", 0, "2026-05-12T03:00:00Z", "third"),
        summary("sum_2", 0, "2026-05-12T02:00:00Z", "second"),
    ];

    assert_eq!(
        build_thread_summary_level0_text(rows.as_slice()),
        "second\n\n---\n\nthird"
    );
}

#[test]
fn thread_summary_blocks_keep_all_maximal_rollups_before_uncovered_level0_summaries() {
    let (blocks, summary_count) = build_thread_summary_blocks_from_frontier(vec![
        summary("sum_top_b", 3, "2026-05-12T05:00:00Z", "parallel top"),
        summary("sum_top_a", 2, "2026-05-12T04:00:00Z", "highest level"),
        summary("sum_3", 0, "2026-05-12T03:00:00Z", "third"),
        summary("sum_2", 0, "2026-05-12T02:00:00Z", "second"),
    ]);

    assert_eq!(summary_count, 4);
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].block_type, "thread_summary_top_level");
    assert_eq!(blocks[0].text, "parallel top");
    assert_eq!(blocks[1].block_type, "thread_summary_top_level");
    assert_eq!(blocks[1].text, "highest level");
    assert_eq!(blocks[2].block_type, "thread_summary_level0");
    assert_eq!(blocks[2].text, "second\n\n---\n\nthird");
}

#[test]
fn subject_memory_block_preserves_identity_fields() {
    let block = format_subject_memory(subject_memory(
        "agent:42",
        "agent_recall",
        2,
        "agent_recall:rollup:l1->2",
        "durable recall",
    ));

    assert!(block.contains("[subject_id=agent:42]"));
    assert!(block.contains("[memory_type=agent_recall]"));
    assert!(block.contains("[level=2]"));
    assert!(block.contains("[memory_key=agent_recall:rollup:l1->2]"));
    assert!(block.ends_with("durable recall"));
}

#[test]
fn thread_agent_subject_id_accepts_only_matching_v2_subject_and_label() {
    let item = thread(
        "agent:42",
        Some(vec!["memory-schema:v2", "project:9", "agent:42"]),
        Some(serde_json::json!({
            "legacy_session_mapping": {
                "agent_id": "999"
            }
        })),
    );

    assert_eq!(thread_agent_subject_id(&item).as_deref(), Some("agent:42"));
}

#[test]
fn thread_agent_subject_id_ignores_legacy_and_metadata_identity() {
    let legacy = thread(
        "session:abc",
        None,
        Some(serde_json::json!({
            "legacy_session_mapping": {
                "agent_id": "42"
            },
            "agent_id": "42"
        })),
    );
    let mismatched_v2 = thread("agent:42", Some(vec!["memory-schema:v2", "agent:99"]), None);

    assert_eq!(thread_agent_subject_id(&legacy), None);
    assert_eq!(thread_agent_subject_id(&mismatched_v2), None);
}

#[test]
fn compose_context_policy_uses_requested_model_capacity() {
    let requested = crate::models::ComposeContextPolicy {
        max_memory_tokens: usize::MAX,
        max_recent_record_tokens: usize::MAX,
        max_thread_summary_tokens: usize::MAX,
        max_project_memory_tokens: usize::MAX,
        max_agent_memory_tokens: usize::MAX,
    };
    let policy = ResolvedComposeContextPolicy::from_request(&requested)
        .expect("resolve explicit compose policy");

    assert_eq!(policy.max_memory_tokens, usize::MAX);
    assert_eq!(policy.max_recent_record_tokens, usize::MAX);
    assert_eq!(policy.max_thread_summary_tokens, usize::MAX);
    assert_eq!(policy.max_project_memory_tokens, usize::MAX);
    assert_eq!(policy.max_agent_memory_tokens, usize::MAX);
}

#[test]
fn compose_rejects_subjects_not_bound_to_the_thread() {
    let bound = vec!["agent:a-1".to_string(), "project:p-1".to_string()];
    assert!(validate_requested_subject(&bound, Some("agent:a-1")).is_ok());
    assert!(validate_requested_subject(&bound, Some("agent:a-2")).is_err());
}

#[test]
fn task_recall_accepts_its_bound_project_and_rejects_another_project() {
    let bound = vec!["agent:a-1".to_string(), "project:p-1".to_string()];
    assert!(super::validate_requested_project(&bound, false, "project:p-1").is_ok());
    assert!(super::validate_requested_project(&bound, false, "project:p-2").is_err());
    let agent_only = vec!["agent:a-1".to_string()];
    assert!(super::validate_requested_project(&agent_only, false, "project:p-1").is_err());
    assert!(super::validate_requested_project(&agent_only, true, "project:p-1").is_ok());
    assert!(super::validate_requested_project(&agent_only, true, "project:").is_err());
    assert!(super::validate_requested_project(&agent_only, true, "p-1").is_err());
}

#[test]
fn stable_agent_thread_allows_only_dynamic_project_recall() {
    let item = thread(
        "agent:a-1",
        Some(vec![
            "memory-schema:v2",
            "agent:a-1",
            "thread-kind:agent_conversation",
        ]),
        None,
    );
    assert!(thread_allows_dynamic_project_recall(&item));
    let bound = vec!["agent:a-1".to_string()];
    assert!(validate_requested_subject(&bound, Some("agent:a-1")).is_ok());
    assert!(validate_requested_subject(&bound, Some("agent:a-2")).is_err());
}
