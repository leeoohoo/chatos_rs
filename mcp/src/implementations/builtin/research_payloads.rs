// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::web_tools::provider::{
    compute_research_extract_stats, ExtractOutcome, ProviderAttempt, ResearchExtractExecution,
    ResearchExtractStats, SearchHit, SearchOutcome,
};

pub fn build_search_payload_from_outcome(
    outcome: &SearchOutcome,
    results_brief: Vec<Value>,
) -> Value {
    build_search_payload(
        outcome.backend.as_str(),
        outcome.fallback_used,
        outcome.attempts.as_slice(),
        outcome.hits.as_slice(),
        results_brief,
    )
}

pub fn build_search_payload(
    backend: &str,
    fallback_used: bool,
    provider_attempts: &[ProviderAttempt],
    hits: &[SearchHit],
    results_brief: Vec<Value>,
) -> Value {
    json!({
        "backend": backend,
        "fallback_used": fallback_used,
        "provider_attempts": provider_attempts,
        "result_count": hits.len(),
        "results_brief": results_brief,
        "data": {
            "web": hits
        }
    })
}

pub fn build_empty_search_payload() -> Value {
    json!({
        "backend": "none",
        "fallback_used": false,
        "provider_attempts": [],
        "result_count": 0,
        "results_brief": [],
        "data": {
            "web": []
        }
    })
}

pub fn build_extract_payload_from_outcome(
    outcome: &ExtractOutcome,
    results_brief: Vec<Value>,
    max_extract_chars: usize,
) -> Value {
    build_extract_payload(
        outcome.backend.as_str(),
        outcome.fallback_used,
        outcome.attempts.as_slice(),
        &outcome.pages,
        compute_research_extract_stats(outcome.pages.as_slice()),
        results_brief,
        max_extract_chars,
    )
}

pub fn build_extract_payload_from_research(
    execution: &ResearchExtractExecution,
    results_brief: Vec<Value>,
    max_extract_chars: usize,
) -> Value {
    build_extract_payload(
        execution.backend.as_str(),
        execution.fallback_used,
        execution.attempts.as_slice(),
        &execution.pages,
        execution.stats,
        results_brief,
        max_extract_chars,
    )
}

pub fn build_extract_payload(
    backend: &str,
    fallback_used: bool,
    provider_attempts: &[ProviderAttempt],
    pages: &[crate::web_tools::provider::ExtractedPage],
    stats: ResearchExtractStats,
    results_brief: Vec<Value>,
    max_extract_chars: usize,
) -> Value {
    json!({
        "backend": backend,
        "fallback_used": fallback_used,
        "provider_attempts": provider_attempts,
        "results_brief": results_brief,
        "extract_summary": build_extract_summary_payload(stats, max_extract_chars),
        "results": pages
    })
}

pub fn build_empty_extract_payload(max_extract_chars: usize) -> Value {
    json!({
        "backend": "none",
        "fallback_used": false,
        "provider_attempts": [],
        "results_brief": [],
        "extract_summary": build_extract_summary_payload(
            ResearchExtractStats::default(),
            max_extract_chars,
        ),
        "results": []
    })
}

pub fn build_extract_summary_payload(
    stats: ResearchExtractStats,
    max_extract_chars: usize,
) -> Value {
    json!({
        "max_extract_chars_per_page": max_extract_chars,
        "page_count": stats.page_count,
        "truncated_page_count": stats.truncated_page_count,
        "total_original_chars": stats.total_original_chars,
        "total_returned_chars": stats.total_returned_chars,
        "total_omitted_chars": stats.total_omitted_chars
    })
}
