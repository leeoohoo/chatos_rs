// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use super::provider_types::SearchHit;
use super::provider_utils::{
    guess_title, guess_title_from_url, normalize_public_web_url, normalize_whitespace,
    normalized_element_text, resolve_public_web_url,
};

const DUCKDUCKGO_BASE_URL: &str = "https://duckduckgo.com";

pub(super) fn build_html_search_hit(
    raw_url: &str,
    base_url: &str,
    raw_title: &str,
    raw_description: &str,
) -> Option<SearchHit> {
    let url = clean_search_result_url(raw_url, base_url);
    if url.is_empty() {
        return None;
    }

    let title = normalize_whitespace(raw_title);
    if title.is_empty() {
        return None;
    }

    Some(SearchHit {
        url,
        title,
        description: normalize_whitespace(raw_description),
    })
}

pub(super) fn build_browser_search_hit(value: &Value) -> Option<SearchHit> {
    let raw_url = value
        .get("url")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())?;
    let url = clean_duckduckgo_result_url(raw_url);
    if url.is_empty() {
        return None;
    }

    let title = value
        .get("title")
        .and_then(|item| item.as_str())
        .map(normalize_whitespace)
        .filter(|item| !item.is_empty())?;
    let description = value
        .get("description")
        .and_then(|item| item.as_str())
        .map(normalize_whitespace)
        .unwrap_or_default();

    Some(SearchHit {
        url,
        title,
        description,
    })
}

pub(super) fn build_related_topic_search_hit(value: &Value) -> Option<SearchHit> {
    let url = value
        .get("FirstURL")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    let url = normalize_public_web_url(url)?;
    let title = value
        .get("Text")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|text| guess_title(text, url.as_str()))
        .unwrap_or_else(|| guess_title_from_url(url.as_str()));
    let description = value
        .get("Text")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string();

    Some(SearchHit {
        url,
        title,
        description,
    })
}

pub(super) fn extract_html_search_hit_from_block(
    block: &ElementRef<'_>,
    link_selector: &Selector,
    snippet_selector: &Selector,
    url_attrs: &[&str],
    base_url: &str,
) -> Option<SearchHit> {
    let link = block.select(link_selector).next()?;
    let raw_url = url_attrs
        .iter()
        .find_map(|attr| link.value().attr(attr))
        .unwrap_or("")
        .trim();
    let raw_title = normalized_element_text(&link);
    let raw_description = block
        .select(snippet_selector)
        .next()
        .map(|item| normalized_element_text(&item))
        .unwrap_or_default();

    build_html_search_hit(
        raw_url,
        base_url,
        raw_title.as_str(),
        raw_description.as_str(),
    )
}

pub(super) fn append_html_search_hits_from_document(
    document: &Html,
    block_selector: &Selector,
    link_selector: &Selector,
    snippet_selector: &Selector,
    url_attrs: &[&str],
    base_url: &str,
    out: &mut Vec<SearchHit>,
    seen: &mut HashSet<String>,
    limit: Option<usize>,
) {
    for block in document.select(block_selector) {
        let Some(hit) = extract_html_search_hit_from_block(
            &block,
            link_selector,
            snippet_selector,
            url_attrs,
            base_url,
        ) else {
            continue;
        };
        if !insert_search_hit(out, seen, hit) {
            continue;
        }
        if limit.is_some_and(|max| out.len() >= max) {
            break;
        }
    }
}

pub(super) fn insert_search_hit(
    out: &mut Vec<SearchHit>,
    seen: &mut HashSet<String>,
    hit: SearchHit,
) -> bool {
    if !seen.insert(hit.url.clone()) {
        return false;
    }
    out.push(hit);
    true
}

fn clean_search_result_url(raw: &str, base_url: &str) -> String {
    resolve_public_web_url(raw, base_url).unwrap_or_default()
}

fn clean_duckduckgo_result_url(raw: &str) -> String {
    clean_search_result_url(raw, DUCKDUCKGO_BASE_URL)
}
