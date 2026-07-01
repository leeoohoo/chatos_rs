// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "actions_research.rs"]
mod actions_research;
#[path = "actions_search_extract.rs"]
mod actions_search_extract;
#[path = "actions_shared.rs"]
mod actions_shared;

use serde_json::Value;

use super::BoundContext;

pub(super) async fn web_search_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
) -> Result<Value, String> {
    actions_search_extract::web_search_with_context(ctx, query, limit).await
}

pub(super) async fn web_extract_with_context(
    ctx: BoundContext,
    urls: Vec<String>,
) -> Result<Value, String> {
    actions_search_extract::web_extract_with_context(ctx, urls).await
}

pub(super) async fn web_research_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
    extract_top: Option<usize>,
) -> Result<Value, String> {
    actions_research::web_research_with_context(ctx, query, limit, extract_top).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::provider::{ExtractedPage, SearchHit};
    use super::actions_research::{build_research_summary, build_web_research_findings};
    use super::actions_shared::{build_extract_results_brief, normalize_extract_urls};

    #[test]
    fn normalize_extract_urls_dedupes_and_clamps() {
        let urls = normalize_extract_urls(
            vec![
                " https://example.com/a ".to_string(),
                "https://example.com/a".to_string(),
                "".to_string(),
                "https://example.com/b".to_string(),
                "https://example.com/c".to_string(),
            ],
            2,
        );

        assert_eq!(
            urls,
            vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
            ]
        );
    }

    #[test]
    fn normalize_extract_urls_filters_non_public_targets() {
        let urls = normalize_extract_urls(
            vec![
                "http://127.0.0.1:8080/admin".to_string(),
                "ftp://example.com/archive.zip".to_string(),
                "https://example.com/docs".to_string(),
                "https://localhost:3000/debug".to_string(),
                "https://example.com/docs#overview".to_string(),
            ],
            5,
        );

        assert_eq!(urls, vec!["https://example.com/docs".to_string()]);
    }

    #[test]
    fn research_summary_mentions_search_extract_and_warning() {
        let summary = build_research_summary(
            "rust browser automation",
            &[SearchHit {
                url: "https://example.com/rust".to_string(),
                title: "Rust Browser Guide".to_string(),
                description: "A practical browser automation guide.".to_string(),
            }],
            &[ExtractedPage {
                url: "https://example.com/rust".to_string(),
                title: "Rust Browser Guide".to_string(),
                content: "A practical browser automation guide with Playwright examples."
                    .to_string(),
                content_chars: 62,
                original_content_chars: 62,
                truncated: false,
                content_summary: None,
                error: None,
            }],
            1,
            0,
            0,
            Some("extract backend is unavailable"),
        );

        assert!(summary.contains("Web research for \"rust browser automation\""));
        assert!(summary.contains("found 1 result(s) and extracted 1 page(s)"));
        assert!(summary.contains("search 1. Rust Browser Guide"));
        assert!(summary.contains("extract 1. Rust Browser Guide"));
        assert!(summary.contains("Research warning: extract backend is unavailable."));
    }

    #[test]
    fn web_research_findings_include_sources_and_next_steps() {
        let response = json!({
            "query": "rust browser automation",
            "research_summary": {
                "search_result_count": 2,
                "selected_url_count": 2,
                "extracted_page_count": 1,
                "search_backend": "chatos_native_search",
                "extract_backend": "chatos_native_extract",
                "truncated_page_count": 1,
                "total_omitted_chars": 240,
                "warning": "extract fallback used"
            },
            "search": {
                "results_brief": [
                    {
                        "title": "Rust Browser Guide",
                        "url": "https://example.com/rust-browser",
                        "description_preview": "Explains Playwright-style automation in Rust."
                    }
                ]
            },
            "extract": {
                "results_brief": [
                    {
                        "title": "Rust Browser Guide",
                        "url": "https://example.com/rust-browser",
                        "status": "ok, truncated",
                        "content_preview": "Includes setup, navigation, and screenshot steps."
                    }
                ]
            }
        });

        let findings = build_web_research_findings(&response);
        assert_eq!(
            findings
                .get("answer_frame")
                .and_then(|value| value.as_str()),
            Some(
                "Web research for \"rust browser automation\" found 2 result(s) and extracted 1 page(s)."
            )
        );
        assert!(findings
            .get("web_findings")
            .and_then(|value| value.as_array())
            .is_some_and(|items| items
                .iter()
                .any(|item| item.as_str().is_some_and(|text| text
                    .contains("Key extracted sources: Rust Browser Guide (ok, truncated).")))));
        assert_eq!(
            findings
                .get("source_highlights")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|item| item.get("status"))
                .and_then(|value| value.as_str()),
            Some("ok, truncated")
        );
        assert!(findings
            .get("recommended_next_steps")
            .and_then(|value| value.as_array())
            .is_some_and(|items| items.iter().any(|item| item
                .as_str()
                .is_some_and(|text| text.contains("browser_research")))));
    }

    #[test]
    fn extract_results_brief_hides_error_only_rows_when_success_content_exists() {
        let items = build_extract_results_brief(&[
            ExtractedPage {
                url: "https://example.com/a".to_string(),
                title: "A".to_string(),
                content: "Useful extracted content".to_string(),
                content_chars: 22,
                original_content_chars: 22,
                truncated: false,
                content_summary: None,
                error: None,
            },
            ExtractedPage {
                url: "https://example.com/b".to_string(),
                title: "B".to_string(),
                content: String::new(),
                content_chars: 0,
                original_content_chars: 0,
                truncated: false,
                content_summary: None,
                error: Some("request timed out".to_string()),
            },
        ]);

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("url").and_then(|value| value.as_str()),
            Some("https://example.com/a")
        );
    }
}
