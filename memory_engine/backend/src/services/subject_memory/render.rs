// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::models::EngineSubjectMemory;

use super::PendingSourceSummary;

pub(crate) fn summary_to_subject_memory_block(item: &PendingSourceSummary) -> String {
    let project_prefix = project_id_from_summary_metadata(item.metadata.as_ref())
        .map(|project_id| format!("[project_id={}]", project_id))
        .unwrap_or_default();
    let mut rendered = format!(
        "{}[summary_id={}][thread_id={}][created_at={}][summary_type={}][level={}]\n{}",
        project_prefix,
        item.id,
        item.thread_id,
        item.created_at,
        item.summary_type,
        item.level,
        item.summary_text
    );
    if let Some(payload) = item.structured_payload.as_ref() {
        rendered.push_str(format!("\n[structured_payload]\n{payload}").as_str());
    }
    rendered
}

pub(crate) fn subject_memory_to_rollup_block(item: &EngineSubjectMemory) -> String {
    let mut rendered = format!(
        "[memory_id={}][level={}][memory_key={}][updated_at={}]\n{}",
        item.id, item.level, item.memory_key, item.updated_at, item.text
    );
    if let Some(payload) = item.structured_payload.as_ref() {
        rendered.push_str(format!("\n[structured_payload]\n{payload}").as_str());
    }
    rendered
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
        .and_then(|value| value.get("project_subject_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            metadata
                .and_then(|value| value.get("legacy_session_mapping"))
                .and_then(|mapping| mapping.get("project_id"))
                .and_then(Value::as_str)
        })
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

#[cfg(test)]
mod tests {
    use super::{project_id_from_summary_metadata, summary_to_subject_memory_block};
    use crate::services::subject_memory::PendingSourceSummary;
    use serde_json::json;

    #[test]
    fn current_project_subject_id_is_rendered_for_agent_memory_input() {
        let item = PendingSourceSummary {
            id: "summary-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "chatos".to_string(),
            thread_id: "thread-1".to_string(),
            summary_type: "thread_incremental".to_string(),
            level: 0,
            summary_text: "project facts".to_string(),
            structured_payload: None,
            metadata: Some(json!({"project_subject_id": "project:alpha"})),
            created_at: "2026-11-05T00:00:00Z".to_string(),
        };

        assert_eq!(
            project_id_from_summary_metadata(item.metadata.as_ref()).as_deref(),
            Some("project:alpha")
        );
        assert!(summary_to_subject_memory_block(&item).starts_with("[project_id=project:alpha]"));
    }
}
