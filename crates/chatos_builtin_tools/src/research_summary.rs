// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::web_tools::provider::{ResearchExtractExecution, SearchOutcome};

pub fn build_empty_research_summary(page_success: Option<bool>) -> Value {
    let mut summary = json!({
        "search_backend": "none",
        "search_fallback_used": false,
        "search_result_count": 0,
        "selected_url_count": 0,
        "extract_backend": "none",
        "extract_fallback_used": false,
        "extracted_page_count": 0,
        "truncated_page_count": 0,
        "total_original_chars": 0,
        "total_returned_chars": 0,
        "total_omitted_chars": 0,
        "warning": Value::Null,
    });
    if let Some(page_success) = page_success {
        if let Some(map) = summary.as_object_mut() {
            map.insert("page_success".to_string(), Value::Bool(page_success));
        }
    }
    summary
}

pub fn apply_research_execution_summary(
    summary: &mut Value,
    search: &SearchOutcome,
    selected_url_count: usize,
    extract: &ResearchExtractExecution,
) {
    let Some(map) = summary.as_object_mut() else {
        return;
    };

    map.insert(
        "search_backend".to_string(),
        Value::String(search.backend.clone()),
    );
    map.insert(
        "search_fallback_used".to_string(),
        Value::Bool(search.fallback_used),
    );
    map.insert(
        "search_result_count".to_string(),
        Value::from(search.hits.len() as u64),
    );
    map.insert(
        "selected_url_count".to_string(),
        Value::from(selected_url_count as u64),
    );
    map.insert(
        "extract_backend".to_string(),
        Value::String(extract.backend.clone()),
    );
    map.insert(
        "extract_fallback_used".to_string(),
        Value::Bool(extract.fallback_used),
    );
    map.insert(
        "extracted_page_count".to_string(),
        Value::from(extract.stats.page_count as u64),
    );
    map.insert(
        "truncated_page_count".to_string(),
        Value::from(extract.stats.truncated_page_count as u64),
    );
    map.insert(
        "total_original_chars".to_string(),
        Value::from(extract.stats.total_original_chars as u64),
    );
    map.insert(
        "total_returned_chars".to_string(),
        Value::from(extract.stats.total_returned_chars as u64),
    );
    map.insert(
        "total_omitted_chars".to_string(),
        Value::from(extract.stats.total_omitted_chars as u64),
    );
}

pub fn set_research_summary_warning(summary: &mut Value, warning: Option<&str>) {
    let Some(map) = summary.as_object_mut() else {
        return;
    };
    map.insert(
        "warning".to_string(),
        warning
            .filter(|value| !value.trim().is_empty())
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
}
