// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use memory_engine_sdk::UpsertRecordInput;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub(super) struct RecordBatchLogSummary {
    pub(super) record_count: usize,
    pub(super) roles: String,
    pub(super) record_ids: String,
    pub(super) tool_names: String,
    pub(super) content_bytes: usize,
    pub(super) max_content_bytes: usize,
    pub(super) metadata_bytes: usize,
    pub(super) structured_payload_bytes: usize,
}

pub(super) fn summarize_record_batch(records: &[UpsertRecordInput]) -> RecordBatchLogSummary {
    let mut roles = Vec::new();
    let mut record_ids = Vec::new();
    let mut tool_names = Vec::new();
    let mut content_bytes = 0usize;
    let mut max_content_bytes = 0usize;
    let mut metadata_bytes = 0usize;
    let mut structured_payload_bytes = 0usize;

    for record in records {
        roles.push(record.role.as_str());
        record_ids.push(record.id.as_str());
        if let Some(tool_name) = record.metadata.as_ref().and_then(metadata_tool_name) {
            tool_names.push(tool_name);
        }
        let current_content_bytes = record.content.len();
        content_bytes += current_content_bytes;
        max_content_bytes = max_content_bytes.max(current_content_bytes);
        metadata_bytes += optional_json_bytes(&record.metadata);
        structured_payload_bytes += optional_json_bytes(&record.structured_payload);
    }

    RecordBatchLogSummary {
        record_count: records.len(),
        roles: summarize_values(roles),
        record_ids: summarize_values(record_ids),
        tool_names: summarize_values(tool_names),
        content_bytes,
        max_content_bytes,
        metadata_bytes,
        structured_payload_bytes,
    }
}

fn metadata_tool_name(metadata: &Value) -> Option<&str> {
    metadata
        .get("toolName")
        .or_else(|| metadata.get("tool_name"))
        .or_else(|| metadata.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn optional_json_bytes(value: &Option<Value>) -> usize {
    value
        .as_ref()
        .and_then(|value| serde_json::to_vec(value).ok())
        .map(|bytes| bytes.len())
        .unwrap_or_default()
}

fn summarize_values(values: Vec<&str>) -> String {
    const LIMIT: usize = 8;
    let mut unique = Vec::<&str>::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || unique.contains(&value) {
            continue;
        }
        unique.push(value);
    }

    let omitted = unique.len().saturating_sub(LIMIT);
    let mut summary = unique.into_iter().take(LIMIT).collect::<Vec<_>>().join(",");
    if omitted > 0 {
        if !summary.is_empty() {
            summary.push(',');
        }
        summary.push_str(format!("+{omitted} more").as_str());
    }
    summary
}
