// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{EngineRecord, EngineSummary};

pub(crate) fn record_to_summary_block(item: &EngineRecord) -> String {
    let mut parts = vec![format!(
        "[record_id={}][created_at={}][role={}]",
        item.id, item.created_at, item.role
    )];

    if !item.content.trim().is_empty() {
        parts.push(item.content.clone());
    }

    if let Some(metadata) = item.metadata.as_ref() {
        if let Some(tool_calls) = metadata
            .get("tool_calls")
            .or_else(|| metadata.get("toolCalls"))
            .filter(|value| !value.is_null())
        {
            parts.push(format!("[tool_calls]\n{}", tool_calls));
        }

        if let Some(structured_result) = metadata
            .get("structured_result")
            .filter(|value| !value.is_null())
        {
            parts.push(format!("[tool_result]\n{}", structured_result));
        }
    }

    if let Some(payload) = item
        .structured_payload
        .as_ref()
        .filter(|value| !value.is_null())
    {
        parts.push(format!("[structured_payload]\n{}", payload));
    }

    parts.join("\n")
}

pub(crate) fn build_summary_digest(
    thread_id: &str,
    level: i64,
    target_level: i64,
    summary_ids: &[String],
) -> String {
    format!(
        "thread_rollup:{}:{}:{}:{}",
        thread_id,
        level,
        target_level,
        summary_ids.join(",")
    )
}

pub(crate) fn summary_to_rollup_block(summary: &EngineSummary) -> String {
    let mut rendered = format!(
        "[level={}][created_at={}][id={}]\n{}",
        summary.level, summary.created_at, summary.id, summary.summary_text
    );
    if let Some(payload) = summary.structured_payload.as_ref() {
        rendered.push_str(format!("\n[structured_payload]\n{payload}").as_str());
    }
    rendered
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::record_to_summary_block;

    #[test]
    fn durable_summary_input_excludes_raw_reasoning_but_keeps_tool_evidence() {
        let record = crate::models::EngineRecord {
            id: "record-1".to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            external_record_id: None,
            role: "assistant".to_string(),
            record_type: "message".to_string(),
            content: "public assistant response".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "reasoning": "private chain of thought",
                "tool_calls": [{"id":"call-1","name":"inspect"}],
                "structured_result": {"call_id":"call-1","result":"verified"}
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-09T00:00:00Z".to_string(),
        };

        let rendered = record_to_summary_block(&record);

        assert!(!rendered.contains("private chain of thought"));
        assert!(!rendered.contains("[reasoning]"));
        assert!(rendered.contains("[tool_calls]"));
        assert!(rendered.contains("[tool_result]"));
        assert!(rendered.contains("verified"));
    }
}
