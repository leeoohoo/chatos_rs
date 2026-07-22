// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::research_output::{
    build_extract_results_brief as build_shared_extract_results_brief, build_extract_summary_line,
    build_search_results_brief as build_shared_search_results_brief, build_search_summary_line,
    ExtractResultsBriefOptions, ExtractStatusStyle, ExtractSummaryLineOptions,
    SearchResultsBriefOptions, SearchSummaryLineOptions,
};
use crate::web_tools::provider::normalize_public_web_url;
use crate::web_tools::provider::{ExtractedPage, SearchHit};

pub(super) use crate::research_output::normalize_inline_text;

pub(super) fn normalize_extract_urls(urls: Vec<String>, max_extract_urls: usize) -> Vec<String> {
    let mut normalized = Vec::new();
    for raw in urls {
        let Some(trimmed) = normalize_public_web_url(raw.as_str()) else {
            continue;
        };
        if normalized.iter().any(|value: &String| value == &trimmed) {
            continue;
        }
        normalized.push(trimmed);
        if normalized.len() >= max_extract_urls {
            break;
        }
    }
    normalized
}

pub(super) fn build_search_summary(
    query: &str,
    _backend: &str,
    _fallback_used: bool,
    _attempt_count: usize,
    hits: &[SearchHit],
) -> String {
    let mut lines = vec![format!(
        "Web search for \"{}\" returned {} result(s).",
        normalize_inline_text(query, 160),
        hits.len(),
    )];

    if hits.is_empty() {
        lines.push("No search hits were returned.".to_string());
        return lines.join("\n");
    }

    for (index, hit) in hits.iter().enumerate() {
        lines.push(build_search_summary_line(
            index,
            hit,
            SearchSummaryLineOptions {
                label_prefix: None,
                fallback_title_to_url: true,
                title_max_chars: 120,
                url_max_chars: 140,
                description_max_chars: 180,
            },
        ));
    }

    lines.join("\n")
}

pub(super) fn build_search_results_brief(hits: &[SearchHit]) -> Vec<Value> {
    build_shared_search_results_brief(
        hits,
        SearchResultsBriefOptions {
            fallback_title_to_url: false,
        },
    )
}

pub(super) fn build_extract_summary(
    _backend: &str,
    _fallback_used: bool,
    _attempt_count: usize,
    pages: &[ExtractedPage],
    truncated_page_count: usize,
    total_omitted_chars: usize,
) -> String {
    let mut lines = vec![format!(
        "Web extract returned {} page(s) (truncated pages: {}, omitted chars: {}).",
        pages.len(),
        truncated_page_count,
        total_omitted_chars,
    )];

    if pages.is_empty() {
        lines.push("No pages were extracted.".to_string());
        return lines.join("\n");
    }

    for (index, page) in pages.iter().enumerate() {
        lines.push(build_extract_summary_line(
            index,
            page,
            ExtractSummaryLineOptions {
                label_prefix: None,
                fallback_title_to_url: true,
                title_max_chars: 120,
                url_max_chars: 140,
                status_style: ExtractStatusStyle::HumanReadable,
                status_error_max_chars: 120,
                ok_preview_max_chars: 220,
                error_preview_max_chars: 180,
            },
        ));
    }

    lines.join("\n")
}

pub(super) fn build_extract_results_brief(pages: &[ExtractedPage]) -> Vec<Value> {
    build_shared_extract_results_brief(
        pages,
        ExtractResultsBriefOptions {
            fallback_title_to_url: false,
            status_style: ExtractStatusStyle::Canonical,
            status_error_max_chars: 120,
            ok_preview_max_chars: 220,
            error_preview_max_chars: 180,
            blank_preview_on_error: true,
            include_error_field: true,
            include_stats_fields: true,
        },
    )
}
