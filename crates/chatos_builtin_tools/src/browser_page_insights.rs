// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::research_output::normalize_inline_text;

pub(crate) fn page_label_from_response(response: &Value) -> String {
    let title = response
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let raw_url = response
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let url = if is_meaningful_browser_url(raw_url) {
        raw_url
    } else {
        ""
    };
    if !title.is_empty() && !url.is_empty() {
        format!(
            "Current page: {} [{}].",
            normalize_inline_text(title, 120),
            normalize_inline_text(url, 180)
        )
    } else if !title.is_empty() {
        format!("Current page title: {}.", normalize_inline_text(title, 120))
    } else if !url.is_empty() {
        format!("Current page URL: {}.", normalize_inline_text(url, 180))
    } else {
        String::new()
    }
}

pub(crate) fn page_state_warning_line(response: &Value) -> Option<String> {
    response
        .get("page_state_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Page state warning: {}.", normalize_inline_text(value, 180)))
}

pub(crate) fn inspection_warning_line(response: &Value) -> Option<String> {
    response
        .get("inspection_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Inspection warning: {}.", normalize_inline_text(value, 180)))
}

pub(crate) fn latest_js_error_line(response: &Value, label: &str) -> Option<String> {
    response
        .get("errors_brief")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("message_preview"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("{}: {}.", label, normalize_inline_text(value, 180)))
}

pub(crate) fn latest_console_text_line(response: &Value, label: &str) -> Option<String> {
    response
        .get("messages_brief")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("text_preview"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("{}: {}.", label, normalize_inline_text(value, 180)))
}

pub(crate) fn visible_refs_summary_line(response: &Value) -> Option<String> {
    response
        .get("element_count")
        .and_then(|value| value.as_u64())
        .map(|count| format!("Visible refs in snapshot: {}.", count))
}

pub(crate) fn is_meaningful_browser_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    !matches!(
        normalized.as_str(),
        "about:blank"
            | "about:srcdoc"
            | "about:newtab"
            | "data:,"
            | "chrome://newtab/"
            | "chrome://new-tab-page/"
            | "edge://newtab/"
    )
}
