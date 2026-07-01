// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{EngineRecord, EngineSummary};
use crate::services::ai_pipeline::SummaryBuildResult;

pub(crate) fn record_to_summary_block(item: &EngineRecord) -> String {
    let mut parts = vec![format!("[{}][{}]", item.created_at, item.role)];

    if !item.content.trim().is_empty() {
        parts.push(item.content.clone());
    }

    if let Some(metadata) = item.metadata.as_ref() {
        if let Some(reasoning) = metadata
            .get("reasoning")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("[reasoning]\n{}", reasoning));
        }

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

pub(crate) fn decorate_generated_text(
    build: SummaryBuildResult,
    oversized_count: Option<usize>,
    label: &str,
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
            "\n\n[meta] {} oversized source items were skipped or marked without merging because each one individually exceeded the token limit.",
            count
        ));
    }
    text
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
    format!(
        "[level={}][created_at={}][id={}]\n{}",
        summary.level, summary.created_at, summary.id, summary.summary_text
    )
}
