// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::models::EngineSubjectMemory;
use crate::services::ai_pipeline::SummaryBuildResult;

use super::PendingSourceSummary;

pub(crate) fn summary_to_subject_memory_block(item: &PendingSourceSummary) -> String {
    let project_prefix = project_id_from_summary_metadata(item.metadata.as_ref())
        .map(|project_id| format!("[project_id={}]", project_id))
        .unwrap_or_default();
    format!(
        "{}[summary_id={}][thread_id={}][created_at={}][summary_type={}][level={}]\n{}",
        project_prefix,
        item.id,
        item.thread_id,
        item.created_at,
        item.summary_type,
        item.level,
        item.summary_text
    )
}

pub(crate) fn subject_memory_to_rollup_block(item: &EngineSubjectMemory) -> String {
    format!(
        "[level={}][memory_key={}][updated_at={}]\n{}",
        item.level, item.memory_key, item.updated_at, item.text
    )
}

pub(crate) fn build_memory_metadata(
    memory_metadata: Option<Value>,
    relation_subject_id: &str,
    source_thread_label: &str,
) -> Option<Value> {
    let mut map = match memory_metadata {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    map.insert(
        "relation_subject_id".to_string(),
        Value::String(relation_subject_id.to_string()),
    );
    map.insert(
        "source_thread_label".to_string(),
        Value::String(source_thread_label.to_string()),
    );
    Some(Value::Object(map))
}

pub(crate) fn decorate_generated_text(
    build: SummaryBuildResult,
    oversized_count: Option<usize>,
    label: &str,
    keep_level0_count: i64,
) -> String {
    let mut text = build.text;
    if build.chunk_count > 1 {
        text.push_str(&format!(
            "\n\n[meta] This {} was merged from {} chunks.",
            label, build.chunk_count
        ));
    }
    if build.overflow_retry_count > 0 {
        text.push_str(&format!(
            "\n\n[meta] Context overflow retry count: {}.",
            build.overflow_retry_count
        ));
    }
    if let Some(count) = oversized_count.filter(|value| *value > 0) {
        text.push_str(&format!(
            "\n\n[meta] {} oversized source memories were marked rolled up without being merged into the body.",
            count
        ));
    }
    if keep_level0_count > 0 {
        let _ = keep_level0_count;
    }
    text
}

pub(crate) fn digest_from_ids(namespace: &str, ids: &[String]) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(namespace.trim().as_bytes());
    hasher.update(b"\n");

    let mut count = 0usize;
    for id in ids {
        let normalized = id.trim();
        if normalized.is_empty() {
            continue;
        }
        hasher.update(normalized.as_bytes());
        hasher.update(b"\n");
        count += 1;
    }

    if count == 0 {
        return None;
    }

    Some(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn project_id_from_summary_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata
        .and_then(|value| value.get("legacy_session_mapping"))
        .and_then(|mapping| mapping.get("project_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            metadata
                .and_then(|value| value.get("project_id"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            metadata
                .and_then(|value| value.get("projectId"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
