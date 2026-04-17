use serde_json::{json, Value};

use super::provider::{
    extract_with_fallback, search_with_fallback, select_research_extract_urls,
    BrowserRenderOptions, ExtractedPage, SearchHit,
};
use super::BoundContext;

pub(super) async fn web_search_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
) -> Result<Value, String> {
    let search_limit = limit
        .unwrap_or(ctx.default_search_limit)
        .clamp(1, ctx.max_search_limit);
    let outcome = match search_with_fallback(
        &ctx.client,
        query.as_str(),
        search_limit,
        Some(&BrowserRenderOptions {
            workspace_dir: ctx.workspace_dir.clone(),
            command_timeout_seconds: ctx.browser_command_timeout_seconds,
        }),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            return Ok(json!({
                "_summary_text": format!(
                    "Web search for \"{}\" could not reach any search provider. {}",
                    normalize_inline_text(query.as_str(), 160),
                    normalize_inline_text(err.as_str(), 220),
                ),
                "success": true,
                "status": "degraded",
                "degraded": true,
                "warning": err,
                "query": query,
                "backend": "none",
                "fallback_used": false,
                "provider_attempts": [],
                "search_available": false,
                "result_count": 0,
                "results_brief": [],
                "error": err,
                "data": {
                    "web": []
                }
            }));
        }
    };
    let summary_text = build_search_summary(
        query.as_str(),
        outcome.backend.as_str(),
        outcome.fallback_used,
        outcome.attempts.len(),
        outcome.hits.as_slice(),
    );
    let results_brief = build_search_results_brief(outcome.hits.as_slice());
    Ok(json!({
        "_summary_text": summary_text,
        "success": true,
        "query": query,
        "backend": outcome.backend,
        "fallback_used": outcome.fallback_used,
        "provider_attempts": outcome.attempts,
        "result_count": outcome.hits.len(),
        "results_brief": results_brief,
        "data": {
            "web": outcome.hits
        }
    }))
}

pub(super) async fn web_extract_with_context(
    ctx: BoundContext,
    urls: Vec<String>,
) -> Result<Value, String> {
    let normalized = normalize_extract_urls(urls, ctx.max_extract_urls);

    if normalized.is_empty() {
        return Ok(json!({
            "_summary_text": "No valid URLs were provided, so nothing was extracted.",
            "success": true,
            "backend": "none",
            "fallback_used": false,
            "provider_attempts": [],
            "results_brief": [],
            "extract_summary": {
                "max_extract_chars_per_page": ctx.max_extract_chars,
                "page_count": 0,
                "truncated_page_count": 0,
                "total_original_chars": 0,
                "total_returned_chars": 0,
                "total_omitted_chars": 0
            },
            "results": []
        }));
    }

    let outcome = extract_with_fallback(
        &ctx.client,
        &normalized,
        ctx.max_extract_chars,
        Some(&BrowserRenderOptions {
            workspace_dir: ctx.workspace_dir.clone(),
            command_timeout_seconds: ctx.browser_command_timeout_seconds,
        }),
    )
    .await?;
    let page_count = outcome.pages.len();
    let truncated_page_count = outcome.pages.iter().filter(|page| page.truncated).count();
    let total_original_chars: usize = outcome
        .pages
        .iter()
        .map(|page| page.original_content_chars)
        .sum();
    let total_returned_chars: usize = outcome.pages.iter().map(|page| page.content_chars).sum();
    let total_omitted_chars = total_original_chars.saturating_sub(total_returned_chars);
    let summary_text = build_extract_summary(
        outcome.backend.as_str(),
        outcome.fallback_used,
        outcome.attempts.len(),
        outcome.pages.as_slice(),
        truncated_page_count,
        total_omitted_chars,
    );
    let results_brief = build_extract_results_brief(outcome.pages.as_slice());

    Ok(json!({
        "_summary_text": summary_text,
        "success": true,
        "backend": outcome.backend,
        "fallback_used": outcome.fallback_used,
        "provider_attempts": outcome.attempts,
        "results_brief": results_brief,
        "extract_summary": {
            "max_extract_chars_per_page": ctx.max_extract_chars,
            "page_count": page_count,
            "truncated_page_count": truncated_page_count,
            "total_original_chars": total_original_chars,
            "total_returned_chars": total_returned_chars,
            "total_omitted_chars": total_omitted_chars
        },
        "results": outcome.pages
    }))
}

pub(super) async fn web_research_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
    extract_top: Option<usize>,
) -> Result<Value, String> {
    let search_limit = limit
        .unwrap_or(ctx.default_search_limit)
        .clamp(1, ctx.max_search_limit);
    let search_outcome = match search_with_fallback(
        &ctx.client,
        query.as_str(),
        search_limit,
        Some(&BrowserRenderOptions {
            workspace_dir: ctx.workspace_dir.clone(),
            command_timeout_seconds: ctx.browser_command_timeout_seconds,
        }),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            let research_warning = err.clone();
            let mut response = json!({
                "_summary_text": format!(
                    "Web research for \"{}\" could not reach any search provider, so no external sources were gathered. {}",
                    normalize_inline_text(query.as_str(), 160),
                    normalize_inline_text(err.as_str(), 220),
                ),
                "success": true,
                "status": "degraded",
                "degraded": true,
                "warning": err,
                "query": query,
                "selected_urls": [],
                "research_warning": research_warning,
                "search_available": false,
                "research_summary": {
                    "search_result_count": 0,
                    "selected_url_count": 0,
                    "extracted_page_count": 0,
                    "search_backend": "none",
                    "search_fallback_used": false,
                    "extract_backend": "none",
                    "extract_fallback_used": false,
                    "truncated_page_count": 0,
                    "total_original_chars": 0,
                    "total_returned_chars": 0,
                    "total_omitted_chars": 0,
                    "warning": err,
                },
                "search": {
                    "backend": "none",
                    "fallback_used": false,
                    "provider_attempts": [],
                    "result_count": 0,
                    "results_brief": [],
                    "data": {
                        "web": []
                    }
                },
                "extract": {
                    "backend": "none",
                    "fallback_used": false,
                    "provider_attempts": [],
                    "results_brief": [],
                    "extract_summary": {
                        "max_extract_chars_per_page": ctx.max_extract_chars,
                        "page_count": 0,
                        "truncated_page_count": 0,
                        "total_original_chars": 0,
                        "total_returned_chars": 0,
                        "total_omitted_chars": 0
                    },
                    "results": []
                }
            });
            response["research_findings"] = build_web_research_findings(&response);
            return Ok(response);
        }
    };
    let search_results_brief = build_search_results_brief(search_outcome.hits.as_slice());

    let desired_extract_count = extract_top
        .unwrap_or(search_limit.min(3))
        .min(ctx.max_extract_urls);
    let selected_urls = select_research_extract_urls(
        search_outcome.hits.as_slice(),
        desired_extract_count,
        ctx.max_extract_urls,
    );

    let mut extract_backend = "none".to_string();
    let mut extract_fallback_used = false;
    let mut extract_attempts = Vec::new();
    let mut extract_pages: Vec<ExtractedPage> = Vec::new();
    let mut extract_warning: Option<String> = None;

    if selected_urls.is_empty() {
        if desired_extract_count == 0 {
            extract_warning =
                Some("Extraction was skipped because extract_top was set to 0.".to_string());
        } else if search_outcome.hits.is_empty() {
            extract_warning =
                Some("No search hits were returned, so nothing was extracted.".to_string());
        }
    } else {
        match extract_with_fallback(
            &ctx.client,
            &selected_urls,
            ctx.max_extract_chars,
            Some(&BrowserRenderOptions {
                workspace_dir: ctx.workspace_dir.clone(),
                command_timeout_seconds: ctx.browser_command_timeout_seconds,
            }),
        )
        .await
        {
            Ok(outcome) => {
                extract_backend = outcome.backend;
                extract_fallback_used = outcome.fallback_used;
                extract_attempts = outcome.attempts;
                extract_pages = outcome.pages;
            }
            Err(err) => {
                extract_warning = Some(err);
            }
        }
    }

    let page_count = extract_pages.len();
    let truncated_page_count = extract_pages.iter().filter(|page| page.truncated).count();
    let total_original_chars: usize = extract_pages
        .iter()
        .map(|page| page.original_content_chars)
        .sum();
    let total_returned_chars: usize = extract_pages.iter().map(|page| page.content_chars).sum();
    let total_omitted_chars = total_original_chars.saturating_sub(total_returned_chars);
    let extract_results_brief = build_extract_results_brief(extract_pages.as_slice());
    let summary_text = build_research_summary(
        query.as_str(),
        search_outcome.backend.as_str(),
        search_outcome.fallback_used,
        search_outcome.attempts.len(),
        search_outcome.hits.as_slice(),
        extract_backend.as_str(),
        extract_fallback_used,
        extract_attempts.len(),
        extract_pages.as_slice(),
        selected_urls.len(),
        truncated_page_count,
        total_omitted_chars,
        extract_warning.as_deref(),
    );

    let mut response = json!({
        "_summary_text": summary_text,
        "success": true,
        "query": query,
        "selected_urls": selected_urls,
        "research_summary": {
            "search_result_count": search_outcome.hits.len(),
            "selected_url_count": selected_urls.len(),
            "extracted_page_count": page_count,
            "search_backend": search_outcome.backend,
            "search_fallback_used": search_outcome.fallback_used,
            "extract_backend": extract_backend,
            "extract_fallback_used": extract_fallback_used,
            "truncated_page_count": truncated_page_count,
            "total_original_chars": total_original_chars,
            "total_returned_chars": total_returned_chars,
            "total_omitted_chars": total_omitted_chars,
            "warning": extract_warning,
        },
        "search": {
            "backend": search_outcome.backend,
            "fallback_used": search_outcome.fallback_used,
            "provider_attempts": search_outcome.attempts,
            "result_count": search_outcome.hits.len(),
            "results_brief": search_results_brief,
            "data": {
                "web": search_outcome.hits
            }
        },
        "extract": {
            "backend": extract_backend,
            "fallback_used": extract_fallback_used,
            "provider_attempts": extract_attempts,
            "results_brief": extract_results_brief,
            "extract_summary": {
                "max_extract_chars_per_page": ctx.max_extract_chars,
                "page_count": page_count,
                "truncated_page_count": truncated_page_count,
                "total_original_chars": total_original_chars,
                "total_returned_chars": total_returned_chars,
                "total_omitted_chars": total_omitted_chars
            },
            "results": extract_pages
        }
    });
    response["research_findings"] = build_web_research_findings(&response);
    Ok(response)
}

fn normalize_extract_urls(urls: Vec<String>, max_extract_urls: usize) -> Vec<String> {
    let mut normalized = Vec::new();
    for raw in urls {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if normalized.iter().any(|value: &String| value == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
        if normalized.len() >= max_extract_urls {
            break;
        }
    }
    normalized
}

fn build_search_summary(
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
        let title = first_non_empty(hit.title.as_str(), hit.url.as_str());
        let description = normalize_inline_text(hit.description.as_str(), 180);
        let mut line = format!(
            "{}. {} [{}]",
            index + 1,
            normalize_inline_text(title, 120),
            normalize_inline_text(hit.url.as_str(), 140),
        );
        if !description.is_empty() {
            line.push_str(format!(" - {}", description).as_str());
        }
        lines.push(line);
    }

    lines.join("\n")
}

fn build_search_results_brief(hits: &[SearchHit]) -> Vec<Value> {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| {
            json!({
                "rank": index + 1,
                "title": normalize_inline_text(hit.title.as_str(), 120),
                "url": hit.url,
                "description_preview": normalize_inline_text(hit.description.as_str(), 180),
            })
        })
        .collect()
}

fn build_research_summary(
    query: &str,
    _search_backend: &str,
    _search_fallback_used: bool,
    _search_attempt_count: usize,
    hits: &[SearchHit],
    _extract_backend: &str,
    _extract_fallback_used: bool,
    _extract_attempt_count: usize,
    pages: &[ExtractedPage],
    selected_url_count: usize,
    truncated_page_count: usize,
    total_omitted_chars: usize,
    extract_warning: Option<&str>,
) -> String {
    let mut lines = vec![format!(
        "Web research for \"{}\" found {} result(s) and extracted {} page(s) (selected URLs: {}, truncated pages: {}, omitted chars: {}).",
        normalize_inline_text(query, 160),
        hits.len(),
        pages.len(),
        selected_url_count,
        truncated_page_count,
        total_omitted_chars,
    )];

    if hits.is_empty() {
        lines.push("No search hits were returned.".to_string());
    } else {
        for (index, hit) in hits.iter().take(3).enumerate() {
            let title = first_non_empty(hit.title.as_str(), hit.url.as_str());
            let description = normalize_inline_text(hit.description.as_str(), 160);
            let mut line = format!(
                "search {}. {} [{}]",
                index + 1,
                normalize_inline_text(title, 120),
                normalize_inline_text(hit.url.as_str(), 140),
            );
            if !description.is_empty() {
                line.push_str(format!(" - {}", description).as_str());
            }
            lines.push(line);
        }
    }

    if pages.is_empty() {
        if selected_url_count > 0 {
            lines.push("No pages were extracted.".to_string());
        }
    } else {
        for (index, page) in pages.iter().take(3).enumerate() {
            let title = first_non_empty(page.title.as_str(), page.url.as_str());
            let status = if let Some(error) = page.error.as_deref() {
                format!("error: {}", normalize_inline_text(error, 120))
            } else if page.truncated {
                "ok, truncated".to_string()
            } else {
                "ok".to_string()
            };
            let preview = if let Some(error) = page.error.as_deref() {
                normalize_inline_text(error, 160)
            } else {
                normalize_inline_text(page.content.as_str(), 180)
            };
            let mut line = format!(
                "extract {}. {} [{}] - {}",
                index + 1,
                normalize_inline_text(title, 120),
                normalize_inline_text(page.url.as_str(), 140),
                status,
            );
            if !preview.is_empty() {
                line.push_str(format!(" - {}", preview).as_str());
            }
            lines.push(line);
        }
    }

    if let Some(warning) = extract_warning
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!(
            "Research warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    lines.join("\n")
}

fn build_web_research_findings(response: &Value) -> Value {
    let query = response
        .get("query")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 160))
        .filter(|value| !value.is_empty());
    let research_summary = response
        .get("research_summary")
        .and_then(|value| value.as_object());
    let search_count = research_summary
        .and_then(|value| value.get("search_result_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let extract_count = research_summary
        .and_then(|value| value.get("extracted_page_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let selected_url_count = research_summary
        .and_then(|value| value.get("selected_url_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let truncated_page_count = research_summary
        .and_then(|value| value.get("truncated_page_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let total_omitted_chars = research_summary
        .and_then(|value| value.get("total_omitted_chars"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let answer_frame = format!(
        "Web research{} found {} result(s) and extracted {} page(s).",
        query
            .as_deref()
            .map(|value| format!(" for \"{}\"", value))
            .unwrap_or_default(),
        search_count,
        extract_count,
    );

    let mut web_findings = Vec::new();
    push_unique_text(
        &mut web_findings,
        format!("Search returned {} result(s).", search_count),
    );
    let search_titles = response
        .get("search")
        .and_then(|value| value.get("results_brief"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("title")
                        .and_then(|value| value.as_str())
                        .map(|value| normalize_inline_text(value, 100))
                        .filter(|value| !value.is_empty())
                })
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !search_titles.is_empty() {
        push_unique_text(
            &mut web_findings,
            format!("Top search hits: {}.", search_titles.join(" | ")),
        );
    }
    push_unique_text(
        &mut web_findings,
        format!(
            "Extraction reviewed {} selected URL(s) and returned {} page(s); truncated pages: {}, omitted chars: {}.",
            selected_url_count,
            extract_count,
            truncated_page_count,
            total_omitted_chars,
        ),
    );
    let extracted_titles = response
        .get("extract")
        .and_then(|value| value.get("results_brief"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let title = item
                        .get("title")
                        .and_then(|value| value.as_str())
                        .map(|value| normalize_inline_text(value, 90))
                        .filter(|value| !value.is_empty())?;
                    let status = item
                        .get("status")
                        .and_then(|value| value.as_str())
                        .map(|value| normalize_inline_text(value, 60))
                        .unwrap_or_else(|| "unknown".to_string());
                    Some(format!("{} ({})", title, status))
                })
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !extracted_titles.is_empty() {
        push_unique_text(
            &mut web_findings,
            format!("Key extracted sources: {}.", extracted_titles.join(" | ")),
        );
    }
    if let Some(warning) = research_summary
        .and_then(|value| value.get("warning"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut web_findings,
            format!("Research warning: {}.", normalize_inline_text(warning, 180)),
        );
    }

    let source_highlights = build_research_source_highlights(
        response
            .get("extract")
            .and_then(|value| value.get("results_brief"))
            .and_then(|value| value.as_array()),
        response
            .get("search")
            .and_then(|value| value.get("results_brief"))
            .and_then(|value| value.as_array()),
    );

    let mut recommended_next_steps = Vec::new();
    if search_count == 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Refine the query with specific product, site, or date terms so web_search can surface more relevant hits.".to_string(),
        );
    } else if extract_count == 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Use web_extract on selected_urls or increase extract_top when you need source text instead of search snippets.".to_string(),
        );
    }
    if truncated_page_count > 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Re-run extraction on the most relevant URL if one source needs deeper reading with less truncation.".to_string(),
        );
    }
    if response
        .get("research_summary")
        .and_then(|value| value.get("warning"))
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        push_unique_text(
            &mut recommended_next_steps,
            "Check the warning field because this research bundle used a degraded extraction path."
                .to_string(),
        );
    }
    push_unique_text(
        &mut recommended_next_steps,
        "If the question depends on the page currently open in the browser, prefer browser_research so page context and web sources are combined in one run.".to_string(),
    );

    json!({
        "answer_frame": answer_frame,
        "page_findings": [],
        "web_findings": web_findings,
        "source_highlights": source_highlights,
        "recommended_next_steps": recommended_next_steps,
    })
}

fn build_research_source_highlights(
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

fn push_unique_text(values: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if values.iter().any(|existing| existing == trimmed) {
        return;
    }
    values.push(trimmed.to_string());
}

fn build_extract_summary(
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
        let title = first_non_empty(page.title.as_str(), page.url.as_str());
        let status = if let Some(error) = page.error.as_deref() {
            format!("error: {}", normalize_inline_text(error, 120))
        } else if page.truncated {
            "ok, truncated".to_string()
        } else {
            "ok".to_string()
        };
        let preview = if let Some(error) = page.error.as_deref() {
            normalize_inline_text(error, 180)
        } else {
            normalize_inline_text(page.content.as_str(), 220)
        };
        let mut line = format!(
            "{}. {} [{}] - {}",
            index + 1,
            normalize_inline_text(title, 120),
            normalize_inline_text(page.url.as_str(), 140),
            status,
        );
        if !preview.is_empty() {
            line.push_str(format!(" - {}", preview).as_str());
        }
        lines.push(line);
    }

    lines.join("\n")
}

fn build_extract_results_brief(pages: &[ExtractedPage]) -> Vec<Value> {
    let show_errors = !pages
        .iter()
        .any(|page| page.error.is_none() && !page.content.trim().is_empty());

    pages
        .iter()
        .filter(|page| show_errors || page.error.is_none())
        .enumerate()
        .map(|(index, page)| {
            json!({
                "rank": index + 1,
                "title": normalize_inline_text(page.title.as_str(), 120),
                "url": page.url,
                "status": if page.error.is_some() {
                    "error"
                } else if page.truncated {
                    "truncated"
                } else {
                    "ok"
                },
                "error": page.error,
                "content_preview": if page.error.is_some() {
                    String::new()
                } else {
                    normalize_inline_text(page.content.as_str(), 220)
                },
                "returned_chars": page.content_chars,
                "original_chars": page.original_content_chars,
                "truncated": page.truncated,
            })
        })
        .collect()
}

fn normalize_inline_text(text: &str, max_chars: usize) -> String {
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

fn first_non_empty<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
    let primary = primary.trim();
    if !primary.is_empty() {
        return primary;
    }
    fallback.trim()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_extract_results_brief, build_research_summary, build_web_research_findings,
        normalize_extract_urls, ExtractedPage, SearchHit,
    };

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
    fn research_summary_mentions_search_extract_and_warning() {
        let summary = build_research_summary(
            "rust browser automation",
            "chatos_native_search",
            false,
            1,
            &[SearchHit {
                url: "https://example.com/rust".to_string(),
                title: "Rust Browser Guide".to_string(),
                description: "A practical browser automation guide.".to_string(),
            }],
            "none",
            false,
            0,
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
