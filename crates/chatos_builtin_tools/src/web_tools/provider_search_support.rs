// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use once_cell::sync::Lazy;
use scraper::{Html, Selector};
use serde_json::Value;

use super::provider_search_shared::{
    append_html_search_hits_from_document, build_browser_search_hit,
    build_related_topic_search_hit, insert_search_hit,
};
use super::provider_types::SearchHit;
use super::provider_utils::{
    compile_selector, js_string_array, js_string_literal, normalize_public_web_url,
    normalized_element_text, text_contains_any_marker, truncate_chars,
};

pub(super) const DUCKDUCKGO_RESULT_BLOCK_SELECTOR_RAW: &str = ".result, .result.web-result";
pub(super) const DUCKDUCKGO_RESULT_LINK_SELECTOR_RAW: &str =
    "a.result__a, a.result-link, h2 a, .links_main a";
pub(super) const DUCKDUCKGO_RESULT_SNIPPET_SELECTOR_RAW: &str =
    ".result__snippet, .result-snippet, .snippet, .result__extras__body";
pub(super) const DUCKDUCKGO_ANTI_BOT_MARKERS: &[&str] = &[
    "anomaly-modal",
    "automated requests",
    "duckduckgo detected unusual traffic",
    "unusual traffic",
];

pub(super) static RESULT_BLOCK_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector(DUCKDUCKGO_RESULT_BLOCK_SELECTOR_RAW, "result selector"));
pub(super) static RESULT_LINK_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector(DUCKDUCKGO_RESULT_LINK_SELECTOR_RAW, "result link selector"));
pub(super) static RESULT_SNIPPET_SELECTOR: Lazy<Option<Selector>> = Lazy::new(|| {
    compile_selector(
        DUCKDUCKGO_RESULT_SNIPPET_SELECTOR_RAW,
        "result snippet selector",
    )
});
pub(super) static ANTI_BOT_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector(".anomaly-modal__modal", "anti-bot selector"));
pub(super) static FORM_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("form", "form selector"));
pub(super) static INPUT_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("input", "input selector"));
pub(super) static SUBMIT_SELECTOR: Lazy<Option<Selector>> = Lazy::new(|| {
    compile_selector(
        "input[type='submit'], button[type='submit']",
        "submit selector",
    )
});

pub(super) static BING_RESULT_BLOCK_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("li.b_algo", "bing result selector"));
pub(super) static BING_RESULT_LINK_SELECTOR: Lazy<Option<Selector>> =
    Lazy::new(|| compile_selector("h2 a", "bing result link selector"));
pub(super) static BING_RESULT_SNIPPET_SELECTOR: Lazy<Option<Selector>> = Lazy::new(|| {
    compile_selector(
        ".b_caption p, .b_snippet, p.b_paractl",
        "bing result snippet selector",
    )
});

pub(super) fn parse_duckduckgo_html_results(
    body: &str,
    seen: &mut HashSet<String>,
) -> Vec<SearchHit> {
    let document = Html::parse_document(body);
    let mut hits = Vec::new();
    let (Some(block_selector), Some(link_selector), Some(snippet_selector)) = (
        RESULT_BLOCK_SELECTOR.as_ref(),
        RESULT_LINK_SELECTOR.as_ref(),
        RESULT_SNIPPET_SELECTOR.as_ref(),
    ) else {
        return hits;
    };
    append_html_search_hits_from_document(
        &document,
        block_selector,
        link_selector,
        snippet_selector,
        &["href"],
        "https://cn.bing.com/search",
        &mut hits,
        seen,
        None,
    );

    hits
}

pub(super) fn parse_bing_html_results(body: &str, limit: usize) -> Vec<SearchHit> {
    let document = Html::parse_document(body);
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    let (Some(block_selector), Some(link_selector), Some(snippet_selector)) = (
        BING_RESULT_BLOCK_SELECTOR.as_ref(),
        BING_RESULT_LINK_SELECTOR.as_ref(),
        BING_RESULT_SNIPPET_SELECTOR.as_ref(),
    ) else {
        return hits;
    };
    append_html_search_hits_from_document(
        &document,
        block_selector,
        link_selector,
        snippet_selector,
        &["href", "data-href"],
        "https://duckduckgo.com",
        &mut hits,
        &mut seen,
        Some(limit),
    );

    hits
}

pub(super) fn collect_duckduckgo_instant_answer_hits(
    value: &Value,
    limit: usize,
) -> Vec<SearchHit> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let abstract_url = value
        .get("AbstractURL")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if let Some(abstract_url) = normalize_public_web_url(abstract_url) {
        let title = value
            .get("Heading")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let description = value
            .get("AbstractText")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        insert_search_hit(
            &mut out,
            &mut seen,
            SearchHit {
                url: abstract_url,
                title,
                description,
            },
        );
    }

    if let Some(related) = value.get("RelatedTopics").and_then(|item| item.as_array()) {
        for item in related {
            append_duckduckgo_hit(item, &mut out, &mut seen, limit);
            if out.len() >= limit {
                break;
            }
        }
    }

    if out.len() > limit {
        out.truncate(limit);
    }
    out
}

pub(super) fn parse_duckduckgo_instant_answer_response(
    text: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let value = serde_json::from_str::<Value>(text).map_err(|err| {
        format!(
            "invalid JSON response: {} body={}",
            err,
            truncate_chars(text, 800)
        )
    })?;

    Ok(collect_duckduckgo_instant_answer_hits(&value, limit))
}

pub(super) fn extract_duckduckgo_next_page_form(body: &str) -> Option<Vec<(String, String)>> {
    let document = Html::parse_document(body);
    let form_selector = FORM_SELECTOR.as_ref()?;
    let submit_selector = SUBMIT_SELECTOR.as_ref()?;
    let input_selector = INPUT_SELECTOR.as_ref()?;

    for form in document.select(form_selector) {
        let has_next_button = form.select(submit_selector).any(|button| {
            let value = button.value().attr("value").unwrap_or("");
            if value.eq_ignore_ascii_case("Next") {
                return true;
            }
            normalized_element_text(&button).eq_ignore_ascii_case("Next")
        });

        if !has_next_button {
            continue;
        }

        let params = form
            .select(input_selector)
            .filter_map(|input| {
                let name = input.value().attr("name")?.trim();
                if name.is_empty() {
                    return None;
                }
                let value = input.value().attr("value").unwrap_or("").trim();
                Some((name.to_string(), value.to_string()))
            })
            .collect::<Vec<_>>();
        if !params.is_empty() {
            return Some(params);
        }
    }

    None
}

pub(super) fn looks_like_duckduckgo_antibot(body: &str) -> bool {
    let document = Html::parse_document(body);
    if ANTI_BOT_SELECTOR
        .as_ref()
        .is_some_and(|selector| document.select(selector).next().is_some())
    {
        return true;
    }

    text_contains_any_marker(body, DUCKDUCKGO_ANTI_BOT_MARKERS)
}

pub(super) fn extract_browser_search_hits(
    parsed: &Value,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    if parsed
        .get("anti_bot")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err("browser search hit an anti-bot or challenge page".to_string());
    }

    Ok(parsed
        .get("hits")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(build_browser_search_hit)
                .take(limit)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

fn append_duckduckgo_hit(
    value: &Value,
    out: &mut Vec<SearchHit>,
    seen: &mut HashSet<String>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }

    if let Some(children) = value.get("Topics").and_then(|item| item.as_array()) {
        for child in children {
            append_duckduckgo_hit(child, out, seen, limit);
            if out.len() >= limit {
                break;
            }
        }
        return;
    }

    let Some(hit) = build_related_topic_search_hit(value) else {
        return;
    };
    let _ = insert_search_hit(out, seen, hit);
}

pub(super) fn browser_search_eval_expression(limit: usize) -> String {
    let template = r##"JSON.stringify((() => {
  const clean = (text) => (text || "").replace(/\u00a0/g, " ").replace(/\s+/g, " ").trim();
  const antiBotMarkers = __ANTI_BOT_MARKERS__;
  const decodeHref = (raw) => {
    const href = clean(raw);
    if (!href) return "";
    try {
      const url = new URL(href, window.location.href);
      const uddg = url.searchParams.get("uddg");
      return uddg ? decodeURIComponent(uddg) : url.toString();
    } catch (_err) {
      return href;
    }
  };
  const antiBotText = clean(document.body?.innerText || "").toLowerCase();
  const antiBot = antiBotMarkers.some((marker) => antiBotText.includes(marker));
  const blocks = Array.from(document.querySelectorAll(__RESULT_BLOCK_SELECTOR__));
  const hits = [];
  const seen = new Set();
  for (const block of blocks) {
    const link = block.querySelector(__RESULT_LINK_SELECTOR__);
    if (!link) continue;
    const url = decodeHref(link.getAttribute("href") || link.href || "");
    const title = clean(link.textContent || link.innerText || "");
    if (!url || !title || seen.has(url)) continue;
    seen.add(url);
    const description = clean(
      block.querySelector(__RESULT_SNIPPET_SELECTOR__)?.textContent
      || ""
    );
    hits.push({ url, title, description });
    if (hits.length >= __LIMIT__) break;
  }
  return { anti_bot: antiBot, hits };
})())"##;

    template
        .replace(
            "__ANTI_BOT_MARKERS__",
            &js_string_array(DUCKDUCKGO_ANTI_BOT_MARKERS),
        )
        .replace(
            "__RESULT_BLOCK_SELECTOR__",
            &js_string_literal(DUCKDUCKGO_RESULT_BLOCK_SELECTOR_RAW),
        )
        .replace(
            "__RESULT_LINK_SELECTOR__",
            &js_string_literal(DUCKDUCKGO_RESULT_LINK_SELECTOR_RAW),
        )
        .replace(
            "__RESULT_SNIPPET_SELECTOR__",
            &js_string_literal(DUCKDUCKGO_RESULT_SNIPPET_SELECTOR_RAW),
        )
        .replace("__LIMIT__", limit.max(1).to_string().as_str())
}
