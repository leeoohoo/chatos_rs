use serde_json::{json, Value};

use crate::builtin::web_tools::provider::{ExtractedPage, SearchHit};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SearchResultsBriefOptions {
    pub(crate) fallback_title_to_url: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SearchSummaryLineOptions {
    pub(crate) label_prefix: Option<&'static str>,
    pub(crate) fallback_title_to_url: bool,
    pub(crate) title_max_chars: usize,
    pub(crate) url_max_chars: usize,
    pub(crate) description_max_chars: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ExtractStatusStyle {
    Canonical,
    HumanReadable,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtractResultsBriefOptions {
    pub(crate) fallback_title_to_url: bool,
    pub(crate) status_style: ExtractStatusStyle,
    pub(crate) status_error_max_chars: usize,
    pub(crate) ok_preview_max_chars: usize,
    pub(crate) error_preview_max_chars: usize,
    pub(crate) blank_preview_on_error: bool,
    pub(crate) include_error_field: bool,
    pub(crate) include_stats_fields: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtractSummaryLineOptions {
    pub(crate) label_prefix: Option<&'static str>,
    pub(crate) fallback_title_to_url: bool,
    pub(crate) title_max_chars: usize,
    pub(crate) url_max_chars: usize,
    pub(crate) status_style: ExtractStatusStyle,
    pub(crate) status_error_max_chars: usize,
    pub(crate) ok_preview_max_chars: usize,
    pub(crate) error_preview_max_chars: usize,
}

pub(crate) fn build_search_summary_line(
    index: usize,
    hit: &SearchHit,
    options: SearchSummaryLineOptions,
) -> String {
    let title = normalize_inline_text(
        pick_display_title(
            hit.title.as_str(),
            hit.url.as_str(),
            options.fallback_title_to_url,
        ),
        options.title_max_chars,
    );
    let url = normalize_inline_text(hit.url.as_str(), options.url_max_chars);
    let description =
        normalize_inline_text(hit.description.as_str(), options.description_max_chars);
    let mut line = format!(
        "{}{} [{}]",
        summary_index_prefix(options.label_prefix, index),
        title,
        url,
    );
    if !description.is_empty() {
        line.push_str(format!(" - {}", description).as_str());
    }
    line
}

pub(crate) fn build_extract_summary_line(
    index: usize,
    page: &ExtractedPage,
    options: ExtractSummaryLineOptions,
) -> String {
    let title = normalize_inline_text(
        pick_display_title(
            page.title.as_str(),
            page.url.as_str(),
            options.fallback_title_to_url,
        ),
        options.title_max_chars,
    );
    let url = normalize_inline_text(page.url.as_str(), options.url_max_chars);
    let status = extract_status_label(page, options.status_style, options.status_error_max_chars);
    let preview = extract_preview_text(
        page,
        options.ok_preview_max_chars,
        options.error_preview_max_chars,
        false,
    );
    let mut line = format!(
        "{}{} [{}] - {}",
        summary_index_prefix(options.label_prefix, index),
        title,
        url,
        status,
    );
    if !preview.is_empty() {
        line.push_str(format!(" - {}", preview).as_str());
    }
    line
}

pub(crate) fn build_search_results_brief(
    hits: &[SearchHit],
    options: SearchResultsBriefOptions,
) -> Vec<Value> {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| {
            json!({
                "rank": index + 1,
                "title": normalize_inline_text(
                    pick_display_title(
                        hit.title.as_str(),
                        hit.url.as_str(),
                        options.fallback_title_to_url,
                    ),
                    120,
                ),
                "url": hit.url,
                "description_preview": normalize_inline_text(hit.description.as_str(), 180),
            })
        })
        .collect()
}

pub(crate) fn build_extract_results_brief(
    pages: &[ExtractedPage],
    options: ExtractResultsBriefOptions,
) -> Vec<Value> {
    let show_errors = !pages
        .iter()
        .any(|page| page.error.is_none() && !page.content.trim().is_empty());

    pages
        .iter()
        .filter(|page| show_errors || page.error.is_none())
        .enumerate()
        .map(|(index, page)| {
            let mut item = json!({
                "rank": index + 1,
                "title": normalize_inline_text(
                    pick_display_title(
                        page.title.as_str(),
                        page.url.as_str(),
                        options.fallback_title_to_url,
                    ),
                    120,
                ),
                "url": page.url,
                "status": extract_status_label(
                    page,
                    options.status_style,
                    options.status_error_max_chars,
                ),
                "content_preview": extract_preview_text(
                    page,
                    options.ok_preview_max_chars,
                    options.error_preview_max_chars,
                    options.blank_preview_on_error,
                ),
            });

            if let Some(map) = item.as_object_mut() {
                if options.include_error_field {
                    map.insert(
                        "error".to_string(),
                        page.error
                            .as_ref()
                            .map(|value| Value::String(value.clone()))
                            .unwrap_or(Value::Null),
                    );
                }
                if options.include_stats_fields {
                    map.insert(
                        "returned_chars".to_string(),
                        Value::from(page.content_chars as u64),
                    );
                    map.insert(
                        "original_chars".to_string(),
                        Value::from(page.original_content_chars as u64),
                    );
                    map.insert("truncated".to_string(), Value::Bool(page.truncated));
                }
            }

            item
        })
        .collect()
}

pub(crate) fn normalize_inline_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let total = collapsed.chars().count();
    if total <= max_chars {
        return collapsed;
    }
    let truncated: String = collapsed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect();
    format!("{}...", truncated)
}

pub(crate) fn first_non_empty<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
    let primary = primary.trim();
    if !primary.is_empty() {
        return primary;
    }
    fallback.trim()
}

fn pick_display_title<'a>(primary: &'a str, fallback: &'a str, fallback_enabled: bool) -> &'a str {
    if fallback_enabled {
        first_non_empty(primary, fallback)
    } else {
        primary
    }
}

fn summary_index_prefix(label_prefix: Option<&str>, index: usize) -> String {
    match label_prefix {
        Some(label) => format!("{} {}. ", label, index + 1),
        None => format!("{}. ", index + 1),
    }
}

fn extract_status_label(
    page: &ExtractedPage,
    style: ExtractStatusStyle,
    error_max_chars: usize,
) -> String {
    match style {
        ExtractStatusStyle::Canonical => {
            if page.error.is_some() {
                "error".to_string()
            } else if page.truncated {
                "truncated".to_string()
            } else {
                "ok".to_string()
            }
        }
        ExtractStatusStyle::HumanReadable => {
            if let Some(error) = page.error.as_deref() {
                format!("error: {}", normalize_inline_text(error, error_max_chars))
            } else if page.truncated {
                "ok, truncated".to_string()
            } else {
                "ok".to_string()
            }
        }
    }
}

fn extract_preview_text(
    page: &ExtractedPage,
    ok_max_chars: usize,
    error_max_chars: usize,
    blank_preview_on_error: bool,
) -> String {
    if let Some(error) = page.error.as_deref() {
        if blank_preview_on_error {
            String::new()
        } else {
            normalize_inline_text(error, error_max_chars)
        }
    } else {
        normalize_inline_text(page.content.as_str(), ok_max_chars)
    }
}
