// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::blocks::{
    build_thread_summary_level0_text, format_subject_memory, subject_ids_for_context,
    subject_memory_subject_ids_for_context, thread_agent_subject_id,
};
use super::policy::ResolvedComposeContextPolicy;
use crate::models::{EngineSubjectMemory, EngineSummary, EngineThread};

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
        status: "active".to_string(),
        rollup_status: "pending".to_string(),
        rollup_memory_key: None,
        rolled_up_at: None,
        created_at: "2026-05-12T00:00:00Z".to_string(),
        updated_at: "2026-05-12T00:00:00Z".to_string(),
    }
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
        summary_status: "idle".to_string(),
        summary_job_run_id: None,
        summary_locked_at: None,
        summary_lock_expires_at: None,
        pending_record_count: 0,
        pending_summary_tokens: 0,
        created_at: "2026-05-12T00:00:00Z".to_string(),
        updated_at: "2026-05-12T00:00:00Z".to_string(),
        archived_at: None,
    }
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
fn collect_subject_ids_keeps_primary_and_deduplicates_related_items() {
    let subject_ids = subject_ids_for_context(
        "session:abc",
        Some(&vec![
            "contact:1".to_string(),
            "contact:1".to_string(),
            "project:9".to_string(),
            "".to_string(),
        ]),
    );

    assert_eq!(
        subject_ids,
        vec![
            "session:abc".to_string(),
            "contact:1".to_string(),
            "project:9".to_string(),
        ]
    );
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
fn thread_agent_subject_id_prefers_agent_scope_over_session_subject() {
    let item = thread(
        "session:abc",
        Some(vec!["project:9", "agent:42"]),
        Some(serde_json::json!({
            "legacy_session_mapping": {
                "agent_id": "999"
            }
        })),
    );

    assert_eq!(thread_agent_subject_id(&item).as_deref(), Some("agent:42"));
}

#[test]
fn thread_agent_subject_id_falls_back_to_legacy_mapping() {
    let item = thread(
        "session:abc",
        None,
        Some(serde_json::json!({
            "legacy_session_mapping": {
                "agent_id": "42"
            }
        })),
    );

    assert_eq!(thread_agent_subject_id(&item).as_deref(), Some("agent:42"));
}

#[test]
fn subject_memory_lookup_prefers_agent_scope_over_session_subject() {
    let item = thread("session:abc", Some(vec!["agent:42"]), None);
    let subject_ids = subject_memory_subject_ids_for_context(&item, Some("session:abc"), None);

    assert_eq!(subject_ids, vec!["agent:42".to_string()]);
}

#[test]
fn subject_memory_lookup_preserves_explicit_non_session_related_subjects() {
    let item = thread("session:abc", Some(vec!["agent:42"]), None);
    let subject_ids = subject_memory_subject_ids_for_context(
        &item,
        Some("contact:9"),
        Some(&vec!["project:1".to_string(), "session:abc".to_string()]),
    );

    assert_eq!(subject_ids.first().map(String::as_str), Some("agent:42"));
    assert!(subject_ids.iter().any(|item| item == "contact:9"));
    assert!(subject_ids.iter().any(|item| item == "project:1"));
    assert!(!subject_ids.iter().any(|item| item == "session:abc"));
}

#[test]
fn compose_context_policy_defaults_match_expected_limits() {
    let policy = ResolvedComposeContextPolicy::from_request(None);

    assert!(policy.include_recent_records);
    assert!(policy.include_thread_summary);
    assert!(policy.include_subject_memory);
    assert_eq!(policy.summary_limit, 2);
    assert_eq!(policy.recent_limit, 10_000);
}
