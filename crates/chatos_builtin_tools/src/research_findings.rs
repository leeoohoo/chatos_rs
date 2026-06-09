use serde_json::{json, Value};

use crate::research_output::normalize_inline_text;

pub fn response_search_results_brief(response: &Value) -> Option<&Vec<Value>> {
    response
        .get("search")
        .and_then(|value| value.get("results_brief"))
        .and_then(|value| value.as_array())
}

pub fn response_extract_results_brief(response: &Value) -> Option<&Vec<Value>> {
    response
        .get("extract")
        .and_then(|value| value.get("results_brief"))
        .and_then(|value| value.as_array())
}

pub fn top_search_hit_titles(
    response: &Value,
    title_max_chars: usize,
    limit: usize,
) -> Vec<String> {
    response_search_results_brief(response)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("title")
                        .and_then(|value| value.as_str())
                        .map(|value| normalize_inline_text(value, title_max_chars))
                        .filter(|value| !value.is_empty())
                })
                .take(limit)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn top_extract_source_titles(
    response: &Value,
    title_max_chars: usize,
    status_max_chars: usize,
    limit: usize,
) -> Vec<String> {
    response_extract_results_brief(response)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let title = item
                        .get("title")
                        .and_then(|value| value.as_str())
                        .map(|value| normalize_inline_text(value, title_max_chars))
                        .filter(|value| !value.is_empty())?;
                    let status = item
                        .get("status")
                        .and_then(|value| value.as_str())
                        .map(|value| normalize_inline_text(value, status_max_chars))
                        .unwrap_or_else(|| "unknown".to_string());
                    Some(format!("{} ({})", title, status))
                })
                .take(limit)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn response_research_warning(response: &Value) -> Option<&str> {
    response
        .get("research_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            response
                .get("research_summary")
                .and_then(|value| value.get("warning"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
        })
}

pub fn response_source_highlights(response: &Value) -> Vec<Value> {
    build_research_source_highlights(
        response_extract_results_brief(response),
        response_search_results_brief(response),
    )
}

pub fn build_research_source_highlights(
    extract_results_brief: Option<&Vec<Value>>,
    search_results_brief: Option<&Vec<Value>>,
) -> Vec<Value> {
    if let Some(items) = extract_results_brief.filter(|items| !items.is_empty()) {
        let highlights = items
            .iter()
            .filter_map(build_extract_source_highlight)
            .take(3)
            .collect::<Vec<_>>();
        if !highlights.is_empty() {
            return highlights;
        }
    }

    search_results_brief
        .map(|items| {
            items
                .iter()
                .filter_map(build_search_source_highlight)
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn push_unique_text(values: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if values.iter().any(|existing| existing == trimmed) {
        return;
    }
    values.push(trimmed.to_string());
}

fn build_extract_source_highlight(item: &Value) -> Option<Value> {
    let url = item
        .get("url")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let title = item
        .get("title")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 120))
        .unwrap_or_else(|| normalize_inline_text(url, 120));
    let note = item
        .get("content_preview")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 200))
        .unwrap_or_default();
    let status = item
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 80))
        .unwrap_or_else(|| "unknown".to_string());

    if title.is_empty() && url.is_empty() && note.is_empty() {
        return None;
    }

    Some(json!({
        "kind": "extract",
        "title": title,
        "url": url,
        "status": status,
        "note": note,
    }))
}

fn build_search_source_highlight(item: &Value) -> Option<Value> {
    let url = item
        .get("url")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let title = item
        .get("title")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 120))
        .unwrap_or_else(|| normalize_inline_text(url, 120));
    let note = item
        .get("description_preview")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 180))
        .unwrap_or_default();

    if title.is_empty() && url.is_empty() && note.is_empty() {
        return None;
    }

    Some(json!({
        "kind": "search",
        "title": title,
        "url": url,
        "status": "search_hit",
        "note": note,
    }))
}
