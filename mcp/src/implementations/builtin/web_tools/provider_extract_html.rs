// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;

use super::super::provider_types::{ExtractedPage, HtmlMetadata, StructuredDataContent};
use super::super::provider_utils::{
    compile_regex, compile_selector, compile_selector_list, decode_basic_html_entities,
    first_non_empty_owned, guess_title_from_url, normalize_multiline_text, normalize_whitespace,
    normalized_element_text, push_unique_normalized_text, replace_all_regex,
};
use super::provider_extract_content::finalize_page;
use super::provider_extract_support::{
    collect_candidate_text, merge_content_candidates, score_content_candidate, BLOCK_SELECTOR,
    CONTENT_SELECTOR_LIST, TABLE_CELL_SELECTOR as TABLE_CELL_SELECTOR_RAW,
};

static RE_SCRIPT_TAG: Lazy<Option<Regex>> =
    Lazy::new(|| compile_regex(r"(?is)<script[^>]*>.*?</script>", "script regex"));
static RE_STYLE_TAG: Lazy<Option<Regex>> =
    Lazy::new(|| compile_regex(r"(?is)<style[^>]*>.*?</style>", "style regex"));
static RE_NOSCRIPT_TAG: Lazy<Option<Regex>> =
    Lazy::new(|| compile_regex(r"(?is)<noscript[^>]*>.*?</noscript>", "noscript regex"));
static RE_TEMPLATE_TAG: Lazy<Option<Regex>> =
    Lazy::new(|| compile_regex(r"(?is)<template[^>]*>.*?</template>", "template regex"));
static RE_COMMENT_TAG: Lazy<Option<Regex>> =
    Lazy::new(|| compile_regex(r"(?is)<!--.*?-->", "comment regex"));
static RE_LAYOUT_TAG: Lazy<Option<Regex>> = Lazy::new(|| {
    compile_regex(
        r"(?is)<(?:header|footer|nav|aside|dialog)[^>]*>.*?</(?:header|footer|nav|aside|dialog)>",
        "layout regex",
    )
});
static RE_NOISE_BLOCK: Lazy<Option<Regex>> = Lazy::new(|| {
    compile_regex(
        r#"(?is)<(?:div|section|aside|nav|header|footer|form)[^>]*(?:id|class|role|aria-label|data-testid)\s*=\s*["'][^"']*(?:header|footer|nav|sidebar|cookie|modal|popup|social|share|breadcrumb|advert|ads|banner|menu|toolbar)[^"']*["'][^>]*>.*?</(?:div|section|aside|nav|header|footer|form)>"#,
        "noise regex",
    )
});
static RE_HTML_TAG: Lazy<Option<Regex>> =
    Lazy::new(|| compile_regex(r"(?is)<[^>]+>", "html tag regex"));

static TITLE_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("title", "title selector"));
static BODY_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("body", "body selector"));
static META_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("meta", "meta selector"));
static JSON_LD_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("script[type='application/ld+json']", "json ld selector"));
static PREFERRED_CONTENT_SELECTORS: Lazy<Vec<Selector>> =
    Lazy::new(|| compile_selector_list(CONTENT_SELECTOR_LIST, "content selector"));
static PARAGRAPHISH_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector(BLOCK_SELECTOR, "paragraphish selector"));
static LINK_SELECTOR: Lazy<Option<Selector>> = Lazy::new(|| compile_selector("a", "link selector"));
static TABLE_CELL_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector(TABLE_CELL_SELECTOR_RAW, "table cell selector"));

pub(super) fn extract_html_page(
    final_url: &str,
    body: &[u8],
    max_extract_chars: usize,
) -> ExtractedPage {
    let html = String::from_utf8_lossy(body).to_string();
    let raw_document = Html::parse_document(&html);
    let metadata = extract_html_metadata(&raw_document);
    let structured = extract_structured_data_content(&raw_document);
    let cleaned_html = clean_html_for_extraction(&html);
    let document = Html::parse_document(&cleaned_html);
    let mut content = extract_main_content_text(&document);
    if content.is_empty() {
        content = html_to_text(&cleaned_html);
    }
    content = merge_content_candidates(
        content,
        metadata.description.as_str(),
        structured.description.as_str(),
        structured.text_blocks.as_slice(),
    );

    finalize_page(
        final_url.to_string(),
        choose_page_title(final_url, &metadata, &structured),
        content,
        max_extract_chars,
    )
}

fn clean_html_for_extraction(html: &str) -> String {
    let mut cleaned = html.to_string();
    cleaned = replace_all_regex(RE_SCRIPT_TAG.as_ref(), &cleaned, " ");
    cleaned = replace_all_regex(RE_STYLE_TAG.as_ref(), &cleaned, " ");
    cleaned = replace_all_regex(RE_NOSCRIPT_TAG.as_ref(), &cleaned, " ");
    cleaned = replace_all_regex(RE_TEMPLATE_TAG.as_ref(), &cleaned, " ");
    cleaned = replace_all_regex(RE_COMMENT_TAG.as_ref(), &cleaned, " ");

    for _ in 0..2 {
        cleaned = replace_all_regex(RE_LAYOUT_TAG.as_ref(), &cleaned, " ");
        cleaned = replace_all_regex(RE_NOISE_BLOCK.as_ref(), &cleaned, " ");
    }

    cleaned
}

fn extract_html_metadata(document: &Html) -> HtmlMetadata {
    let title = TITLE_SELECTOR
        .as_ref()
        .and_then(|selector| document.select(selector).next())
        .map(|item| normalized_element_text(&item))
        .unwrap_or_default();

    let mut description = String::new();
    let mut og_title = String::new();

    if let Some(selector) = META_SELECTOR.as_ref() {
        for meta in document.select(selector) {
            let content = meta.value().attr("content").unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }

            let name = meta
                .value()
                .attr("name")
                .or_else(|| meta.value().attr("property"))
                .or_else(|| meta.value().attr("itemprop"))
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();

            match name.as_str() {
                "description" | "og:description" if description.is_empty() => {
                    description = normalize_whitespace(content);
                }
                "og:title" if og_title.is_empty() => {
                    og_title = normalize_whitespace(content);
                }
                _ => {}
            }
        }
    }

    HtmlMetadata {
        title: first_non_empty_owned(&[og_title.as_str(), title.as_str()]),
        description,
    }
}

fn extract_structured_data_content(document: &Html) -> StructuredDataContent {
    let mut out = StructuredDataContent::default();
    let mut seen = HashSet::new();

    if let Some(selector) = JSON_LD_SELECTOR.as_ref() {
        for script in document.select(selector) {
            let raw = script.text().collect::<Vec<_>>().join("");
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }

            let Ok(value) = serde_json::from_str::<Value>(raw) else {
                continue;
            };
            collect_structured_data_value(&value, &mut out, &mut seen, false);
        }
    }

    out
}

fn collect_structured_data_value(
    value: &Value,
    out: &mut StructuredDataContent,
    seen: &mut HashSet<String>,
    in_contentish_node: bool,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_structured_data_value(item, out, seen, in_contentish_node);
            }
        }
        Value::Object(map) => {
            let type_hint = map
                .get("@type")
                .map(extract_json_string_values)
                .unwrap_or_default()
                .join(" ")
                .to_ascii_lowercase();
            let contentish = in_contentish_node || looks_like_content_type(&type_hint);

            if out.title.is_empty() {
                for key in ["headline", "name", "title"] {
                    if let Some(text) = map.get(key).and_then(normalized_json_string) {
                        out.title = text;
                        break;
                    }
                }
            }

            if out.description.is_empty() {
                for key in ["description", "abstract"] {
                    if let Some(text) = map.get(key).and_then(normalized_json_string) {
                        out.description = text;
                        break;
                    }
                }
            }

            for key in ["articleBody", "text", "description"] {
                if let Some(candidate) = map.get(key).and_then(normalized_json_string) {
                    let long_enough = candidate.chars().count() >= 80;
                    let should_keep = key == "articleBody" || (contentish && long_enough);
                    if should_keep {
                        push_unique_normalized_text(&mut out.text_blocks, seen, candidate);
                    }
                }
            }

            for child in map.values() {
                collect_structured_data_value(child, out, seen, contentish);
            }
        }
        _ => {}
    }
}

fn looks_like_content_type(type_hint: &str) -> bool {
    [
        "article",
        "newsarticle",
        "blogposting",
        "webpage",
        "techarticle",
        "report",
        "documentation",
        "faqpage",
        "howto",
        "product",
    ]
    .iter()
    .any(|needle| type_hint.contains(needle))
}

fn extract_json_string_values(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.to_string()],
        Value::Array(items) => items.iter().flat_map(extract_json_string_values).collect(),
        _ => Vec::new(),
    }
}

fn normalized_json_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let normalized = normalize_multiline_text(&decode_basic_html_entities(text));
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        }
        _ => None,
    }
}

fn choose_page_title(
    final_url: &str,
    metadata: &HtmlMetadata,
    structured: &StructuredDataContent,
) -> String {
    let guessed = guess_title_from_url(final_url);
    first_non_empty_owned(&[
        metadata.title.as_str(),
        structured.title.as_str(),
        guessed.as_str(),
    ])
}

pub(super) fn extract_main_content_text(document: &Html) -> String {
    let paragraphish_selector = PARAGRAPHISH_SELECTOR.as_ref();
    let table_cell_selector = TABLE_CELL_SELECTOR.as_ref();
    let link_selector = LINK_SELECTOR.as_ref();

    let body_text =
        if let (Some(body_selector), Some(paragraphish_selector), Some(table_cell_selector)) = (
            BODY_SELECTOR.as_ref(),
            paragraphish_selector,
            table_cell_selector,
        ) {
            document
                .select(body_selector)
                .next()
                .map(|element| {
                    collect_candidate_text(&element, paragraphish_selector, table_cell_selector)
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

    let mut best_text = String::new();
    let mut best_score = 0usize;

    let Some(paragraphish_selector) = paragraphish_selector else {
        return body_text;
    };
    let Some(table_cell_selector) = table_cell_selector else {
        return body_text;
    };
    let Some(link_selector) = link_selector else {
        return body_text;
    };

    for selector in PREFERRED_CONTENT_SELECTORS.iter() {
        for element in document.select(selector) {
            let text = collect_candidate_text(&element, paragraphish_selector, table_cell_selector);
            let text_chars = text.chars().count();
            if text_chars < 120 {
                continue;
            }

            let score =
                score_content_candidate(&element, text_chars, paragraphish_selector, link_selector);
            if score > best_score {
                best_score = score;
                best_text = text;
            }
        }
    }

    if best_text.is_empty() {
        return body_text;
    }

    let body_len = body_text.chars().count();
    let best_len = best_text.chars().count();
    if body_len > 0 && best_len * 100 / body_len < 20 {
        body_text
    } else {
        best_text
    }
}

pub(super) fn html_to_text(html: &str) -> String {
    let no_script = replace_all_regex(RE_SCRIPT_TAG.as_ref(), html, " ");
    let no_style = replace_all_regex(RE_STYLE_TAG.as_ref(), &no_script, " ");
    let no_noscript = replace_all_regex(RE_NOSCRIPT_TAG.as_ref(), &no_style, " ");
    let raw = replace_all_regex(RE_HTML_TAG.as_ref(), &no_noscript, " ");
    let decoded = decode_basic_html_entities(raw.as_ref());
    normalize_whitespace(decoded.as_str())
}
