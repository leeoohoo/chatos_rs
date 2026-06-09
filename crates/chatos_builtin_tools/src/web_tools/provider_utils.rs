use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Error, Response};
use scraper::{ElementRef, Selector};
use tracing::warn;
use url::Url;

use super::provider_types::{ExtractContentSummary, ExtractSummaryChunk, ProviderAttempt};
pub(crate) use super::provider_url_policy::{
    normalize_public_web_url, resolve_public_web_url, DEFAULT_USER_AGENT,
};

static REQWEST_URL_SUFFIX_RE: Lazy<Regex> = Lazy::new(|| {
    compile_regex(r#"(?i)\s+for url \([^)]+\)"#, "reqwest url suffix regex")
        .unwrap_or_else(|| Regex::new("$^").unwrap_or_else(|_| unreachable!()))
});

pub(crate) fn compile_regex(pattern: &str, label: &str) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(regex) => Some(regex),
        Err(err) => {
            warn!("failed to compile {}: {}", label, err);
            None
        }
    }
}

pub(crate) fn compile_selector(pattern: &str, label: &str) -> Option<Selector> {
    match Selector::parse(pattern) {
        Ok(selector) => Some(selector),
        Err(err) => {
            warn!("failed to compile {}: {}", label, err);
            None
        }
    }
}

pub(crate) fn compile_selector_list(patterns: &[&str], label: &str) -> Vec<Selector> {
    patterns
        .iter()
        .filter_map(|pattern| compile_selector(pattern, label))
        .collect()
}

pub(crate) fn replace_all_regex(pattern: Option<&Regex>, input: &str, replacement: &str) -> String {
    pattern
        .map(|regex| regex.replace_all(input, replacement).into_owned())
        .unwrap_or_else(|| input.to_string())
}

pub(crate) fn summarize_attempts(attempts: &[ProviderAttempt]) -> String {
    if attempts.is_empty() {
        return "no strategy attempted".to_string();
    }
    attempts
        .iter()
        .map(|item| format!("{}: {}", item.provider, item.error))
        .collect::<Vec<_>>()
        .join(" | ")
}

pub(crate) fn summarize_reqwest_error(err: &Error, fallback: &str) -> String {
    if err.is_timeout() {
        return "request timed out".to_string();
    }
    if err.is_connect() {
        return "network connection failed".to_string();
    }
    if err.is_request() {
        return "network request could not be sent".to_string();
    }
    if err.is_decode() {
        return "response could not be decoded".to_string();
    }

    let normalized = sanitize_provider_error(err.to_string());
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

pub(crate) fn sanitize_provider_error(error: impl AsRef<str>) -> String {
    let raw = error.as_ref().trim();
    if raw.is_empty() {
        return "unknown error".to_string();
    }

    let lowered = raw.to_ascii_lowercase();
    if lowered.contains("error sending request") || lowered.contains("send request failed") {
        return "network request could not be sent".to_string();
    }
    if lowered.contains("dns")
        || lowered.contains("failed to lookup address information")
        || lowered.contains("name or service not known")
        || lowered.contains("no address associated with hostname")
    {
        return "DNS lookup failed".to_string();
    }
    if lowered.contains("timed out") {
        return "request timed out".to_string();
    }
    if lowered.contains("connection refused") {
        return "connection was refused".to_string();
    }

    let without_url = replace_all_regex(Some(&REQWEST_URL_SUFFIX_RE), raw, "");
    normalize_whitespace(without_url.trim())
}

pub(crate) async fn read_success_response_text(
    response: Response,
    read_error_fallback: &str,
    body_limit: usize,
) -> Result<String, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("{}: {}", read_error_fallback, err))?;
    if !status.is_success() {
        return Err(format!(
            "status={} body={}",
            status,
            truncate_chars(&body, body_limit)
        ));
    }

    Ok(body)
}

pub(crate) fn guess_title(description: &str, url: &str) -> String {
    let trimmed = description.trim();
    if let Some((head, _)) = trimmed.split_once(" - ") {
        let title = head.trim();
        if !title.is_empty() {
            return title.to_string();
        }
    }

    guess_title_from_url(url)
}

pub(crate) fn guess_title_from_url(url: &str) -> String {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(segment) = parsed
            .path_segments()
            .and_then(|segments| segments.filter(|item| !item.is_empty()).last())
        {
            let decoded = urlencoding::decode(segment)
                .map(|value| value.into_owned())
                .unwrap_or_else(|_| segment.to_string());
            let normalized = decoded.replace(['-', '_'], " ");
            let title = normalize_whitespace(&normalized);
            if !title.is_empty() {
                return title;
            }
        }

        if let Some(host) = parsed.host_str() {
            return host.to_string();
        }
    }

    normalize_whitespace(url)
}

pub(crate) fn decode_basic_html_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

pub(crate) fn joined_element_text(element: &ElementRef<'_>) -> String {
    element.text().collect::<Vec<_>>().join(" ")
}

pub(crate) fn normalized_element_text(element: &ElementRef<'_>) -> String {
    normalize_whitespace(&joined_element_text(element))
}

pub(crate) fn normalized_decoded_element_text(element: &ElementRef<'_>) -> String {
    normalize_multiline_text(&decode_basic_html_entities(&joined_element_text(element)))
}

pub(crate) fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn normalized_text_key(input: &str) -> String {
    normalize_whitespace(input).to_ascii_lowercase()
}

pub(crate) fn text_contains_any_marker(text: &str, markers: &[&str]) -> bool {
    let lowered = text.to_ascii_lowercase();
    markers.iter().any(|marker| lowered.contains(marker))
}

pub(crate) fn normalized_text_contains(haystack: &str, needle: &str) -> bool {
    normalized_text_key(haystack).contains(normalized_text_key(needle).as_str())
}

pub(crate) fn normalize_multiline_text(input: &str) -> String {
    let mut lines = Vec::new();
    let mut previous_blank = false;

    for raw_line in input.lines() {
        let line = normalize_whitespace(raw_line);
        if line.is_empty() {
            if !previous_blank && !lines.is_empty() {
                lines.push(String::new());
            }
            previous_blank = true;
        } else {
            lines.push(line);
            previous_blank = false;
        }
    }

    lines.join("\n").trim().to_string()
}

pub(crate) fn first_non_empty_owned(candidates: &[&str]) -> String {
    for candidate in candidates {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

pub(crate) fn push_unique_normalized_text(
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
    value: impl Into<String>,
) -> bool {
    let value = value.into();
    let normalized_key = normalized_text_key(&value);
    if normalized_key.is_empty() || !seen.insert(normalized_key) {
        return false;
    }
    out.push(value);
    true
}

pub(crate) fn js_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| {
        format!(
            "\"{}\"",
            value
                .chars()
                .flat_map(char::escape_default)
                .collect::<String>()
        )
    })
}

pub(crate) fn js_string_array(values: &[&str]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&js_string_literal(value));
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::{
        compile_regex, compile_selector, js_string_array, js_string_literal, replace_all_regex,
    };

    #[test]
    fn compile_regex_returns_none_for_invalid_pattern() {
        assert!(compile_regex("(", "broken regex").is_none());
    }

    #[test]
    fn compile_selector_returns_none_for_invalid_selector() {
        assert!(compile_selector("div[", "broken selector").is_none());
    }

    #[test]
    fn replace_all_regex_keeps_input_when_pattern_missing() {
        assert_eq!(replace_all_regex(None, "hello", "x"), "hello");
    }

    #[test]
    fn js_string_helpers_emit_json_compatible_literals() {
        assert_eq!(js_string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(js_string_array(&["a", "b"]), "[\"a\",\"b\"]");
    }
}

pub(crate) fn build_extract_summary(
    content: &str,
    max_extract_chars: usize,
) -> ExtractContentSummary {
    let total_chars = content.chars().count();
    let chunk_chars = max_extract_chars.max(1);
    let total_chunks = if total_chars == 0 {
        0
    } else {
        ((total_chars - 1) / chunk_chars) + 1
    };
    let sample_indexes = build_chunk_sample_indexes(total_chunks);

    let sampled_chunks = sample_indexes
        .into_iter()
        .map(|chunk_index| {
            let char_start = chunk_index * chunk_chars;
            let char_end = (char_start + chunk_chars).min(total_chars);
            let span = char_end.saturating_sub(char_start);
            let preview = truncate_chars(slice_chars(content, char_start, span).as_str(), 280);
            ExtractSummaryChunk {
                index: chunk_index,
                char_start,
                char_end,
                preview,
            }
        })
        .collect();

    ExtractContentSummary {
        strategy: "sampled_chunks".to_string(),
        total_chars,
        chunk_chars,
        total_chunks,
        sampled_chunks,
    }
}

fn build_chunk_sample_indexes(total_chunks: usize) -> Vec<usize> {
    if total_chunks == 0 {
        return Vec::new();
    }
    if total_chunks == 1 {
        return vec![0];
    }

    let mut indexes = vec![0, total_chunks / 2, total_chunks - 1];
    indexes.sort_unstable();
    indexes.dedup();
    indexes
}

fn slice_chars(input: &str, start: usize, len: usize) -> String {
    if len == 0 {
        return String::new();
    }
    input.chars().skip(start).take(len).collect()
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}
