// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::StreamExt;
use reqwest::header::CONTENT_TYPE;
use scraper::{ElementRef, Selector};

use super::super::provider_types::ExtractedPage;
use super::super::provider_utils::{
    normalize_multiline_text, normalize_public_web_url, normalize_whitespace,
    normalized_decoded_element_text, normalized_text_contains, DEFAULT_USER_AGENT,
};
use super::provider_extract_content::error_page;

pub(super) const WEB_EXTRACT_ACCEPT_HEADER: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,text/plain;q=0.8,application/json;q=0.8,application/pdf;q=0.7,*/*;q=0.5";
const WEB_EXTRACT_RESPONSE_BODY_FLOOR_BYTES: usize = 4 * 1024 * 1024;
const WEB_EXTRACT_RESPONSE_BODY_CEILING_BYTES: usize = 16 * 1024 * 1024;
const WEB_EXTRACT_RESPONSE_BODY_CHAR_MULTIPLIER: usize = 16;

pub(super) struct PreparedExtractResponse {
    pub(super) source_url: String,
    pub(super) final_url: String,
    pub(super) content_type: Option<String>,
    pub(super) body: Vec<u8>,
}

pub(super) const MIN_BROWSER_RENDER_TRIGGER_CHARS: usize = 320;

pub(super) const CONTENT_SELECTOR_LIST: &[&str] = &[
    "main",
    "article",
    "[role='main']",
    "#main",
    ".main",
    ".content",
    ".article",
    ".post",
    ".post-content",
    ".entry-content",
    ".article-content",
    ".markdown-body",
    ".doc-content",
    ".docs-content",
    "section[itemprop='articleBody']",
    "body",
];

pub(super) const BLOCK_SELECTOR: &str = "p, li, pre, blockquote, h1, h2, h3, h4, h5, h6, tr";
pub(super) const TABLE_CELL_SELECTOR: &str = "th, td";

pub(super) const CONTENT_HINT_MARKERS: &[&str] = &[
    "content", "article", "post", "markdown", "docs", "main", "body",
];

pub(super) const NOISE_EXCLUSION_MARKERS: &[&str] =
    &["article", "content", "markdown", "docs", "main", "post"];

pub(super) const NOISE_ATTR_MARKERS: &[&str] = &[
    "header",
    "footer",
    "nav",
    "sidebar",
    "cookie",
    "modal",
    "popup",
    "social",
    "share",
    "breadcrumb",
    "advert",
    "banner",
    "menu",
    "toolbar",
];

pub(super) const NOISE_TAGS: &[&str] = &["header", "footer", "nav", "aside", "form", "dialog"];

pub(super) const WEAK_RENDER_TRIGGER_MARKERS: &[&str] = &[
    "enable javascript",
    "javascript is required",
    "requires javascript",
    "please turn on javascript",
    "checking your browser",
    "just a moment",
    "loading",
    "please wait",
];

pub(super) const NAVIGATION_MARKERS: &[&str] = &[
    "home",
    "docs",
    "pricing",
    "blog",
    "contact",
    "menu",
    "navigation",
    "sign in",
    "log in",
    "get started",
];

pub(super) const SPA_SHELL_MARKERS: &[&str] = &[
    "id=\"__next\"",
    "id=\"app\"",
    "id=\"root\"",
    "data-reactroot",
    "ng-version",
    "webpack",
    "vite",
    "__nuxt",
];

pub(super) const WEAK_CONTENT_SCORE_MARKERS: &[&str] = &[
    "enable javascript",
    "javascript is required",
    "please wait",
    "loading",
    "sign in",
    "log in",
    "menu",
    "navigation",
];

pub(super) async fn fetch_extract_response(
    client: &reqwest::Client,
    raw_source_url: &str,
    max_extract_chars: usize,
) -> Result<PreparedExtractResponse, ExtractedPage> {
    let Some(source_url) = normalize_public_web_url(raw_source_url) else {
        return Err(error_page(
            raw_source_url.to_string(),
            raw_source_url.to_string(),
            "URL must be a public http(s) address".to_string(),
        ));
    };

    let response = match client
        .get(source_url.as_str())
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header("Accept", WEB_EXTRACT_ACCEPT_HEADER)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return Err(error_page(
                source_url.clone(),
                source_url.clone(),
                format!("request failed: {}", err),
            ));
        }
    };

    finalize_extract_response(
        source_url,
        response,
        extract_response_body_limit_bytes(max_extract_chars),
    )
    .await
}

pub(super) fn count_text_marker_hits(text: &str, markers: &[&str]) -> usize {
    let lowered = text.to_ascii_lowercase();
    markers
        .iter()
        .filter(|marker| lowered.contains(**marker))
        .count()
}

pub(super) fn sentenceish_count(text: &str) -> usize {
    text.matches('.').count() + text.matches('!').count() + text.matches('?').count()
}

pub(super) fn non_empty_line_count(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

pub(super) fn content_quality_score(content: &str) -> usize {
    let normalized = normalize_multiline_text(content);
    if normalized.is_empty() {
        return 0;
    }

    let chars = normalized.chars().count().min(5_000);
    let paragraphs = non_empty_line_count(&normalized);
    let sentenceish = sentenceish_count(&normalized);
    let weak_hits = count_text_marker_hits(&normalized, WEAK_CONTENT_SCORE_MARKERS);

    chars
        .saturating_add(paragraphs.saturating_mul(50))
        .saturating_add(sentenceish.saturating_mul(35))
        .saturating_sub(weak_hits.saturating_mul(90))
}

pub(super) fn looks_like_main_content_attr_value(raw: &str) -> bool {
    let value = normalize_whitespace(raw).to_ascii_lowercase();
    CONTENT_HINT_MARKERS
        .iter()
        .any(|needle| value.contains(needle))
}

pub(super) fn looks_like_noise_attr_value(raw: &str) -> bool {
    let value = normalize_whitespace(raw).to_ascii_lowercase();
    if NOISE_EXCLUSION_MARKERS
        .iter()
        .any(|needle| value.contains(needle))
    {
        return false;
    }

    NOISE_ATTR_MARKERS
        .iter()
        .any(|needle| value.contains(needle))
}

pub(super) fn is_noise_tag(tag: &str) -> bool {
    NOISE_TAGS
        .iter()
        .any(|needle| tag.eq_ignore_ascii_case(needle))
}

pub(super) fn merge_content_candidates(
    primary_content: String,
    metadata_description: &str,
    structured_description: &str,
    structured_blocks: &[String],
) -> String {
    let mut sections = Vec::new();

    if !primary_content.trim().is_empty() {
        sections.push(primary_content.trim().to_string());
    }

    let current_chars = sections
        .iter()
        .map(|item| item.chars().count())
        .sum::<usize>();

    if current_chars < 400 {
        for block in structured_blocks {
            let trimmed = block.trim();
            if trimmed.is_empty() {
                continue;
            }
            if sections
                .iter()
                .any(|existing| normalized_text_contains(existing, trimmed))
            {
                continue;
            }
            sections.push(trimmed.to_string());
        }
    }

    if sections
        .iter()
        .map(|item| item.chars().count())
        .sum::<usize>()
        < 220
    {
        for description in [metadata_description, structured_description] {
            let trimmed = description.trim();
            if trimmed.is_empty() {
                continue;
            }
            if sections
                .iter()
                .any(|existing| normalized_text_contains(existing, trimmed))
            {
                continue;
            }
            sections.insert(0, trimmed.to_string());
        }
    }

    normalize_multiline_text(&sections.join("\n\n"))
}

pub(super) fn collect_candidate_text(
    element: &ElementRef<'_>,
    paragraphish_selector: &Selector,
    table_cell_selector: &Selector,
) -> String {
    let mut blocks = Vec::new();

    for block in element.select(paragraphish_selector) {
        if is_probably_noise_element(&block) {
            continue;
        }

        let formatted = format_block_text(&block, table_cell_selector);
        if formatted.is_empty() {
            continue;
        }
        if blocks
            .last()
            .is_some_and(|last: &String| last == &formatted)
        {
            continue;
        }
        blocks.push(formatted);
    }

    if blocks.is_empty() {
        return flatten_element_text(element);
    }

    normalize_multiline_text(&blocks.join("\n\n"))
}

pub(super) fn score_content_candidate(
    element: &ElementRef<'_>,
    text_chars: usize,
    paragraphish_selector: &Selector,
    link_selector: &Selector,
) -> usize {
    let paragraph_count = element.select(paragraphish_selector).count();
    let link_chars = element
        .select(link_selector)
        .map(|item| flatten_element_text(&item))
        .map(|text| text.chars().count())
        .sum::<usize>();
    let tag_bonus = match element.value().name() {
        "main" => 250,
        "article" => 220,
        "section" => 120,
        "body" => 0,
        _ => 80,
    };
    let attr_bonus = if looks_like_main_content_attr(element) {
        180
    } else {
        0
    };

    text_chars
        .saturating_add(paragraph_count.saturating_mul(60))
        .saturating_add(tag_bonus)
        .saturating_add(attr_bonus)
        .saturating_sub(link_chars / 3)
}

fn flatten_element_text(element: &ElementRef<'_>) -> String {
    normalized_decoded_element_text(element)
}

fn format_block_text(element: &ElementRef<'_>, table_cell_selector: &Selector) -> String {
    let name = element.value().name();
    let text = if name == "tr" {
        let cells = element
            .select(table_cell_selector)
            .map(|cell| flatten_element_text(&cell))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        if cells.is_empty() {
            flatten_element_text(element)
        } else {
            cells.join(" | ")
        }
    } else {
        flatten_element_text(element)
    };

    if text.is_empty() {
        return String::new();
    }

    match name {
        "li" => format!("- {}", text),
        "blockquote" => format!("> {}", text),
        _ => text,
    }
}

fn looks_like_main_content_attr(element: &ElementRef<'_>) -> bool {
    let attrs = [
        element.value().attr("id"),
        element.value().attr("class"),
        element.value().attr("role"),
        element.value().attr("itemprop"),
    ];
    attrs
        .into_iter()
        .flatten()
        .any(looks_like_main_content_attr_value)
}

fn is_probably_noise_element(element: &ElementRef<'_>) -> bool {
    let tag = element.value().name();
    if is_noise_tag(tag) {
        return true;
    }

    let attrs = [
        element.value().attr("id"),
        element.value().attr("class"),
        element.value().attr("role"),
        element.value().attr("aria-label"),
        element.value().attr("data-testid"),
    ];
    attrs.into_iter().flatten().any(looks_like_noise_attr_value)
}

async fn finalize_extract_response(
    source_url: String,
    response: reqwest::Response,
    body_limit_bytes: usize,
) -> Result<PreparedExtractResponse, ExtractedPage> {
    let status = response.status();
    let raw_final_url = response.url().to_string();
    let Some(final_url) = normalize_public_web_url(raw_final_url.as_str()) else {
        return Err(error_page(
            source_url.clone(),
            raw_final_url,
            "final URL resolved to a non-public address".to_string(),
        ));
    };

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    if !status.is_success() {
        return Err(error_page(
            source_url.clone(),
            final_url,
            format!("status={}", status),
        ));
    }

    let body = read_response_body_with_limit(
        response,
        source_url.as_str(),
        final_url.as_str(),
        body_limit_bytes,
    )
    .await?;

    Ok(PreparedExtractResponse {
        source_url,
        final_url,
        content_type,
        body,
    })
}

fn extract_response_body_limit_bytes(max_extract_chars: usize) -> usize {
    max_extract_chars
        .saturating_mul(WEB_EXTRACT_RESPONSE_BODY_CHAR_MULTIPLIER)
        .clamp(
            WEB_EXTRACT_RESPONSE_BODY_FLOOR_BYTES,
            WEB_EXTRACT_RESPONSE_BODY_CEILING_BYTES,
        )
}

async fn read_response_body_with_limit(
    response: reqwest::Response,
    source_url: &str,
    final_url: &str,
    body_limit_bytes: usize,
) -> Result<Vec<u8>, ExtractedPage> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(err) => {
                return Err(error_page(
                    source_url.to_string(),
                    final_url.to_string(),
                    format!("read response failed: {}", err),
                ));
            }
        };
        body.extend_from_slice(chunk.as_ref());
        if body.len() > body_limit_bytes {
            return Err(error_page(
                source_url.to_string(),
                final_url.to_string(),
                format!(
                    "response body exceeded size limit (>{} bytes)",
                    body_limit_bytes
                ),
            ));
        }
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::extract_response_body_limit_bytes;

    #[test]
    fn extract_response_body_limit_bytes_has_floor_and_ceiling() {
        assert_eq!(extract_response_body_limit_bytes(1), 4 * 1024 * 1024);
        assert_eq!(extract_response_body_limit_bytes(100_000), 4 * 1024 * 1024);
        assert_eq!(
            extract_response_body_limit_bytes(2_000_000),
            16 * 1024 * 1024
        );
    }
}
