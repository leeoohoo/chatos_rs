// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::research_output::{
    build_extract_results_brief as build_shared_extract_results_brief,
    build_search_results_brief as build_shared_search_results_brief, ExtractResultsBriefOptions,
    ExtractStatusStyle, SearchResultsBriefOptions,
};
use crate::research_payloads::{
    build_empty_extract_payload, build_extract_payload_from_research,
    build_search_payload_from_outcome,
};
use crate::web_tools::provider::{ExtractedPage, ResearchExecution, SearchHit};

use super::DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS;

pub(super) fn browser_research_search_payload(research: &ResearchExecution) -> Value {
    build_search_payload_from_outcome(
        &research.search,
        build_browser_research_results_brief(research.search.hits.as_slice()),
    )
}

pub(super) fn browser_research_extract_payload(research: &ResearchExecution) -> Value {
    build_extract_payload_from_research(
        &research.extract,
        build_browser_extract_results_brief(research.extract.pages.as_slice()),
        DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS,
    )
}

pub(super) fn build_browser_research_results_brief(hits: &[SearchHit]) -> Vec<Value> {
    build_shared_search_results_brief(
        hits,
        SearchResultsBriefOptions {
            fallback_title_to_url: true,
        },
    )
}

pub(super) fn build_browser_extract_results_brief(pages: &[ExtractedPage]) -> Vec<Value> {
    build_shared_extract_results_brief(
        pages,
        ExtractResultsBriefOptions {
            fallback_title_to_url: true,
            status_style: ExtractStatusStyle::HumanReadable,
            status_error_max_chars: 120,
            ok_preview_max_chars: 180,
            error_preview_max_chars: 180,
            blank_preview_on_error: false,
            include_error_field: false,
            include_stats_fields: false,
        },
    )
}

pub(super) fn browser_research_empty_extract_payload() -> Value {
    build_empty_extract_payload(DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS)
}
